use crate::{
    content::{self, json, yaml, Content},
    elog,
    utils::style,
};
use once_cell::sync::Lazy;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{borrow::Cow, fmt};

static RUN_ID: Lazy<String> = Lazy::new(|| {
    if let Ok(run_id) = env::var("NEXTEST_RUN_ID") {
        run_id
    } else {
        let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        format!("{}-{}", d.as_secs(), d.subsec_nanos())
    }
});

/// Holds a pending inline snapshot loaded from a json file or read from an assert
/// macro (doesn't write to the rust file, which is done by `cargo-insta`)
#[derive(Debug)]
pub struct PendingInlineSnapshot {
    pub run_id: String,
    pub line: u32,
    pub new: Option<Snapshot>,
    pub old: Option<Snapshot>,
}

impl PendingInlineSnapshot {
    pub fn new(new: Option<Snapshot>, old: Option<Snapshot>, line: u32) -> PendingInlineSnapshot {
        PendingInlineSnapshot {
            new,
            old,
            line,
            run_id: RUN_ID.clone(),
        }
    }

    #[cfg(feature = "_cargo_insta_internal")]
    pub fn load_batch(p: &Path) -> Result<Vec<PendingInlineSnapshot>, Box<dyn Error>> {
        let contents =
            fs::read_to_string(p).map_err(|e| content::Error::FileIo(e, p.to_path_buf()))?;

        let mut rv: Vec<Self> = contents
            .lines()
            .map(|line| {
                let value = yaml::parse_str(line, p)?;
                Self::from_content(value)
            })
            .collect::<Result<_, Box<dyn Error>>>()?;

        // remove all but the last run
        if let Some(last_run_id) = rv.last().map(|x| x.run_id.clone()) {
            rv.retain(|x| x.run_id == last_run_id);
        }

        Ok(rv)
    }

    #[cfg(feature = "_cargo_insta_internal")]
    pub fn save_batch(p: &Path, batch: &[PendingInlineSnapshot]) -> Result<(), Box<dyn Error>> {
        fs::remove_file(p).ok();
        for snap in batch {
            snap.save(p)?;
        }
        Ok(())
    }

    pub fn save(&self, p: &Path) -> Result<(), Box<dyn Error>> {
        let mut f = fs::OpenOptions::new().create(true).append(true).open(p)?;
        let mut s = json::to_string(&self.as_content());
        s.push('\n');
        f.write_all(s.as_bytes())?;
        Ok(())
    }

    #[cfg(feature = "_cargo_insta_internal")]
    fn from_content(content: Content) -> Result<PendingInlineSnapshot, Box<dyn Error>> {
        if let Content::Map(map) = content {
            let mut run_id = None;
            let mut line = None;
            let mut old = None;
            let mut new = None;

            for (key, value) in map.into_iter() {
                match key.as_str() {
                    Some("run_id") => run_id = value.as_str().map(|x| x.to_string()),
                    Some("line") => line = value.as_u64().map(|x| x as u32),
                    Some("old") if !value.is_nil() => {
                        old = Some(Snapshot::from_content(value, TextSnapshotKind::Inline)?)
                    }
                    Some("new") if !value.is_nil() => {
                        new = Some(Snapshot::from_content(value, TextSnapshotKind::Inline)?)
                    }
                    _ => {}
                }
            }

            Ok(PendingInlineSnapshot {
                run_id: run_id.ok_or(content::Error::MissingField)?,
                line: line.ok_or(content::Error::MissingField)?,
                new,
                old,
            })
        } else {
            Err(content::Error::UnexpectedDataType.into())
        }
    }

    fn as_content(&self) -> Content {
        let fields = vec![
            ("run_id", Content::from(self.run_id.as_str())),
            ("line", Content::from(self.line)),
            (
                "new",
                match &self.new {
                    Some(snap) => snap.as_content(),
                    None => Content::None,
                },
            ),
            (
                "old",
                match &self.old {
                    Some(snap) => snap.as_content(),
                    None => Content::None,
                },
            ),
        ];

        Content::Struct("PendingInlineSnapshot", fields)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotKind {
    Text,
    Binary { extension: String },
}

impl Default for SnapshotKind {
    fn default() -> Self {
        SnapshotKind::Text
    }
}

/// Snapshot metadata information.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MetaData {
    /// The source file (relative to workspace root).
    pub(crate) source: Option<String>,
    /// The source line, if available. This is used by pending snapshots, but trimmed
    /// before writing to the final `.snap` files in [`MetaData::trim_for_persistence`].
    pub(crate) assertion_line: Option<u32>,
    /// Optional human readable (non formatted) snapshot description.
    pub(crate) description: Option<String>,
    /// Optionally the expression that created the snapshot.
    pub(crate) expression: Option<String>,
    /// An optional arbitrary structured info object.
    pub(crate) info: Option<Content>,
    /// Reference to the input file.
    pub(crate) input_file: Option<String>,
    /// The type of the snapshot (string or binary).
    pub(crate) snapshot_kind: SnapshotKind,
}

impl MetaData {
    /// Returns the absolute source path.
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Returns the assertion line.
    pub fn assertion_line(&self) -> Option<u32> {
        self.assertion_line
    }

