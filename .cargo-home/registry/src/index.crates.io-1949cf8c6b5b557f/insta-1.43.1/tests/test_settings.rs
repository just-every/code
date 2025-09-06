#[cfg(feature = "yaml")]
use insta::assert_yaml_snapshot;
use similar_asserts::assert_eq;

use insta::{assert_debug_snapshot, with_settings, Settings};

#[cfg(feature = "yaml")]
#[test]
fn test_simple() {
    let mut map = std::collections::HashMap::new();
    map.insert("a", "first value");
    map.insert("b", "second value");
    map.insert("c", "third value");
    map.insert("d", "fourth value");

    let mut settings = insta::Settings::new();
    settings.set_sort_maps(true);
    settings.bind(|| {
        assert_yaml_snapshot!(&map, @r"
        a: first value
        b: second value
        c: third value
        d: fourth value
        ");
    });
}

#[cfg(feature = "yaml")]
#[test]
fn test_bound_to_scope() {
    let mut map = std::collections::HashMap::new();
    map.insert("a", "first value");
    map.insert("b", "second value");
    map.insert("c", "third value");
    map.insert("d", "fourth value");

    {
        let mut settings = Settings::new();
        settings.set_sort_maps(true);
        let _guard = settings.bind_to_scope();
        assert_yaml_snapshot!(&map, @r"
        a: first value
        b: second value
        c: third value
        d: fourth value
        ");
    }

    assert!(!Settings::clone_current().sort_maps());
}

#[cfg(feature = "yaml")]
#[test]
fn test_settings_macro() {
    let mut map = std::collections::HashMap::new();
    map.insert("a", "first value");
    map.insert("b", "second value");
    map.insert("c", "third value");
    map.insert("d", "fourth value");

    with_settings!({sort_maps => true}, {
        insta::assert_yaml_snapshot!(&map, @r"
        a: first value
        b: second value
        c: third value
        d: fourth value
        ");
    });
}

#[test]
fn test_snapshot_path() {
    with_settings!({snapshot_path => "snapshots2"}, {
        assert_debug_snapshot!(vec![1, 2, 3]);
    });
}

#[test]
fn test_snapshot_no_module_prepending() {
    with_settings!({prepend_module_to_snapshot => false}, {
        assert_debug_snapshot!(vec![1, 2, 3]);
    });
}

#[test]
fn test_snapshot_with_description() {
    with_settings!({description => "The snapshot is three integers"}, {
        assert_debug_snapshot!(vec![1, 2, 3])
    });
}

#[test]
fn test_snapshot_with_description_and_raw_info() {
    use insta::internals::Content;

    let raw_info = Content::Map(vec![
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
    ]);
    with_settings!({description => "The snapshot is four integers", raw_info => &raw_info}, {
        assert_debug_snapshot!(vec![1, 2, 3, 4])
    });
}

#[cfg(feature = "serde")]
#[test]
fn test_snapshot_with_description_and_info() {
    #[derive(serde::Serialize)]
    pub struct Info {
        env: std::collections::HashMap<&'static str, &'static str>,
        cmdline: Vec<&'static str>,
    }
    let info = Info {
        env: From::from([("ENVIRONMENT", "production")]),
        cmdline: vec!["my-tool", "run"],
    };
    with_settings!({description => "The snapshot is four integers", info => &info}, {
        assert_debug_snapshot!(vec![1, 2, 3, 4])
    });
}

#[test]
fn test_with_settings_inherit() {
    with_settings!({sort_maps => true}, {
        with_settings!({description => "aha"}, {
            let settings = Settings::clone_current();
            assert!(settings.sort_maps());
            assert_eq!(settings.description(), Some("aha"));
        });
    });
}
