use once_cell::sync::Lazy;
#[cfg(feature = "serde")]
use serde::{de::value::Error as ValueError, Serialize};
use std::cell::RefCell;
use std::future::Future;
use std::mem;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use crate::content::Content;
#[cfg(feature = "serde")]
use crate::content::ContentSerializer;
#[cfg(feature = "filters")]
use crate::filters::Filters;
#[cfg(feature = "redactions")]
use crate::redaction::{dynamic_redaction, sorted_redaction, ContentPath, Redaction, Selector};

static DEFAULT_SETTINGS: Lazy<Arc<ActualSettings>> = Lazy::new(|| {
    Arc::new(ActualSettings {
        sort_maps: false,
        snapshot_path: "snapshots".into(),
        snapshot_suffix: "".into(),
        input_file: None,
        description: None,
        info: None,
        omit_expression: false,
        prepend_module_to_snapshot: true,
        #[cfg(feature = "redactions")]
        redactions: Redactions::default(),
        #[cfg(feature = "filters")]
        filters: Filters::default(),
        #[cfg(feature = "glob")]
        allow_empty_glob: false,
    })
});

thread_local!(static CURRENT_SETTINGS: RefCell<Settings> = RefCell::new(Settings::new()));

/// Represents stored redactions.
#[cfg(feature = "redactions")]
#[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
#[derive(Clone, Default)]
pub struct Redactions(Vec<(Selector<'static>, Arc<Redaction>)>);

#[cfg(feature = "redactions")]
impl<'a> From<Vec<(&'a str, Redaction)>> for Redactions {
    fn from(value: Vec<(&'a str, Redaction)>) -> Redactions {
        Redactions(
            value
                .into_iter()
                .map(|x| (Selector::parse(x.0).unwrap().make_static(), Arc::new(x.1)))
                .collect(),
        )
    }
}

#[derive(Clone)]
#[doc(hidden)]
pub struct ActualSettings {
    pub sort_maps: bool,
    pub snapshot_path: PathBuf,
    pub snapshot_suffix: String,
    pub input_file: Option<PathBuf>,
    pub description: Option<String>,
    pub info: Option<Content>,
    pub omit_expression: bool,
    pub prepend_module_to_snapshot: bool,
    #[cfg(feature = "redactions")]
    pub redactions: Redactions,
    #[cfg(feature = "filters")]
    pub filters: Filters,
    #[cfg(feature = "glob")]
    pub allow_empty_glob: bool,
}

impl ActualSettings {
    pub fn sort_maps(&mut self, value: bool) {
        self.sort_maps = value;
    }

    pub fn snapshot_path<P: AsRef<Path>>(&mut self, path: P) {
        self.snapshot_path = path.as_ref().to_path_buf();
    }

    pub fn snapshot_suffix<I: Into<String>>(&mut self, suffix: I) {
        self.snapshot_suffix = suffix.into();
    }

    pub fn input_file<P: AsRef<Path>>(&mut self, p: P) {
        self.input_file = Some(p.as_ref().to_path_buf());
    }

    pub fn description<S: Into<String>>(&mut self, value: S) {
        self.description = Some(value.into());
    }

    #[cfg(feature = "serde")]
    pub fn info<S: Serialize>(&mut self, s: &S) {
        let serializer = ContentSerializer::<ValueError>::new();
        let content = Serialize::serialize(s, serializer).unwrap();
        self.info = Some(content);
    }

    pub fn raw_info(&mut self, content: &Content) {
        self.info = Some(content.to_owned());
    }

    pub fn omit_expression(&mut self, value: bool) {
        self.omit_expression = value;
    }

    pub fn prepend_module_to_snapshot(&mut self, value: bool) {
        self.prepend_module_to_snapshot = value;
    }

    #[cfg(feature = "redactions")]
    pub fn redactions<R: Into<Redactions>>(&mut self, r: R) {
        self.redactions = r.into();
    }

    #[cfg(feature = "filters")]
    pub fn filters<F: Into<Filters>>(&mut self, f: F) {
        self.filters = f.into();
    }

    #[cfg(feature = "glob")]
    pub fn allow_empty_glob(&mut self, value: bool) {
        self.allow_empty_glob = value;
    }
}

/// Configures how insta operates at test time.
///
/// Settings are always bound to a thread and some default settings are always
/// available.  These settings can be changed and influence how insta behaves on
/// that thread.  They can either temporarily or permanently changed.
///
/// This can be used to influence how the snapshot macros operate.
/// For instance it can be useful to force ordering of maps when
/// unordered structures are used through settings.
///
/// Some of the settings can be changed but shouldn't as it will make it harder
/// for tools like cargo-insta or an editor integration to locate the snapshot
/// files.
///
/// Settings can also be configured with the [`with_settings!`] macro.
///
/// Example:
///
/// ```ignore
/// use insta;
///
/// let mut settings = insta::Settings::clone_current();
/// settings.set_sort_maps(true);
/// settings.bind(|| {
///     // runs the assertion with the changed settings enabled
///     insta::assert_snapshot!(...);
/// });
/// ```
#[derive(Clone)]
pub struct Settings {
    inner: Arc<ActualSettings>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            inner: DEFAULT_SETTINGS.clone(),
        }
    }
}