    /// Returns the expression that created the snapshot.
    pub fn expression(&self) -> Option<&str> {
        self.expression.as_deref()
    }

    /// Returns the description that created the snapshot.
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref().filter(|x| !x.is_empty())
    }

    /// Returns the embedded info.
    #[doc(hidden)]
    pub fn private_info(&self) -> Option<&Content> {
        self.info.as_ref()
    }

    /// Returns the relative source path.
    pub fn get_relative_source(&self, base: &Path) -> Option<PathBuf> {
        self.source.as_ref().map(|source| {
            base.join(source)
                .canonicalize()
                .ok()
                .and_then(|s| s.strip_prefix(base).ok().map(|x| x.to_path_buf()))
                .unwrap_or_else(|| base.to_path_buf())
        })
    }

    /// Returns the input file reference.
    pub fn input_file(&self) -> Option<&str> {
        self.input_file.as_deref()
    }

    fn from_content(content: Content) -> Result<MetaData, Box<dyn Error>> {
        if let Content::Map(map) = content {
            let mut source = None;
            let mut assertion_line = None;
            let mut description = None;
            let mut expression = None;
            let mut info = None;
            let mut input_file = None;
            let mut snapshot_type = TmpSnapshotKind::Text;
            let mut extension = None;

            enum TmpSnapshotKind {
                Text,
                Binary,
            }

            for (key, value) in map.into_iter() {
                match key.as_str() {
                    Some("source") => source = value.as_str().map(|x| x.to_string()),
                    Some("assertion_line") => assertion_line = value.as_u64().map(|x| x as u32),
                    Some("description") => description = value.as_str().map(Into::into),
                    Some("expression") => expression = value.as_str().map(Into::into),
                    Some("info") if !value.is_nil() => info = Some(value),
                    Some("input_file") => input_file = value.as_str().map(Into::into),
                    Some("snapshot_kind") => {
                        snapshot_type = match value.as_str() {
                            Some("binary") => TmpSnapshotKind::Binary,
                            _ => TmpSnapshotKind::Text,
                        }
                    }
                    Some("extension") => {
                        extension = value.as_str().map(Into::into);
                    }
                    _ => {}
                }
            }

            Ok(MetaData {
                source,
                assertion_line,
                description,
                expression,
                info,
                input_file,
                snapshot_kind: match snapshot_type {
                    TmpSnapshotKind::Text => SnapshotKind::Text,
                    TmpSnapshotKind::Binary => SnapshotKind::Binary {
                        extension: extension.ok_or(content::Error::MissingField)?,
                    },
                },
            })
        } else {
            Err(content::Error::UnexpectedDataType.into())
        }
    }

    fn as_content(&self) -> Content {
        let mut fields = Vec::new();
        if let Some(source) = self.source.as_deref() {
            fields.push(("source", Content::from(source)));
        }
        if let Some(line) = self.assertion_line {
            fields.push(("assertion_line", Content::from(line)));
        }
        if let Some(description) = self.description.as_deref() {
            fields.push(("description", Content::from(description)));
        }
        if let Some(expression) = self.expression.as_deref() {
            fields.push(("expression", Content::from(expression)));
        }
        if let Some(info) = &self.info {
            fields.push(("info", info.to_owned()));
        }
        if let Some(input_file) = self.input_file.as_deref() {
            fields.push(("input_file", Content::from(input_file)));
        }

        match self.snapshot_kind {
            SnapshotKind::Text => {}
            SnapshotKind::Binary { ref extension } => {
                fields.push(("extension", Content::from(extension.clone())));
                fields.push(("snapshot_kind", Content::from("binary")));
            }
        }

        Content::Struct("MetaData", fields)
    }

