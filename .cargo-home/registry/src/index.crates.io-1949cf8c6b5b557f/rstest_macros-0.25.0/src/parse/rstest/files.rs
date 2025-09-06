use std::{env, path::PathBuf};

use crate::{
    error::ErrorsVec,
    parse::{
        extract_argument_attrs,
        vlist::{Value, ValueList},
    },
    refident::{IntoPat, MaybeIdent},
    utils::attr_is,
};
use glob::glob;
use quote::ToTokens;
use regex::Regex;
use relative_path::RelativePath;
use syn::{
    parse::Parse, parse_quote, visit_mut::VisitMut, Attribute, Expr, FnArg, Ident, ItemFn, LitStr,
    MetaNameValue,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FilesGlobReferences {
    base_dir: Option<LitStrAttr>,
    glob: Vec<LitStrAttr>,
    exclude: Vec<Exclude>,
    ignore_dot_files: bool,
    ignore_missing_env_vars: bool,
    files_mode: FilesMode,
}

impl FilesGlobReferences {
    /// Replace environment variables in a string.
    fn replace_env_vars(
        &self,
        attr: &LitStrAttr,
        env: impl EnvironmentResolver,
    ) -> Result<String, syn::Error> {
        let re = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]+)|\$\{([^}]+)}")
            .expect("Could not build the regex");

        let ignore_missing = self.ignore_missing_env_vars;

        let path = attr.value();
        let haystack = path.as_str();
        let mut result = String::with_capacity(attr.value().len());
        let mut last_match = 0;

        for caps in re.captures_iter(haystack) {
            let match_all = caps.get(0).expect("The regex should have matched");

            // Match the name of the variable and its default value (if any).
            let (var_name, default) = if let Some(m) = caps.get(1) {
                // In the first case ($VAR), the default value is None.
                (m.as_str(), None)
            } else {
                // In the second case we have to split the variable name and the default value
                // if there's a `:-` separator.
                let m = caps.get(2).expect("The regex should have matched");

                let (var_name, default) =
                    if let Some((var_name, default)) = m.as_str().split_once(":-") {
                        (var_name, Some(default))
                    } else {
                        (m.as_str(), None)
                    };

                var_name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_')
                    .then_some((var_name, default))
                    .ok_or_else(|| {
                        attr.error(&format!(
                            "The variable \"{}\" name does not match [a-zA-Z0-9_]+",
                            m.as_str()
                        ))
                    })?
            };

            let v = env.get(var_name);
            let replacement = match (v.as_deref(), default) {
                (Some(""), Some(default)) => default.to_string(),
                (Some(val), _) => val.to_string(),
                (None, Some(default)) => default.to_string(),
                (None, _) if ignore_missing => String::new(),
                (None, _) => {
                    return Err(attr.error(&format!(
                        "Could not find the environment variable {:?}",
                        var_name
                    )))
                }
            };

            result.push_str(&haystack[last_match..match_all.start()]);
            result.push_str(&replacement);
            last_match = match_all.end();
        }

        result.push_str(&haystack[last_match..]);

        Ok(result)
    }

    /// Return the base_dir attribute if set, with variables replaced.
    fn base_dir(&self) -> Result<Option<PathBuf>, syn::Error> {
        self.base_dir
            .as_ref()
            .map(|attr| {
                self.replace_env_vars(attr, StdEnvironmentResolver)
                    .map(PathBuf::from)
            })
            .transpose()
    }

    /// Return the tuples attribute, path string if they are valid relative paths
    fn paths(&self, base_dir: &PathBuf) -> Result<Vec<(&LitStrAttr, String)>, syn::Error> {
        self.glob
            .iter()
            .map(|attr| {
                let path = self.replace_env_vars(attr, StdEnvironmentResolver)?;
                RelativePath::from_path(&path)
                    .map_err(|e| attr.error(&format!("Invalid glob path: {e}")))
                    .map(|p| p.to_logical_path(base_dir))
                    .map(|p| (attr, p.to_string_lossy().into_owned()))
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FilesMode {
    #[default]
    Path,
    IncludeStr,
    IncludeBytes,
}

pub(crate) mod files_mode_keywords {
    syn::custom_keyword!(path);
    syn::custom_keyword!(str);
    syn::custom_keyword!(bytes);
}

impl Parse for FilesMode {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if Option::<files_mode_keywords::path>::parse(input)?.is_some() {
            Ok(Self::Path)
        } else if Option::<files_mode_keywords::str>::parse(input)?.is_some() {
            Ok(Self::IncludeStr)
        } else if Option::<files_mode_keywords::bytes>::parse(input)?.is_some() {
            Ok(Self::IncludeBytes)
        } else {
            Err(input.error("Expected one of the following keywords: path, str or bytes"))
        }
    }
}

trait RaiseError: ToTokens {
    fn error(&self, msg: &str) -> syn::Error {
        syn::Error::new_spanned(self, msg)
    }
}

impl RaiseError for Attribute {}
impl RaiseError for LitStrAttr {}
impl ToTokens for LitStrAttr {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.attr.to_tokens(tokens)
    }
}

impl FilesGlobReferences {
    fn new(
        glob: Vec<LitStrAttr>,
        exclude: Vec<Exclude>,
        ignore_dot_files: bool,
        ignore_missing_env_vars: bool,
        files_mode: FilesMode,
    ) -> Self {
        Self {
            glob,
            base_dir: None,
            exclude,
            ignore_dot_files,
            ignore_missing_env_vars,
            files_mode,
        }
    }

    fn with_base_dir_opt(mut self, base_dir: Option<LitStrAttr>) -> Self {
        self.base_dir = base_dir;
        self
    }

    fn is_valid(&self, p: &RelativePath) -> bool {
        if self.ignore_dot_files
            && p.components()
                .any(|c| matches!(c, relative_path::Component::Normal(c) if c.starts_with('.')))
        {
            return false;
        }
        !self.exclude.iter().any(|e| e.r.is_match(p.as_ref()))
    }
}

/// An attribute in the form `#[name("some string")]`
#[derive(Debug, Clone, PartialEq)]
struct LitStrAttr {
    attr: Attribute,
    value: LitStr,
}

impl LitStrAttr {
    fn parse_meta_name_value(attr: &Attribute) -> Result<Self, syn::Error> {
        let meta = attr.meta.require_name_value()?;
        if let syn::Expr::Lit(expr) = &meta.value {
            if let syn::Lit::Str(value) = expr.lit.clone() {
                return Ok(Self {
                    attr: attr.clone(),
                    value,
                });
            }
        }

        Err(syn::Error::new_spanned(
            attr,
            "expected a string literal value",
        ))
    }

    fn value(&self) -> String {
        self.value.value()
    }
}

impl TryFrom<Attribute> for LitStrAttr {
    type Error = syn::Error;

    fn try_from(attr: Attribute) -> Result<Self, Self::Error> {
        let value = attr.parse_args::<LitStr>()?;
        Ok(Self { attr, value })
    }
}

/// The `#[exclude("regex")]` attribute
#[derive(Debug, Clone)]
struct Exclude {
    attr: LitStrAttr,
    r: Regex,
}

impl PartialEq for Exclude {
    fn eq(&self, other: &Self) -> bool {
        self.attr.value == other.attr.value
    }
}

impl TryFrom<Attribute> for Exclude {
    type Error = syn::Error;

    fn try_from(attr: Attribute) -> Result<Self, Self::Error> {
        let attr: LitStrAttr = attr.try_into()?;
        let r = Regex::new(&attr.value()).map_err(|e| {
            syn::Error::new_spanned(
                &attr,
                format!(r#""{}" Should be a valid regex: {e}"#, attr.value()),
            )
        })?;
        Ok(Self { attr, r })
    }
}

impl From<Vec<LitStrAttr>> for FilesGlobReferences {
    fn from(value: Vec<LitStrAttr>) -> Self {
        Self::new(value, Default::default(), true, false, Default::default())
    }
}

/// Entry point function to extract files attributes
pub(crate) fn extract_files(
    item_fn: &mut ItemFn,
) -> Result<Vec<(Ident, FilesGlobReferences)>, ErrorsVec> {
    let mut extractor = ValueFilesExtractor::default();
    extractor.visit_item_fn_mut(item_fn);
    extractor.take()
}

/// Simple struct used to visit function attributes and extract future args to
/// implement the boilerplate.
#[derive(Default)]
struct ValueFilesExtractor {
    files: Vec<(Ident, FilesGlobReferences)>,
    errors: Vec<syn::Error>,
}

impl ValueFilesExtractor {
    pub(crate) fn take(self) -> Result<Vec<(Ident, FilesGlobReferences)>, ErrorsVec> {
        if self.errors.is_empty() {
            Ok(self.files)
        } else {
            Err(self.errors.into())
        }
    }

    fn collect_errors<T: Default>(&mut self, result: Result<T, syn::Error>) -> T {
        match result {
            Ok(v) => v,
            Err(e) => {
                self.errors.push(e);
                T::default()
            }
        }
    }

    fn extract_argument_attrs<'a, B: 'a + std::fmt::Debug>(
        &mut self,
        node: &mut FnArg,
        is_valid_attr: fn(&syn::Attribute) -> bool,
        build: impl Fn(syn::Attribute) -> syn::Result<B> + 'a,
    ) -> Vec<B> {
        self.collect_errors(
            extract_argument_attrs(node, is_valid_attr, build).collect::<Result<Vec<_>, _>>(),
        )
    }

    fn extract_files(&mut self, node: &mut FnArg) -> Vec<LitStrAttr> {
        self.extract_argument_attrs(node, |a| attr_is(a, "files"), |attr| attr.try_into())
    }

    fn extract_exclude(&mut self, node: &mut FnArg) -> Vec<Exclude> {
        self.extract_argument_attrs(node, |a| attr_is(a, "exclude"), Exclude::try_from)
    }

    fn extract_include_dot_files(&mut self, node: &mut FnArg) -> Vec<Attribute> {
        self.extract_argument_attrs(
            node,
            |a| attr_is(a, "include_dot_files"),
            |attr| {
                attr.meta
                    .require_path_only()
                    .map_err(|_| attr.error("Use #[include_dot_files] to include dot files"))?;
                Ok(attr)
            },
        )
    }

    fn extract_ignore_missing_env_vars(&mut self, node: &mut FnArg) -> Vec<Attribute> {
        self.extract_argument_attrs(
            node,
            |a| attr_is(a, "ignore_missing_env_vars"),
            |attr| {
                attr.meta.require_path_only().map_err(|_| {
                    attr.error(
                        "Use #[ignore_missing_env_vars] to ignore missing environment variables",
                    )
                })?;
                Ok(attr)
            },
        )
    }

    fn extract_base_dir(&mut self, node: &mut FnArg) -> Vec<LitStrAttr> {
        self.extract_argument_attrs(
            node,
            |a| attr_is(a, "base_dir"),
            |attr| {
                LitStrAttr::parse_meta_name_value(&attr).map_err(|_| {
                    attr.error(
                        "Use #[base_dir = \"...\"] to define the base directory for the glob path",
                    )
                })
            },
        )
    }

    fn extract_mode(&mut self, node: &mut FnArg) -> Vec<(MetaNameValue, FilesMode)> {
        self.extract_argument_attrs(
            node,
            |a| attr_is(a, "mode"),
            |attr| {
                attr.meta
                    .require_name_value()
                    .map_err(|_| {
                        attr.error("Use #[mode = ...] to define the argument of the file input")
                    })
                    .and_then(|attr| {
                        syn::parse2(attr.value.to_token_stream())
                            .map(|file_mode| (attr.clone(), file_mode))
                    })
            },
        )
    }
}

impl VisitMut for ValueFilesExtractor {
    fn visit_fn_arg_mut(&mut self, node: &mut FnArg) {
        let name = node.maybe_ident().cloned();
        if matches!(node, FnArg::Receiver(_)) || name.is_none() {
            return;
        }
        let name = name.unwrap();
        let files = self.extract_files(node);
        let excludes = self.extract_exclude(node);
        let include_dot_files = self.extract_include_dot_files(node);
        if !include_dot_files.is_empty() {
            include_dot_files.iter().skip(1).for_each(|attr| {
                self.errors
                    .push(attr.error("Cannot use #[include_dot_files] more than once"))
            })
        }
        let ignore_missing_env_vars = self.extract_ignore_missing_env_vars(node);
        if !ignore_missing_env_vars.is_empty() {
            ignore_missing_env_vars.iter().skip(1).for_each(|attr| {
                self.errors
                    .push(attr.error("Cannot use #[ignore_missing_env_vars] more than once"))
            })
        }
        let base_dir = self.extract_base_dir(node);
        if base_dir.len() > 1 {
            base_dir.iter().skip(1).for_each(|attr| {
                self.errors
                    .push(attr.error(r#"Cannot use #[base_dir = "..."] more than once"#))
            })
        }
        let mode_attr = self.extract_mode(node);
        let mode = if let Some(value) = mode_attr.first() {
            mode_attr.iter().skip(1).for_each(|attr| {
                self.errors.push(syn::Error::new_spanned(
                    &attr.0,
                    r#"Cannot use #[mode = ...] more than once"#,
                ))
            });
            value.1
        } else {
            Default::default()
        };
        if !files.is_empty() {
            self.files.push((
                name,
                FilesGlobReferences::new(
                    files,
                    excludes,
                    include_dot_files.is_empty(),
                    !ignore_missing_env_vars.is_empty(),
                    mode,
                )
                .with_base_dir_opt(base_dir.into_iter().next()),
            ))
        } else {
            excludes.into_iter().for_each(|e| {
                self.errors.push(
                    e.attr
                        .error("You cannot use #[exclude(...)] without #[files(...)]"),
                )
            });
            include_dot_files.into_iter().for_each(|attr| {
                self.errors
                    .push(attr.error("You cannot use #[include_dot_files] without #[files(...)]"))
            });
            ignore_missing_env_vars.into_iter().for_each(|attr| {
                self.errors.push(
                    attr.error("You cannot use #[ignore_missing_env_vars] without #[files(...)]"),
                )
            });
            base_dir.into_iter().for_each(|attr| {
                self.errors
                    .push(attr.error(r#"You cannot use #[base_dir = "..."] without #[files(...)]"#))
            });
        }
    }
}

trait BaseDir {
    fn base_dir(&self) -> Result<PathBuf, String> {
        env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .map_err(|_|
                "Rstest's #[files(...)] requires that CARGO_MANIFEST_DIR is defined to define glob the relative path".to_string()
            )
    }
}

struct DefaultBaseDir;

impl BaseDir for DefaultBaseDir {}

trait EnvironmentResolver {
    fn get(&self, var: &str) -> Option<String>;
}

struct StdEnvironmentResolver;

impl EnvironmentResolver for StdEnvironmentResolver {
    fn get(&self, var: &str) -> Option<String> {
        env::var(var).ok()
    }
}

trait GlobResolver {
    fn glob(&self, pattern: &str) -> Result<Vec<PathBuf>, String> {
        let globs =
            glob(pattern).map_err(|e| format!("glob failed for whole path `{pattern}` due {e}"))?;
        globs
            .into_iter()
            .map(|p| p.map_err(|e| format!("glob failed for file due {e}")))
            .map(|r| {
                r.and_then(|p| {
                    p.canonicalize()
                        .map_err(|e| format!("failed to canonicalize {} due {e}", p.display()))
                })
            })
            .collect()
    }
}

struct DefaultGlobResolver;

impl GlobResolver for DefaultGlobResolver {}

/// The struct used to gel te values from the files attributes. You can inject
/// the base dir resolver and glob resolver implementation.
pub(crate) struct ValueListFromFiles<'a> {
    base_dir: Box<dyn BaseDir + 'a>,
    g_resolver: Box<dyn GlobResolver + 'a>,
}

impl Default for ValueListFromFiles<'_> {
    fn default() -> Self {
        Self {
            g_resolver: Box::new(DefaultGlobResolver),
            base_dir: Box::new(DefaultBaseDir),
        }
    }
}

impl ValueListFromFiles<'_> {
    pub fn to_value_list(
        &self,
        files: Vec<(Ident, FilesGlobReferences)>,
    ) -> Result<Vec<ValueList>, syn::Error> {
        files
            .into_iter()
            .map(|(arg, refs)| {
                self.file_list_values(refs).map(|values| ValueList {
                    arg: arg.into_pat(),
                    values,
                })
            })
            .collect::<Result<Vec<ValueList>, _>>()
    }

    fn file_list_values(&self, refs: FilesGlobReferences) -> Result<Vec<Value>, syn::Error> {
        let default_base_dir = self
            .base_dir
            .base_dir()
            .map_err(|msg| refs.glob[0].error(&msg))?;

        let base_dir = match refs.base_dir()? {
            Some(mut p) => {
                if p.is_relative() {
                    p = default_base_dir.join(p);
                }
                p.canonicalize().map_err(|e| {
                    refs.glob[0].error(&format!("Cannot canonicalize base dir due {e}"))
                })?
            }
            None => default_base_dir,
        };

        let resolved_paths = refs.paths(&base_dir)?;
        let base_dir = base_dir
            .into_os_string()
            .into_string()
            .map_err(|p| refs.glob[0].error(&format!("Cannot get a valid string from {p:?}")))?;

        let mut values: Vec<(Expr, String)> = vec![];
        for (attr, abs_path) in self.all_files_path(resolved_paths)? {
            let relative_path = abs_path
                .clone()
                .into_os_string()
                .into_string()
                .map(|inner| RelativePath::new(base_dir.as_str()).relative(inner))
                .map_err(|e| {
                    attr.error(&format!("Invalid absolute path {}", e.to_string_lossy()))
                })?;

            if !refs.is_valid(&relative_path) {
                continue;
            }

            let path_str = abs_path.to_string_lossy();
            let value = match refs.files_mode {
                FilesMode::Path => parse_quote! {
                    <::std::path::PathBuf as std::str::FromStr>::from_str(#path_str).unwrap()
                },
                FilesMode::IncludeStr => parse_quote! {
                    include_str!(#path_str)
                },
                FilesMode::IncludeBytes => parse_quote! {
                    include_bytes!(#path_str)
                },
            };
            values.push((value, render_file_description(&relative_path)));
        }

        if values.is_empty() {
            Err(refs.glob[0].error("No file found"))?;
        }

        Ok(values
            .into_iter()
            .map(|(e, desc)| Value::new(e, Some(desc)))
            .collect())
    }

    /// Return the tuples of attribute, file path resolved via glob resolver, sorted by path and without duplications.
    fn all_files_path<'b>(
        &self,
        resolved_paths: Vec<(&'b LitStrAttr, String)>,
    ) -> Result<Vec<(&'b LitStrAttr, PathBuf)>, syn::Error> {
        let mut paths = resolved_paths
            .iter()
            .map(|(attr, pattern)| {
                self.g_resolver
                    .glob(pattern.as_ref())
                    .map_err(|msg| attr.error(&msg))
                    .map(|p| (attr, p))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|(&attr, inner)| inner.into_iter().map(move |p| (attr, p)))
            .collect::<Vec<_>>();
        paths.sort_by(|(_, a), (_, b)| a.cmp(b));
        paths.dedup_by(|(_, a), (_, b)| a.eq(&b));
        Ok(paths)
    }
}

fn render_file_description(file: &RelativePath) -> String {
    let mut description = String::new();
    for c in file.components() {
        match c {
            relative_path::Component::CurDir => continue,
            relative_path::Component::ParentDir => description.push_str("_UP"),
            relative_path::Component::Normal(segment) => description.push_str(segment),
        }
        description.push('/')
    }
    description.pop();
    description
}

#[cfg(test)]
mod should {
    use std::collections::HashMap;

    use super::*;
    use crate::test::{assert_eq, *};
    use maplit::hashmap;
    use rstest_test::assert_in;

    fn lit_str_attr(name: &str, value: impl AsRef<str>) -> LitStrAttr {
        attrs(&format!(r#"#[{name}("{}")]"#, value.as_ref()))
            .into_iter()
            .next()
            .unwrap()
            .try_into()
            .unwrap()
    }

    fn name_value_attr(name: &str, value: impl AsRef<str>) -> LitStrAttr {
        LitStrAttr::parse_meta_name_value(
            &attrs(&format!(r#"#[{name} = "{}"]"#, value.as_ref()))
                .into_iter()
                .next()
                .unwrap(),
        )
        .expect("Could not parse attribute")
    }

    fn files_attr(lstr: impl AsRef<str>) -> LitStrAttr {
        lit_str_attr("files", lstr)
    }

    fn base_dir_attr(lstr: impl AsRef<str>) -> LitStrAttr {
        name_value_attr("base_dir", lstr)
    }

    fn exclude_attr(lstr: impl AsRef<str>) -> LitStrAttr {
        lit_str_attr("exclude", lstr)
    }

    impl Exclude {
        fn fake(value: &str, r: Option<Regex>) -> Self {
            let r = r.unwrap_or_else(|| regex::Regex::new(value).unwrap());
            Self {
                attr: exclude_attr(value),
                r,
            }
        }
    }

    impl From<&str> for Exclude {
        fn from(value: &str) -> Self {
            Self {
                attr: exclude_attr(value),
                r: Regex::new(value).unwrap(),
            }
        }
    }

    #[rstest]
    #[case::simple(r#"fn f(#[files("some_glob")] a: PathBuf) {}"#, "fn f(a: PathBuf) {}", &[("a", None, &["some_glob"], &[], true, false)])]
    #[case::more_than_one(
        r#"fn f(#[files("first")] a: PathBuf, b: u32, #[files("third")] c: PathBuf) {}"#,
        r#"fn f(a: PathBuf,
                b: u32,
                c: PathBuf) {}"#,
        &[("a", None, &["first"], &[], true, false), ("c", None, &["third"], &[], true, false)],
    )]
    #[case::more_globs_on_the_same_var(
        r#"fn f(#[files("first")] #[files("second")] a: PathBuf) {}"#,
        r#"fn f(a: PathBuf) {}"#,
        &[("a", None, &["first", "second"], &[], true, false)],
    )]
    #[case::exclude(r#"fn f(#[files("some_glob")] #[exclude("exclude")] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", None, &["some_glob"], &["exclude"], true, false)])]
    #[case::exclude_more(r#"fn f(#[files("some_glob")] #[exclude("first")]  #[exclude("second")] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", None, &["some_glob"], &["first", "second"], true, false)])]
    #[case::include_dot_files(r#"fn f(#[files("some_glob")] #[include_dot_files] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", None, &["some_glob"], &[], false, false)])]
    #[case::base_dir(r#"fn f(#[files("some_glob")] #[base_dir = "/base/"] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", Some("/base/"), &["some_glob"], &[], true, false)])]
    #[case::ignore_env_vars(r#"fn f(#[files("some_glob")] #[ignore_missing_env_vars] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", None, &["some_glob"], &[], true, true)])]
    #[case::ignore_env_vars_unknown(r#"fn f(#[files("some${SOME_VAR}_glob")] #[ignore_missing_env_vars] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", None, &["some${SOME_VAR}_glob"], &[], true, true)])]
    #[case::env_vars_default(r#"fn f(#[files("some${__UNKNOWN__:-default/value}_glob")] a: PathBuf) {}"#,
    "fn f(a: PathBuf) {}", &[("a", None, &["some${__UNKNOWN__:-default/value}_glob"], &[], true, false)])]
    fn extract<'a, G: AsRef<[&'a str]>, E: AsRef<[&'a str]>>(
        #[case] item_fn: &str,
        #[case] expected: &str,
        #[case] expected_files: &[(&str, Option<&str>, G, E, bool, bool)],
    ) {
        let mut item_fn: ItemFn = item_fn.ast();
        let expected: ItemFn = expected.ast();

        let files = extract_files(&mut item_fn).unwrap();

        assert_eq!(expected, item_fn);
        assert_eq!(
            files,
            expected_files
                .into_iter()
                .map(|(id, base_dir, globs, ex, ignore, ignore_envvars)| {
                    (
                        ident(id),
                        FilesGlobReferences::new(
                            globs.as_ref().iter().map(files_attr).collect(),
                            ex.as_ref().iter().map(|&ex| ex.into()).collect(),
                            *ignore,
                            *ignore_envvars,
                            Default::default(),
                        )
                        .with_base_dir_opt(base_dir.map(base_dir_attr)),
                    )
                })
                .collect::<Vec<_>>()
        );
    }

    #[rstest]
    #[case::no_files_arg("fn f(#[files] a: PathBuf) {}", "#[files(...)]")]
    #[case::invalid_files_inner("fn f(#[files(a::b::c)] a: PathBuf) {}", "string literal")]
    #[case::no_exclude_args(
        r#"fn f(#[files("some")] #[exclude] a: PathBuf) {}"#,
        "#[exclude(...)]"
    )]
    #[case::invalid_exclude_inner(
        r#"fn f(#[files("some")] #[exclude(a::b)] a: PathBuf) {}"#,
        "string literal"
    )]
    #[case::invalid_exclude_regex(
        r#"fn f(#[files("some")] #[exclude("invalid(reg(ex")] a: PathBuf) {}"#,
        "valid regex"
    )]
    #[case::include_dot_files_with_args(
        r#"fn f(#[files("some")] #[include_dot_files(some)] a: PathBuf) {}"#,
        "#[include_dot_files]"
    )]
    #[case::exclude_without_files(
        r#"fn f(#[exclude("some")] a: PathBuf) {}"#,
        "#[exclude(...)] without #[files(...)]"
    )]
    #[case::include_dot_files_without_files(
        r#"fn f(#[include_dot_files] a: PathBuf) {}"#,
        "#[include_dot_files] without #[files(...)]"
    )]
    #[case::include_dot_files_more_than_once(
        r#"fn f(#[files("some")] #[include_dot_files] #[include_dot_files] a: PathBuf) {}"#,
        "more than once"
    )]
    #[case::ignore_missing_env_vars_without_files(
        r#"fn f(#[ignore_missing_env_vars] a: PathBuf) {}"#,
        "#[ignore_missing_env_vars] without #[files(...)]"
    )]
    #[case::ignore_missing_env_vars_more_than_once(
        r#"fn f(#[files("some")] #[ignore_missing_env_vars] #[ignore_missing_env_vars] a: PathBuf) {}"#,
        "more than once"
    )]
    #[case::override_base_dir_more_than_once(
        r#"fn f(#[files("some")] #[base_dir = "/"] #[base_dir = "/"] a: PathBuf) {}"#,
        "more than once"
    )]
    #[case::invalid_base_dir(
        r#"fn f(#[files("some")] #[base_dir = 123] a: PathBuf) {}"#,
        "base directory for the glob path"
    )]
    #[case::multiple_file_modes(
        r#"fn f(#[files("some")] #[mode = str] #[mode = str] a: PathBuf) {}"#,
        r#"Cannot use #[mode = ...] more than once"#
    )]
    #[case::unknown_file_mode(
        r#"fn f(#[files("some")] #[mode = blabla] a: PathBuf) {}"#,
        r#"Expected one of the following keywords: path, str or bytes"#
    )]
    #[case::wrong_attr_file_mode(
        r#"fn f(#[files("some")] #[mode(blabla)] a: PathBuf) {}"#,
        "Use #[mode = ...] to define the argument of the file input"
    )]
    fn raise_error(#[case] item_fn: &str, #[case] message: &str) {
        let mut item_fn: ItemFn = item_fn.ast();

        let err = extract_files(&mut item_fn).unwrap_err();

        assert_in!(format!("{:?}", err), message);
    }

    #[rstest]
    #[case::unknown_env_var(
        r#"fn f(#[files("so${__UNKNOWN__}me")] a: PathBuf) {}"#,
        "Could not find the environment variable \\\"__UNKNOWN__\\\""
    )]
    #[case::invalid_env_var(
        r#"fn f(#[files("so${__INVALID%%__}me")] a: PathBuf) {}"#,
        "The variable \\\"__INVALID%%__\\\" name does not match [a-zA-Z0-9_]+"
    )]
    #[case::base_dir_unknown_env_var(
        r#"fn f(#[base_dir = "${__UNKNOWN__}"] #[files("some")] a: PathBuf) {}"#,
        "Could not find the environment variable \\\"__UNKNOWN__\\\""
    )]
    fn raise_error_on_paths(#[case] item_fn: &str, #[case] message: &str) {
        let mut item_fn: ItemFn = item_fn.ast();

        let ok = extract_files(&mut item_fn).unwrap();
        let err: syn::Error = ok
            .into_iter()
            .map(|(_id, refs)| {
                let base_dir = refs.base_dir()?;
                let x = refs.paths(base_dir.as_ref().unwrap_or(&PathBuf::from("/default")))?;
                eprintln!("x {:?}", x);
                Ok(())
            })
            .collect::<Result<(), _>>()
            .unwrap_err();

        assert_in!(format!("{:?}", err), message);
    }

    #[derive(Default)]
    struct FakeBaseDir(PathBuf);
    impl From<&str> for FakeBaseDir {
        fn from(value: &str) -> Self {
            Self(PathBuf::from(value))
        }
    }
    impl BaseDir for FakeBaseDir {
        fn base_dir(&self) -> Result<PathBuf, String> {
            Ok(self.0.clone())
        }
    }

    impl<'a> ValueListFromFiles<'a> {
        fn new(bdir: impl BaseDir + 'a, g_resover: impl GlobResolver + 'a) -> Self {
            Self {
                base_dir: Box::new(bdir),
                g_resolver: Box::new(g_resover),
            }
        }
    }

    #[derive(Default)]
    struct FakeResolver(Vec<String>);

    impl From<&[&str]> for FakeResolver {
        fn from(value: &[&str]) -> Self {
            Self(value.iter().map(ToString::to_string).collect())
        }
    }

    impl GlobResolver for FakeResolver {
        fn glob(&self, _pattern: &str) -> Result<Vec<PathBuf>, String> {
            Ok(self.0.iter().map(PathBuf::from).collect())
        }
    }

    #[derive(Default)]
    struct FakeMapResolver(String, HashMap<String, Vec<PathBuf>>);

    impl From<(&str, &HashMap<&str, &[&str]>)> for FakeMapResolver {
        fn from(value: (&str, &HashMap<&str, &[&str]>)) -> Self {
            Self(
                value.0.to_string(),
                value
                    .1
                    .iter()
                    .map(|(&key, &values)| {
                        (
                            key.to_string(),
                            values
                                .iter()
                                .map(|&v| PathBuf::from(format!("{}/{v}", value.0)))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect(),
            )
        }
    }

    impl GlobResolver for FakeMapResolver {
        fn glob(&self, pattern: &str) -> Result<Vec<PathBuf>, String> {
            let pattern = pattern.strip_prefix(&format!("{}/", self.0)).unwrap();
            Ok(self.1.get(pattern).cloned().unwrap_or_default())
        }
    }

    #[rstest]
    #[case::simple("/base", None, FakeResolver::from(["/base/first", "/base/second"].as_slice()), vec![], true, &["first", "second"])]
    #[case::more_glob("/base", Some(["path1", "path2"].as_slice()), FakeMapResolver::from(
        ("/base", &hashmap!(
            "path1" => ["first", "second"].as_slice(),
            "path2" => ["third", "zzzz"].as_slice()
        ))
    ), vec![], true, &["first", "second", "third", "zzzz"])]
    #[case::should_remove_duplicates("/base", Some(["path1", "path2"].as_slice()), FakeMapResolver::from(
        ("/base", &hashmap!(
            "path1" => ["first", "second"].as_slice(),
            "path2" => ["second", "third"].as_slice()
        ))
    ), vec![], true, &["first", "second", "third"])]
    #[case::should_sort("/base", None, FakeResolver::from(["/base/second", "/base/first"].as_slice()), vec![], true, &["first", "second"])]
    #[case::exclude("/base", None, FakeResolver::from([
        "/base/first", "/base/rem_1", "/base/other/rem_2", "/base/second"].as_slice()),
        vec![Exclude::fake("no_mater", Some(Regex::new("rem_").unwrap()))], true, &["first", "second"])]
    #[case::exclude_more("/base", None, FakeResolver::from([
        "/base/first", "/base/rem_1", "/base/other/rem_2", "/base/some/other", "/base/second"].as_slice()),
        vec![
            Exclude::fake("no_mater", Some(Regex::new("rem_").unwrap())),
            Exclude::fake("no_mater", Some(Regex::new("some").unwrap())),
            ], true, &["first", "second"])]
    #[case::ignore_dot_files("/base", None, FakeResolver::from([
        "/base/first", "/base/.ignore", "/base/.ignore_dir/a", "/base/second/.not", "/base/second/but_include", "/base/in/.out/other/ignored"].as_slice()),
        vec![], true, &["first", "second/but_include"])]
    #[case::include_dot_files("/base", None, FakeResolver::from([
        "/base/first", "/base/.ignore", "/base/.ignore_dir/a", "/base/second/.not", "/base/second/but_include", "/base/in/.out/other/ignored"].as_slice()),
        vec![], false, &[".ignore", ".ignore_dir/a", "first", "in/.out/other/ignored", "second/.not", "second/but_include"])]
    #[case::relative_path("/base/some/other/folders", None,
        FakeResolver::from(["/base/first", "/base/second"].as_slice()), vec![], true, &["../../../first", "../../../second"])]
    fn generate_a_variable_with_the_glob_resolved_path(
        #[case] bdir: &str,
        #[case] paths: Option<&[&str]>,
        #[case] resolver: impl GlobResolver,
        #[case] exclude: Vec<Exclude>,
        #[case] ignore_dot_files: bool,
        #[case] expected: &[&str],
    ) {
        let paths = paths
            .map(|inner| inner.into_iter().map(files_attr).collect())
            .unwrap_or(vec![files_attr("no_mater")]);
        let values = ValueListFromFiles::new(FakeBaseDir::from(bdir), resolver)
            .to_value_list(vec![(
                ident("a"),
                FilesGlobReferences::new(
                    paths,
                    exclude,
                    ignore_dot_files,
                    false,
                    Default::default(),
                ),
            )])
            .unwrap();

        let mut v_list = values_list(
            "a",
            &expected
                .iter()
                .map(|&p| RelativePath::from_path(p).unwrap())
                .map(|r| r.to_logical_path(bdir))
                .map(|p| {
                    format!(
                        r#"<::std::path::PathBuf as std::str::FromStr>::from_str("{}").unwrap()"#,
                        p.as_os_str().to_str().unwrap()
                    )
                })
                .collect::<Vec<_>>(),
        );
        v_list
            .values
            .iter_mut()
            .zip(expected.iter())
            .for_each(|(v, &ex)| {
                v.description = Some(render_file_description(
                    &RelativePath::from_path(ex).unwrap(),
                ))
            });
        assert_eq!(vec![v_list], values);
    }

    #[rstest]
    #[case::file("name.txt", "name.txt")]
    #[case::in_folder("some/folder/name.txt", "some/folder/name.txt")]
    #[case::no_extension("name", "name")]
    #[case::parent("../../name.txt", "_UP/_UP/name.txt")]
    #[case::ignore_current("./../other/name.txt", "_UP/other/name.txt")]
    fn render_file_description_should(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(
            render_file_description(&RelativePath::from_path(path).unwrap()),
            expected
        );
    }

    #[test]
    #[should_panic(expected = "Fake error")]
    fn raise_error_if_fail_to_get_root() {
        #[derive(Default)]
        struct ErrorBaseDir;
        impl BaseDir for ErrorBaseDir {
            fn base_dir(&self) -> Result<PathBuf, String> {
                Err("Fake error".to_string())
            }
        }

        ValueListFromFiles::new(ErrorBaseDir::default(), FakeResolver::default())
            .to_value_list(vec![(
                ident("a"),
                FilesGlobReferences::new(
                    vec![files_attr("no_mater")],
                    Default::default(),
                    true,
                    false,
                    Default::default(),
                ),
            )])
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "No file found")]
    fn raise_error_if_no_files_found() {
        ValueListFromFiles::new(FakeBaseDir::default(), FakeResolver::default())
            .to_value_list(vec![(
                ident("a"),
                FilesGlobReferences::new(
                    vec![files_attr("no_mater")],
                    Default::default(),
                    true,
                    false,
                    Default::default(),
                ),
            )])
            .unwrap();
    }

    #[test]
    #[should_panic(expected = "glob failed")]
    fn default_glob_resolver_raise_error_if_invalid_glob_path() {
        DefaultGlobResolver.glob("/invalid/path/***").unwrap();
    }

    struct MapEnvironmentResolver(HashMap<String, String>);

    impl EnvironmentResolver for MapEnvironmentResolver {
        fn get(&self, var: &str) -> Option<String> {
            self.0.get(var).cloned()
        }
    }

    #[rstest]
    #[case::works(&[("VALUE", "VVV")], false, "some${VALUE}_glob", Ok("someVVV_glob"))]
    #[case::works_simple(&[("VALUE", "VVV")], false, "some/$VALUE/glob", Ok("some/VVV/glob"))]
    #[case::unknown_err(&[], false, "some${__UNKNOWN__}_glob", Err("__UNKNOWN__"))]
    #[case::ignore_unknown(&[], true, "some${__UNKNOWN__}_glob", Ok("some_glob"))]
    #[case::invalid(&[], false, "some${__%$!@__}_glob", Err("__%$!@__"))]
    #[case::default_unknown(&[], false, "some${__UNKNOWN__:-VVV}_glob", Ok("someVVV_glob"))]
    #[case::default_empty(&[("EMPTY", "")], false, "some${EMPTY:-EEE}_glob", Ok("someEEE_glob"))]
    fn replace_env_vars(
        #[case] env_vars: &[(&str, &str)],
        #[case] ignore_missing_env_vars: bool,
        #[case] glob: &str,
        #[case] expected: Result<&str, &str>,
    ) {
        let resolver = MapEnvironmentResolver(
            env_vars
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        let refs = FilesGlobReferences::new(
            vec![],
            Default::default(),
            true,
            ignore_missing_env_vars,
            Default::default(),
        );

        let result = refs.replace_env_vars(&files_attr(glob), resolver);
        match (&result, expected) {
            (Ok(r), Ok(e)) => assert_eq!(r, e),
            (Err(r), Err(e)) => assert_in!(format!("{:?}", r), e),
            _ => panic!(
                "Expected and result should be the same. Expected: {:?}, Result: {:?}",
                expected, result
            ),
        }
    }
}