impl Settings {
    /// Returns the default settings.
    ///
    /// It's recommended to use [`Self::clone_current`] instead so that
    /// already applied modifications are not discarded.
    pub fn new() -> Settings {
        Settings::default()
    }

    /// Returns a copy of the current settings.
    pub fn clone_current() -> Settings {
        Settings::with(|x| x.clone())
    }

    /// Internal helper for macros
    #[doc(hidden)]
    pub fn _private_inner_mut(&mut self) -> &mut ActualSettings {
        Arc::make_mut(&mut self.inner)
    }

    /// Enables forceful sorting of maps before serialization.
    ///
    /// Note that this only applies to snapshots that undergo serialization
    /// (eg: does not work for [`assert_debug_snapshot!`](crate::assert_debug_snapshot!).)
    ///
    /// The default value is `false`.
    pub fn set_sort_maps(&mut self, value: bool) {
        self._private_inner_mut().sort_maps = value;
    }

    /// Returns the current value for map sorting.
    pub fn sort_maps(&self) -> bool {
        self.inner.sort_maps
    }

    /// Disables prepending of modules to the snapshot filename.
    ///
    /// By default, the filename of a snapshot is `<module>__<name>.snap`.
    /// Setting this flag to `false` changes the snapshot filename to just
    /// `<name>.snap`.
    ///
    /// The default value is `true`.
    pub fn set_prepend_module_to_snapshot(&mut self, value: bool) {
        self._private_inner_mut().prepend_module_to_snapshot(value);
    }

    /// Returns the current value for module name prepending.
    pub fn prepend_module_to_snapshot(&self) -> bool {
        self.inner.prepend_module_to_snapshot
    }

    /// Allows the [`glob!`] macro to succeed if it matches no files.
    ///
    /// By default, the glob macro will fail the test if it does not find
    /// any files to prevent accidental typos.  This can be disabled when
    /// fixtures should be conditional.
    ///
    /// The default value is `false`.
    #[cfg(feature = "glob")]
    pub fn set_allow_empty_glob(&mut self, value: bool) {
        self._private_inner_mut().allow_empty_glob(value);
    }

    /// Returns the current value for the empty glob setting.
    #[cfg(feature = "glob")]
    pub fn allow_empty_glob(&self) -> bool {
        self.inner.allow_empty_glob
    }

    /// Sets the snapshot suffix.
    ///
    /// The snapshot suffix is added to all snapshot names with an `@` sign
    /// between.  For instance if the snapshot suffix is set to `"foo"` and
    /// the snapshot would be named `"snapshot"` it turns into `"snapshot@foo"`.
    /// This is useful to separate snapshots if you want to use test
    /// parameterization.
    pub fn set_snapshot_suffix<I: Into<String>>(&mut self, suffix: I) {
        self._private_inner_mut().snapshot_suffix(suffix);
    }

    /// Removes the snapshot suffix.
    pub fn remove_snapshot_suffix(&mut self) {
        self.set_snapshot_suffix("");
    }