    /// Trims the metadata of fields that we don't save to `.snap` files (those
    /// we only use for display while reviewing)
    fn trim_for_persistence(&self) -> Cow<'_, MetaData> {
        // TODO: in order for `--require-full-match` to work on inline snapshots
        // without cargo-insta, we need to trim all fields if there's an inline
        // snapshot. But we don't know that from here (notably
        // `self.input_file.is_none()` is not a correct approach). Given that
        // `--require-full-match` is experimental and we're working on making
        // inline & file snapshots more coherent, I'm leaving this as is for
        // now.
        if self.assertion_line.is_some() {
            let mut rv = self.clone();
            rv.assertion_line = None;
            Cow::Owned(rv)
        } else {
            Cow::Borrowed(self)
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum TextSnapshotKind {
    Inline,
    File,
}

/// A helper to work with file snapshots.
#[derive(Debug, Clone)]
pub struct Snapshot {
    module_name: String,
    snapshot_name: Option<String>,
    metadata: MetaData,
    snapshot: SnapshotContents,
}

impl Snapshot {
    /// Loads a snapshot from a file.
    pub fn from_file(p: &Path) -> Result<Snapshot, Box<dyn Error>> {
        let mut f = BufReader::new(fs::File::open(p)?);
        let mut buf = String::new();

        f.read_line(&mut buf)?;

        // yaml format
        let metadata = if buf.trim_end() == "---" {
            loop {
                let read = f.read_line(&mut buf)?;
                if read == 0 {
                    break;
                }
                if buf[buf.len() - read..].trim_end() == "---" {
                    buf.truncate(buf.len() - read);
                    break;
                }
            }
            let content = yaml::parse_str(&buf, p)?;
            MetaData::from_content(content)?
        // legacy format
        // (but not viable to move into `match_legacy` given it's more than
        // just the snapshot value itself...)
        } else {
            let mut rv = MetaData::default();
            loop {
                buf.clear();
                let read = f.read_line(&mut buf)?;
                if read == 0 || buf.trim_end().is_empty() {
                    buf.truncate(buf.len() - read);
                    break;
                }
                let mut iter = buf.splitn(2, ':');
                if let Some(key) = iter.next() {
                    if let Some(value) = iter.next() {
                        let value = value.trim();
                        match key.to_lowercase().as_str() {
                            "expression" => rv.expression = Some(value.to_string()),
                            "source" => rv.source = Some(value.into()),
                            _ => {}
                        }
                    }
                }
            }
            elog!("A snapshot uses a legacy snapshot format; please update it to the new format with `cargo insta test --force-update-snapshots --accept`.\nSnapshot is at: {}", p.to_string_lossy());
            rv
        };

        let contents = match metadata.snapshot_kind {
            SnapshotKind::Text => {
                buf.clear();
                for (idx, line) in f.lines().enumerate() {
                    let line = line?;
                    if idx > 0 {
                        buf.push('\n');
                    }
                    buf.push_str(&line);
                }

                TextSnapshotContents {
                    contents: buf,
                    kind: TextSnapshotKind::File,
                }
                .into()
            }
            SnapshotKind::Binary { ref extension } => {
                let path = build_binary_path(extension, p);
                let contents = fs::read(path)?;

                SnapshotContents::Binary(Rc::new(contents))
            }
        };

        let (snapshot_name, module_name) = names_of_path(p);

        Ok(Snapshot::from_components(
            module_name,
            Some(snapshot_name),
            metadata,
            contents,
        ))
    }

    pub(crate) fn from_components(
        module_name: String,
        snapshot_name: Option<String>,
        metadata: MetaData,
        snapshot: SnapshotContents,
    ) -> Snapshot {
        Snapshot {
            module_name,
            snapshot_name,
            metadata,
            snapshot,
        }
    }

    #[cfg(feature = "_cargo_insta_internal")]
    fn from_content(content: Content, kind: TextSnapshotKind) -> Result<Snapshot, Box<dyn Error>> {
        if let Content::Map(map) = content {
            let mut module_name = None;
            let mut snapshot_name = None;
            let mut metadata = None;
            let mut snapshot = None;

            for (key, value) in map.into_iter() {
                match key.as_str() {
                    Some("module_name") => module_name = value.as_str().map(|x| x.to_string()),
                    Some("snapshot_name") => snapshot_name = value.as_str().map(|x| x.to_string()),
                    Some("metadata") => metadata = Some(MetaData::from_content(value)?),
                    Some("snapshot") => {
                        snapshot = Some(
                            TextSnapshotContents {
                                contents: value
                                    .as_str()
                                    .ok_or(content::Error::UnexpectedDataType)?
                                    .to_string(),
                                kind,
                            }
                            .into(),
                        );
                    }
                    _ => {}
                }
            }

            Ok(Snapshot {
                module_name: module_name.ok_or(content::Error::MissingField)?,
                snapshot_name,
                metadata: metadata.ok_or(content::Error::MissingField)?,
                snapshot: snapshot.ok_or(content::Error::MissingField)?,
            })
        } else {
            Err(content::Error::UnexpectedDataType.into())
        }
    }

    fn as_content(&self) -> Content {
        let mut fields = vec![("module_name", Content::from(self.module_name.as_str()))];
        // Note this is currently never used, since this method is only used for
        // inline snapshots
        if let Some(name) = self.snapshot_name.as_deref() {
            fields.push(("snapshot_name", Content::from(name)));
        }
        fields.push(("metadata", self.metadata.as_content()));

        if let SnapshotContents::Text(ref content) = self.snapshot {
            fields.push(("snapshot", Content::from(content.to_string())));
        }

        Content::Struct("Content", fields)
    }

    /// Returns the module name.
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Returns the snapshot name.
    pub fn snapshot_name(&self) -> Option<&str> {
        self.snapshot_name.as_deref()
    }

    /// The metadata in the snapshot.
    pub fn metadata(&self) -> &MetaData {
        &self.metadata
    }

    /// The snapshot contents
    pub fn contents(&self) -> &SnapshotContents {
        &self.snapshot
    }

    /// Snapshot contents match another snapshot's.
    pub fn matches(&self, other: &Self) -> bool {
        self.contents() == other.contents()
            // For binary snapshots the extension also need to be the same:
            && self.metadata.snapshot_kind == other.metadata.snapshot_kind
    }

    /// Both the exact snapshot contents and the persisted metadata match another snapshot's.
    // (could rename to `matches_exact` for consistency, after some current
    // pending merge requests are merged)
    pub fn matches_fully(&self, other: &Self) -> bool {
        match (self.contents(), other.contents()) {
            (SnapshotContents::Text(self_contents), SnapshotContents::Text(other_contents)) => {
                // Note that we previously would match the exact values of the
                // unnormalized text. But that's too strict — it means we can
                // never match a snapshot that has leading/trailing whitespace.
                // So instead we check it matches on the latest format.
                // Generally those should be the same — latest should be doing
                // the minimum normalization; if they diverge we could update
                // this to be stricter.
                //
                // (I think to do this perfectly, we'd want to match the
                // _reference_ value unnormalized, but the _generated_ value
                // normalized. That way, we can get the But at the moment we
                // don't distinguish between which is which in our data
                // structures.)
                let contents_match_exact = self_contents.matches_latest(other_contents);
                match self_contents.kind {
                    TextSnapshotKind::File => {
                        self.metadata.trim_for_persistence()
                            == other.metadata.trim_for_persistence()
                            && contents_match_exact
                    }
                    TextSnapshotKind::Inline => contents_match_exact,
                }
            }
            _ => self.matches(other),
        }
    }

    fn serialize_snapshot(&self, md: &MetaData) -> String {
        let mut buf = yaml::to_string(&md.as_content());
        buf.push_str("---\n");

        if let SnapshotContents::Text(ref contents) = self.snapshot {
            buf.push_str(&contents.to_string());
            buf.push('\n');
        }

        buf
    }

    // We take `md` as an argument here because the calling methods want to
    // adjust it; e.g. removing volatile fields when writing to the final
    // `.snap` file.
    fn save_with_metadata(&self, path: &Path, md: &MetaData) -> Result<(), Box<dyn Error>> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }

        let serialized_snapshot = self.serialize_snapshot(md);
        fs::write(path, serialized_snapshot)
            .map_err(|e| content::Error::FileIo(e, path.to_path_buf()))?;

        if let SnapshotContents::Binary(ref contents) = self.snapshot {
            fs::write(self.build_binary_path(path).unwrap(), &**contents)
                .map_err(|e| content::Error::FileIo(e, path.to_path_buf()))?;
        }

        Ok(())
    }

