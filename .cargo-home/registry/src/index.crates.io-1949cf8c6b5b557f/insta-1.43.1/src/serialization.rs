use serde::de::value::Error as ValueError;
use serde::Serialize;

use crate::content::{json, yaml, Content, ContentSerializer};
use crate::settings::Settings;

pub enum SerializationFormat {
    #[cfg(feature = "csv")]
    Csv,
    #[cfg(feature = "ron")]
    Ron,
    #[cfg(feature = "toml")]
    Toml,
    Yaml,
    Json,
    JsonCompact,
}

#[derive(Debug)]
pub enum SnapshotLocation {
    Inline,
    File,
}

pub fn serialize_content(mut content: Content, format: SerializationFormat) -> String {
    content = Settings::with(|settings| {
        if settings.sort_maps() {
            content.sort_maps();
        }
        #[cfg(feature = "redactions")]
        {
            for (selector, redaction) in settings.iter_redactions() {
                content = selector.redact(content, redaction);
            }
        }
        content
    });

    match format {
        SerializationFormat::Yaml => yaml::to_string(&content)[4..].to_string(),
        SerializationFormat::Json => json::to_string_pretty(&content),
        SerializationFormat::JsonCompact => json::to_string_compact(&content),
        #[cfg(feature = "csv")]
        SerializationFormat::Csv => {
            let mut buf = Vec::with_capacity(128);
            {
                let mut writer = csv::Writer::from_writer(&mut buf);
                // if the top-level content we're serializing is a vector we
                // want to serialize it multiple times once for each item.
                if let Some(content_slice) = content.as_slice() {
                    for content in content_slice {
                        writer.serialize(content).unwrap();
                    }
                } else {
                    writer.serialize(&content).unwrap();
                }
                writer.flush().unwrap();
            }
            if buf.ends_with(b"\n") {
                buf.truncate(buf.len() - 1);
            }
            String::from_utf8(buf).unwrap()
        }
        #[cfg(feature = "ron")]
        SerializationFormat::Ron => {
            let mut buf = Vec::new();
            let mut config = ron::ser::PrettyConfig::new();
            config.new_line = "\n".to_string();
            config.indentor = "  ".to_string();
            config.struct_names = true;
            let mut serializer = ron::ser::Serializer::with_options(
                &mut buf,
                Some(config),
                ron::options::Options::default(),
            )
            .unwrap();
            content.serialize(&mut serializer).unwrap();
            String::from_utf8(buf).unwrap()
        }
        #[cfg(feature = "toml")]
        SerializationFormat::Toml => {
            let mut rv = toml::to_string_pretty(&content).unwrap();
            if rv.ends_with('\n') {
                rv.truncate(rv.len() - 1);
            }
            rv
        }
    }
}

pub fn serialize_value<S: Serialize>(s: &S, format: SerializationFormat) -> String {
    let serializer = ContentSerializer::<ValueError>::new();
    let content = Serialize::serialize(s, serializer).unwrap();
    serialize_content(content, format)
}

#[cfg(feature = "redactions")]
pub fn serialize_value_redacted<S: Serialize>(
    s: &S,
    redactions: &[(crate::redaction::Selector, crate::redaction::Redaction)],
    format: SerializationFormat,
) -> String {
    let serializer = ContentSerializer::<ValueError>::new();
    let mut content = Serialize::serialize(s, serializer).unwrap();
    for (selector, redaction) in redactions {
        content = selector.redact(content, redaction);
    }
    serialize_content(content, format)
}

#[test]
fn test_yaml_serialization() {
    let yaml = serialize_content(
        Content::Map(vec![
            (
                Content::from("env"),
                Content::Seq(vec![
                    Content::from("ENVIRONMENT"),
                    Content::from("production"),
                ]),
            ),
            (
                Content::from("cmdline"),
                Content::Seq(vec![Content::from("my-tool"), Content::from("run")]),
            ),
        ]),
        SerializationFormat::Yaml,
    );
    crate::assert_snapshot!(&yaml, @r"
    env:
      - ENVIRONMENT
      - production
    cmdline:
      - my-tool
      - run
    ");

    let inline_yaml = serialize_content(
        Content::Map(vec![
            (
                Content::from("env"),
                Content::Seq(vec![
                    Content::from("ENVIRONMENT"),
                    Content::from("production"),
                ]),
            ),
            (
                Content::from("cmdline"),
                Content::Seq(vec![Content::from("my-tool"), Content::from("run")]),
            ),
        ]),
        SerializationFormat::Yaml,
    );
    crate::assert_snapshot!(&inline_yaml, @r"
    env:
      - ENVIRONMENT
      - production
    cmdline:
      - my-tool
      - run
    ");
}