    /// Returns the current snapshot suffix.
    pub fn snapshot_suffix(&self) -> Option<&str> {
        if self.inner.snapshot_suffix.is_empty() {
            None
        } else {
            Some(&self.inner.snapshot_suffix)
        }
    }

    /// Sets the input file reference.
    ///
    /// This value is completely unused by the snapshot testing system but it
    /// allows storing some metadata with a snapshot that refers back to the
    /// input file.  The path stored here is made relative to the workspace root
    /// before storing with the snapshot.
    pub fn set_input_file<P: AsRef<Path>>(&mut self, p: P) {
        self._private_inner_mut().input_file(p);
    }

    /// Removes the input file reference.
    pub fn remove_input_file(&mut self) {
        self._private_inner_mut().input_file = None;
    }

    /// Returns the current input file reference.
    pub fn input_file(&self) -> Option<&Path> {
        self.inner.input_file.as_deref()
    }

    /// Sets the description.
    ///
    /// The description is stored alongside the snapshot and will be displayed
    /// in the diff UI.  When a snapshot is captured the Rust expression for that
    /// snapshot is always retained.  However sometimes that information is not
    /// super useful by itself, particularly when working with loops and generated
    /// tests.  In that case the `description` can be set as extra information.
    ///
    /// See also [`Self::set_info`].
    pub fn set_description<S: Into<String>>(&mut self, value: S) {
        self._private_inner_mut().description(value);
    }

    /// Removes the description.
    pub fn remove_description(&mut self) {
        self._private_inner_mut().description = None;
    }

    /// Returns the current description
    pub fn description(&self) -> Option<&str> {
        self.inner.description.as_deref()
    }

    /// Sets the info.
    ///
    /// The `info` is similar to `description` but for structured data.  This is
    /// stored with the snapshot and shown in the review UI.  This for instance
    /// can be used to show extended information that can make a reviewer better
    /// understand what the snapshot is supposed to be testing.
    ///
    /// As an example the input parameters to the function that creates the snapshot
    /// can be persisted here.
    ///
    /// Alternatively you can use [`Self::set_raw_info`] instead.
    #[cfg(feature = "serde")]
    #[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
    pub fn set_info<S: Serialize>(&mut self, s: &S) {
        self._private_inner_mut().info(s);
    }

    /// Sets the info from a content object.
    ///
    /// This works like [`Self::set_info`] but does not require [`serde`].
    pub fn set_raw_info(&mut self, content: &Content) {
        self._private_inner_mut().raw_info(content);
    }

    /// Removes the info.
    pub fn remove_info(&mut self) {
        self._private_inner_mut().info = None;
    }

    /// Returns the current info
    pub(crate) fn info(&self) -> Option<&Content> {
        self.inner.info.as_ref()
    }

    /// Returns the current info
    pub fn has_info(&self) -> bool {
        self.inner.info.is_some()
    }

    /// If set to true, does not retain the expression in the snapshot.
    pub fn set_omit_expression(&mut self, value: bool) {
        self._private_inner_mut().omit_expression(value);
    }

    /// Returns true if expressions are omitted from snapshots.
    pub fn omit_expression(&self) -> bool {
        self.inner.omit_expression
    }

    /// Registers redactions that should be applied.
    ///
    /// This can be useful if redactions must be shared across multiple
    /// snapshots.
    ///
    /// Note that this only applies to snapshots that undergo serialization
    /// (eg: does not work for [`assert_debug_snapshot!`](crate::assert_debug_snapshot!).)
    #[cfg(feature = "redactions")]
    #[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
    pub fn add_redaction<R: Into<Redaction>>(&mut self, selector: &str, replacement: R) {
        self.add_redaction_impl(selector, replacement.into())
    }

    #[cfg(feature = "redactions")]
    fn add_redaction_impl(&mut self, selector: &str, replacement: Redaction) {
        self._private_inner_mut().redactions.0.push((
            Selector::parse(selector).unwrap().make_static(),
            Arc::new(replacement),
        ));
    }