    pub fn build_binary_path(&self, path: impl Into<PathBuf>) -> Option<PathBuf> {
        if let SnapshotKind::Binary { ref extension } = self.metadata.snapshot_kind {
            Some(build_binary_path(extension, path))
        } else {
            None
        }
    }

    /// Saves the snapshot.
    #[doc(hidden)]
    pub fn save(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        self.save_with_metadata(path, &self.metadata.trim_for_persistence())
    }

    /// Same as [`Self::save`] but instead of writing a normal snapshot file this will write
    /// a `.snap.new` file with additional information.
    ///
    /// The path of the new snapshot file is returned.
    pub(crate) fn save_new(&self, path: &Path) -> Result<PathBuf, Box<dyn Error>> {
        // TODO: should we be the actual extension here rather than defaulting
        // to the standard `.snap`?
        let new_path = path.to_path_buf().with_extension("snap.new");
        self.save_with_metadata(&new_path, &self.metadata)?;
        Ok(new_path)
    }
}

/// The contents of a Snapshot
#[derive(Debug, Clone)]
pub enum SnapshotContents {
    Text(TextSnapshotContents),

    // This is in an `Rc` because we need to be able to clone this struct cheaply and the contents
    // of the `Vec` could be rather large. The reason it's not an `Rc<[u8]>` is because creating one
    // of those would require re-allocating because of the additional size needed for the reference
    // count.
    Binary(Rc<Vec<u8>>),
}

// Could be Cow, but I think limited savings
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TextSnapshotContents {
    contents: String,
    pub kind: TextSnapshotKind,
}

impl From<TextSnapshotContents> for SnapshotContents {
    fn from(value: TextSnapshotContents) -> Self {
        SnapshotContents::Text(value)
    }
}

impl SnapshotContents {
    pub fn is_binary(&self) -> bool {
        matches!(self, SnapshotContents::Binary(_))
    }
}

impl TextSnapshotContents {
    pub fn new(contents: String, kind: TextSnapshotKind) -> TextSnapshotContents {
        // We could store a normalized version of the string as part of `new`;
        // it would avoid allocating a new `String` when we get the normalized
        // versions, which we may do a few times. (We want to store the
        // unnormalized version because it allows us to use `matches_fully`.)
        TextSnapshotContents { contents, kind }
    }

    /// Snapshot matches based on the latest format.
    pub fn matches_latest(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }

    pub fn matches_legacy(&self, other: &Self) -> bool {
        fn as_str_legacy(sc: &TextSnapshotContents) -> String {
            let out = sc.to_string();
            // Legacy inline snapshots have `---` at the start, so this strips that if
            // it exists.
            let out = match out.strip_prefix("---\n") {
                Some(old_snapshot) => old_snapshot.to_string(),
                None => out,
            };
            match sc.kind {
                TextSnapshotKind::Inline => legacy_inline_normalize(&out),
                TextSnapshotKind::File => out,
            }
        }
        as_str_legacy(self) == as_str_legacy(other)
    }

    fn normalize(&self) -> String {
        let kind_specific_normalization = match self.kind {
            TextSnapshotKind::Inline => normalize_inline_snapshot(&self.contents),
            TextSnapshotKind::File => self.contents.clone(),
        };
        // Then this we do for both kinds
        let out = kind_specific_normalization
            .trim_start_matches(['\r', '\n'])
            .trim_end();
        out.replace("\r\n", "\n")
    }

    /// Returns the string literal, including `#` delimiters, to insert into a
    /// Rust source file.
    pub fn to_inline(&self, indentation: &str) -> String {
        let contents = self.normalize();
        let mut out = String::new();

        // Some characters can't be escaped in a raw string literal, so we need
        // to escape the string if it contains them. We prefer escaping control
        // characters except for newlines, tabs, and ESC.
        let has_control_chars = contents
            .chars()
            .any(|c| c.is_control() && !['\n', '\t', '\x1b'].contains(&c));

        // We prefer raw strings for strings containing a quote or an escape
        // character, and for strings containing newlines (which reduces diffs).
        // We can't use raw strings for some control characters.
        if !has_control_chars && contents.contains(['\\', '"', '\n']) {
            out.push('r');
        }

        let delimiter = "#".repeat(required_hashes(&contents));

        out.push_str(&delimiter);

        // If there are control characters, then we have to just use a simple
        // string with unicode escapes from the debug output. We don't attempt
        // block mode (though not impossible to do so).
        if has_control_chars {
            out.push_str(format!("{:?}", contents).as_str());
        } else {
            out.push('"');
            // if we have more than one line we want to change into the block
            // representation mode
            if contents.contains('\n') {
                out.extend(
                    contents
                        .lines()
                        // Adds an additional newline at the start of multiline
                        // string (not sure this is the clearest way of representing
                        // it, but it works...)
                        .map(|l| {
                            format!(
                                "\n{i}{l}",
                                i = if l.is_empty() { "" } else { indentation },
                                l = l
                            )
                        })
                        // `lines` removes the final line ending — add back. Include
                        // indentation so the closing delimited aligns with the full string.
                        .chain(Some(format!("\n{}", indentation))),
                );
            } else {
                out.push_str(contents.as_str());
            }
            out.push('"');
        }

        out.push_str(&delimiter);
        out
    }
}