    /// Registers a replacement callback.
    ///
    /// This works similar to a redaction but instead of changing the value it
    /// asserts the value at a certain place.  This function is internally
    /// supposed to call things like [`assert_eq!`].
    ///
    /// This is a shortcut to `add_redaction(selector, dynamic_redaction(...))`;
    #[cfg(feature = "redactions")]
    #[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
    pub fn add_dynamic_redaction<I, F>(&mut self, selector: &str, func: F)
    where
        I: Into<Content>,
        F: Fn(Content, ContentPath<'_>) -> I + Send + Sync + 'static,
    {
        self.add_redaction(selector, dynamic_redaction(func));
    }

    /// A special redaction that sorts a sequence or map.
    ///
    /// This is a shortcut to `add_redaction(selector, sorted_redaction())`.
    #[cfg(feature = "redactions")]
    #[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
    pub fn sort_selector(&mut self, selector: &str) {
        self.add_redaction(selector, sorted_redaction());
    }

    /// Replaces the currently set redactions.
    ///
    /// The default set is empty.
    #[cfg(feature = "redactions")]
    #[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
    pub fn set_redactions<R: Into<Redactions>>(&mut self, redactions: R) {
        self._private_inner_mut().redactions(redactions);
    }

    /// Removes all redactions.
    #[cfg(feature = "redactions")]
    #[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
    pub fn clear_redactions(&mut self) {
        self._private_inner_mut().redactions.0.clear();
    }

    /// Iterate over the redactions.
    #[cfg(feature = "redactions")]
    #[cfg_attr(docsrs, doc(cfg(feature = "redactions")))]
    pub(crate) fn iter_redactions(&self) -> impl Iterator<Item = (&Selector, &Redaction)> {
        self.inner.redactions.0.iter().map(|(a, b)| (a, &**b))
    }

    /// Adds a new filter.
    ///
    /// Filters are similar to redactions but are applied as regex onto the final snapshot
    /// value.  This can be used to perform modifications to the snapshot string that would
    /// be impossible to do with redactions because for instance the value is just a string.
    ///
    /// The first argument is the [`regex`] pattern to apply, the second is a replacement
    /// string.  The replacement string has the same functionality as the second argument
    /// to [`regex::Regex::replace`].
    ///
    /// This is useful to perform some cleanup procedures on the snapshot for unstable values.
    ///
    /// ```rust
    /// # use insta::Settings;
    /// # async fn foo() {
    /// # let mut settings = Settings::new();
    /// settings.add_filter(r"\b[[:xdigit:]]{32}\b", "[UID]");
    /// # }
    /// ```
    #[cfg(feature = "filters")]
    #[cfg_attr(docsrs, doc(cfg(feature = "filters")))]
    pub fn add_filter<S: Into<String>>(&mut self, regex: &str, replacement: S) {
        self._private_inner_mut().filters.add(regex, replacement);
    }

    /// Replaces the currently set filters.
    ///
    /// The default set is empty.
    #[cfg(feature = "filters")]
    #[cfg_attr(docsrs, doc(cfg(feature = "filters")))]
    pub fn set_filters<F: Into<Filters>>(&mut self, filters: F) {
        self._private_inner_mut().filters(filters);
    }

    /// Removes all filters.
    #[cfg(feature = "filters")]
    #[cfg_attr(docsrs, doc(cfg(feature = "filters")))]
    pub fn clear_filters(&mut self) {
        self._private_inner_mut().filters.clear();
    }

    /// Returns the current filters
    #[cfg(feature = "filters")]
    #[cfg_attr(docsrs, doc(cfg(feature = "filters")))]
    pub(crate) fn filters(&self) -> &Filters {
        &self.inner.filters
    }

    /// Sets the snapshot path.
    ///
    /// If not absolute it's relative to where the test is in.
    ///
    /// Defaults to `snapshots`.
    pub fn set_snapshot_path<P: AsRef<Path>>(&mut self, path: P) {
        self._private_inner_mut().snapshot_path(path);
    }

    /// Returns the snapshot path.
    pub fn snapshot_path(&self) -> &Path {
        &self.inner.snapshot_path
    }

    /// Runs a function with the current settings bound to the thread.
    ///
    /// This is an alternative to [`Self::bind_to_scope`]()
    /// which does not require holding on to a drop guard.  The return value
    /// of the closure is passed through.
    ///
    /// ```
    /// # use insta::Settings;
    /// let mut settings = Settings::clone_current();
    /// settings.set_sort_maps(true);
    /// settings.bind(|| {
    ///     // do stuff here
    /// });
    /// ```
    pub fn bind<F: FnOnce() -> R, R>(&self, f: F) -> R {
        let _guard = self.bind_to_scope();
        f()
    }

    /// Like [`Self::bind`] but for futures.
    ///
    /// This lets you bind settings for the duration of a future like this:
    ///
    /// ```rust
    /// # use insta::Settings;
    /// # async fn foo() {
    /// let settings = Settings::new();
    /// settings.bind_async(async {
    ///     // do assertions here
    /// }).await;
    /// # }
    /// ```
    pub fn bind_async<F: Future<Output = T>, T>(&self, future: F) -> impl Future<Output = T> {
        struct BindingFuture<F> {
            settings: Arc<ActualSettings>,
            future: F,
        }

        impl<F: Future> Future for BindingFuture<F> {
            type Output = F::Output;

            fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
                let inner = self.settings.clone();
                // SAFETY: This is okay because `future` is pinned when `self` is.
                let future = unsafe { self.map_unchecked_mut(|s| &mut s.future) };
                CURRENT_SETTINGS.with(|x| {
                    let old = {
                        let mut current = x.borrow_mut();
                        let old = current.inner.clone();
                        current.inner = inner;
                        old
                    };
                    let rv = future.poll(cx);
                    let mut current = x.borrow_mut();
                    current.inner = old;
                    rv
                })
            }
        }

        BindingFuture {
            settings: self.inner.clone(),
            future,
        }
    }