impl fmt::Display for TextSnapshotContents {
    /// Returns the snapshot contents as a normalized string (for example,
    /// removing surrounding whitespace)
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.normalize())
    }
}

impl PartialEq for SnapshotContents {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (SnapshotContents::Text(this), SnapshotContents::Text(other)) => {
                // Ideally match on current rules, but otherwise fall back to legacy rules
                if this.matches_latest(other) {
                    true
                } else if this.matches_legacy(other) {
                    elog!("{} {}\n{}",style("Snapshot test passes but the existing value is in a legacy format. Please run `cargo insta test --force-update-snapshots` to update to a newer format.").yellow().bold(),"Snapshot contents:", this.to_string());
                    true
                } else {
                    false
                }
            }
            (SnapshotContents::Binary(this), SnapshotContents::Binary(other)) => this == other,
            _ => false,
        }
    }
}

fn build_binary_path(extension: &str, path: impl Into<PathBuf>) -> PathBuf {
    let path = path.into();
    let mut new_extension = path.extension().unwrap().to_os_string();
    new_extension.push(".");
    new_extension.push(extension);

    path.with_extension(new_extension)
}

/// The number of `#` we need to surround a raw string literal with.
fn required_hashes(text: &str) -> usize {
    text.split('"')
        .skip(1) // Skip the first part which is before the first quote
        .map(|s| s.chars().take_while(|&c| c == '#').count() + 1)
        .max()
        .unwrap_or_default()
}

#[test]
fn test_required_hashes() {
    assert_snapshot!(required_hashes(""), @"0");
    assert_snapshot!(required_hashes("Hello, world!"), @"0");
    assert_snapshot!(required_hashes("\"\""), @"1");
    assert_snapshot!(required_hashes("##"), @"0");
    assert_snapshot!(required_hashes("\"#\"#"), @"2");
    assert_snapshot!(required_hashes(r##""#"##), @"2");
    assert_snapshot!(required_hashes(r######"foo ""##### bar "###" baz"######), @"6");
    assert_snapshot!(required_hashes("\"\"\""), @"1");
    assert_snapshot!(required_hashes("####"), @"0");
    assert_snapshot!(required_hashes(r###"\"\"##\"\""###), @"3");
    assert_snapshot!(required_hashes(r###"r"#"Raw string"#""###), @"2");
}

fn leading_space(value: &str) -> String {
    value
        .chars()
        .take_while(|x| x.is_whitespace())
        .collect::<String>()
}

fn min_indentation(snapshot: &str) -> String {
    let lines = snapshot.trim_end().lines();

    if lines.clone().count() <= 1 {
        // not a multi-line string
        return "".into();
    }

    lines
        .filter(|l| !l.is_empty())
        .map(leading_space)
        .min_by(|a, b| a.len().cmp(&b.len()))
        .unwrap_or("".into())
}

/// Removes excess indentation, and changes newlines to \n.
fn normalize_inline_snapshot(snapshot: &str) -> String {
    let indentation = min_indentation(snapshot);
    snapshot
        .lines()
        .map(|l| l.get(indentation.len()..).unwrap_or(""))
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Extracts the module and snapshot name from a snapshot path
fn names_of_path(path: &Path) -> (String, String) {
    // The final part of the snapshot file name is the test name; the
    // initial parts are the module name
    let parts: Vec<&str> = path
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap_or("")
        .rsplitn(2, "__")
        .collect();

    match parts.as_slice() {
        [snapshot_name, module_name] => (snapshot_name.to_string(), module_name.to_string()),
        [snapshot_name] => (snapshot_name.to_string(), String::new()),
        _ => (String::new(), "<unknown>".to_string()),
    }
}

#[test]
fn test_names_of_path() {
    assert_debug_snapshot!(
        names_of_path(Path::new("/src/snapshots/insta_tests__tests__name_foo.snap")), @r#"
    (
        "name_foo",
        "insta_tests__tests",
    )
    "#
    );
    assert_debug_snapshot!(
        names_of_path(Path::new("/src/snapshots/name_foo.snap")), @r#"
    (
        "name_foo",
        "",
    )
    "#
    );
    assert_debug_snapshot!(
        names_of_path(Path::new("foo/src/snapshots/go1.20.5.snap")), @r#"
    (
        "go1.20.5",
        "",
    )
    "#
    );
}

/// legacy format - retain so old snapshots still work
fn legacy_inline_normalize(frozen_value: &str) -> String {
    if !frozen_value.trim_start().starts_with('⋮') {
        return frozen_value.to_string();
    }
    let mut buf = String::new();
    let mut line_iter = frozen_value.lines();
    let mut indentation = 0;

    for line in &mut line_iter {
        let line_trimmed = line.trim_start();
        if line_trimmed.is_empty() {
            continue;
        }
        indentation = line.len() - line_trimmed.len();
        // 3 because '⋮' is three utf-8 bytes long
        buf.push_str(&line_trimmed[3..]);
        buf.push('\n');
        break;
    }

    for line in &mut line_iter {
        if let Some(prefix) = line.get(..indentation) {
            if !prefix.trim().is_empty() {
                return "".to_string();
            }
        }
        if let Some(remainder) = line.get(indentation..) {
            if let Some(rest) = remainder.strip_prefix('⋮') {
                buf.push_str(rest);
                buf.push('\n');
            } else if remainder.trim().is_empty() {
                continue;
            } else {
                return "".to_string();
            }
        }
    }

    buf.trim_end().to_string()
}

#[test]
fn test_snapshot_contents() {
    use similar_asserts::assert_eq;
    let snapshot_contents =
        TextSnapshotContents::new("testing".to_string(), TextSnapshotKind::Inline);
    assert_eq!(snapshot_contents.to_inline(""), r#""testing""#);

    let t = &"
a
b"[1..];
    assert_eq!(
        TextSnapshotContents::new(t.to_string(), TextSnapshotKind::Inline).to_inline(""),
        r##"r"
a
b
""##
    );

    assert_eq!(
        TextSnapshotContents::new("a\nb".to_string(), TextSnapshotKind::Inline).to_inline("    "),
        r##"r"
    a
    b
    ""##
    );

    assert_eq!(
        TextSnapshotContents::new("\n    a\n    b".to_string(), TextSnapshotKind::Inline)
            .to_inline(""),
        r##"r"
a
b
""##
    );

    assert_eq!(
        TextSnapshotContents::new("\na\n\nb".to_string(), TextSnapshotKind::Inline)
            .to_inline("    "),
        r##"r"
    a

    b
    ""##
    );

    assert_eq!(
        TextSnapshotContents::new("\n    ab\n".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r##""ab""##
    );

    assert_eq!(
        TextSnapshotContents::new("ab".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r#""ab""#
    );

    // Test control and special characters
    assert_eq!(
        TextSnapshotContents::new("a\tb".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r##""a	b""##
    );

    assert_eq!(
        TextSnapshotContents::new("a\t\nb".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r##"r"
a	
b
""##
    );

    assert_eq!(
        TextSnapshotContents::new("a\rb".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r##""a\rb""##
    );

    assert_eq!(
        TextSnapshotContents::new("a\0b".to_string(), TextSnapshotKind::Inline).to_inline(""),
        // Nul byte is printed as `\0` in Rust string literals
        r##""a\0b""##
    );

    assert_eq!(
        TextSnapshotContents::new("a\u{FFFD}b".to_string(), TextSnapshotKind::Inline).to_inline(""),
        // Replacement character is returned as the character in literals
        r##""a�b""##
    );
}

#[test]
fn test_snapshot_contents_hashes() {
    assert_eq!(
        TextSnapshotContents::new("a###b".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r#""a###b""#
    );

    assert_eq!(
        TextSnapshotContents::new("a\n\\###b".to_string(), TextSnapshotKind::Inline).to_inline(""),
        r#####"r"
a
\###b
""#####
    );
}

#[test]
fn test_normalize_inline_snapshot() {
    use similar_asserts::assert_eq;
    // here we do exact matching (rather than `assert_snapshot`)
    // to ensure we're not incorporating the modifications this library makes
    assert_eq!(
        normalize_inline_snapshot(
            r#"
   1
   2
   "#,
        ),
        r###"
1
2
"###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
            1
    2"#
        ),
        r###"
        1
2"###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
            1
            2
    "#
        ),
        r###"
1
2
"###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
   1
   2
"#
        ),
        r###"
1
2"###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
        a
    "#
        ),
        "
a
"
    );

    assert_eq!(normalize_inline_snapshot(""), "");

    assert_eq!(
        normalize_inline_snapshot(
            r#"
    a
    b
c
    "#
        ),
        r###"
    a
    b
c
    "###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
a
    "#
        ),
        "
a
    "
    );

    assert_eq!(
        normalize_inline_snapshot(
            "
    a"
        ),
        "
a"
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"a
  a"#
        ),
        r###"a
  a"###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
			1
	2"#
        ),
        r###"
		1
2"###
    );

    assert_eq!(
        normalize_inline_snapshot(
            r#"
	  	  1
	  	  2
    "#
        ),
        r###"
1
2
"###
    );
}

#[test]
fn test_min_indentation() {
    use similar_asserts::assert_eq;
    let t = r#"
   1
   2
    "#;
    assert_eq!(min_indentation(t), "   ".to_string());

    let t = r#"
            1
    2"#;
    assert_eq!(min_indentation(t), "    ".to_string());

    let t = r#"
            1
            2
    "#;
    assert_eq!(min_indentation(t), "            ".to_string());

    let t = r#"
   1
   2
"#;
    assert_eq!(min_indentation(t), "   ".to_string());

    let t = r#"
        a
    "#;
    assert_eq!(min_indentation(t), "        ".to_string());

    let t = "";
    assert_eq!(min_indentation(t), "".to_string());

    let t = r#"
    a
    b
c
    "#;
    assert_eq!(min_indentation(t), "".to_string());

    let t = r#"
a
    "#;
    assert_eq!(min_indentation(t), "".to_string());

    let t = "
    a";
    assert_eq!(min_indentation(t), "    ".to_string());

    let t = r#"a
  a"#;
    assert_eq!(min_indentation(t), "".to_string());

    let t = r#"
 	1
 	2
    "#;
    assert_eq!(min_indentation(t), " 	".to_string());

    let t = r#"
  	  	  	1
  	2"#;
    assert_eq!(min_indentation(t), "  	".to_string());

    let t = r#"
			1
	2"#;
    assert_eq!(min_indentation(t), "	".to_string());
}

#[test]
fn test_inline_snapshot_value_newline() {
    // https://github.com/mitsuhiko/insta/issues/39
    assert_eq!(normalize_inline_snapshot("\n"), "");
}

#[test]
fn test_parse_yaml_error() {
    use std::env::temp_dir;
    let mut temp = temp_dir();
    temp.push("bad.yaml");
    let mut f = fs::File::create(temp.clone()).unwrap();

    let invalid = r#"---
    This is invalid yaml:
     {
        {
    ---
    "#;

    f.write_all(invalid.as_bytes()).unwrap();

    let error = format!("{}", Snapshot::from_file(temp.as_path()).unwrap_err());
    assert!(error.contains("Failed parsing the YAML from"));
    assert!(error.contains("bad.yaml"));
}

/// Check that snapshots don't take ownership of the value
#[test]
fn test_ownership() {
    // Range is non-copy
    use std::ops::Range;
    let r = Range { start: 0, end: 10 };
    assert_debug_snapshot!(r, @"0..10");
    assert_debug_snapshot!(r, @"0..10");
}

#[test]
fn test_empty_lines() {
    assert_snapshot!(r#"single line should fit on a single line"#, @"single line should fit on a single line");
    assert_snapshot!(r#"single line should fit on a single line, even if it's really really really really really really really really really long"#, @"single line should fit on a single line, even if it's really really really really really really really really really long");

    assert_snapshot!(r#"multiline content starting on first line

    final line
    "#, @r"
    multiline content starting on first line

        final line
    ");

    assert_snapshot!(r#"
    multiline content starting on second line

    final line
    "#, @r###"

        multiline content starting on second line

        final line

    "###);
}