    /// Binds the settings to the current thread and resets when the drop
    /// guard is released.
    ///
    /// This is the recommended way to temporarily bind settings and replaces
    /// the earlier [`bind_to_scope`](Settings::bind_to_scope) and relies on
    /// drop guards.  An alternative is [`bind`](Settings::bind) which binds
    /// for the duration of the block it wraps.
    ///
    /// ```
    /// # use insta::Settings;
    /// let mut settings = Settings::clone_current();
    /// settings.set_sort_maps(true);
    /// let _guard = settings.bind_to_scope();
    /// // do stuff here
    /// ```
    pub fn bind_to_scope(&self) -> SettingsBindDropGuard {
        CURRENT_SETTINGS.with(|x| {
            let mut x = x.borrow_mut();
            let old = mem::replace(&mut x.inner, self.inner.clone());
            SettingsBindDropGuard(Some(old), std::marker::PhantomData)
        })
    }

    /// Runs a function with the current settings.
    pub(crate) fn with<R, F: FnOnce(&Settings) -> R>(f: F) -> R {
        CURRENT_SETTINGS.with(|x| f(&x.borrow()))
    }
}

/// Returned from [`Settings::bind_to_scope`]
///
/// This type is not shareable between threads:
///
/// ```compile_fail E0277
/// let mut settings = insta::Settings::clone_current();
/// settings.set_snapshot_suffix("test drop guard");
/// let guard = settings.bind_to_scope();
///
/// std::thread::spawn(move || { let guard = guard; }); // doesn't compile
/// ```
///
/// This is to ensure tests under async runtimes like `tokio` don't show unexpected results
#[must_use = "The guard is immediately dropped so binding has no effect. Use `let _guard = ...` to bind it."]
pub struct SettingsBindDropGuard(
    Option<Arc<ActualSettings>>,
    /// A ZST that is not [`Send`] but is [`Sync`]
    ///
    /// This is necessary due to the lack of stable [negative impls](https://github.com/rust-lang/rust/issues/68318).
    ///
    /// Required as [`SettingsBindDropGuard`] modifies a thread local variable which would end up
    /// with unexpected results if sent to a different thread.
    std::marker::PhantomData<std::sync::MutexGuard<'static, ()>>,
);

impl Drop for SettingsBindDropGuard {
    fn drop(&mut self) {
        CURRENT_SETTINGS.with(|x| {
            x.borrow_mut().inner = self.0.take().unwrap();
        })
    }
}
