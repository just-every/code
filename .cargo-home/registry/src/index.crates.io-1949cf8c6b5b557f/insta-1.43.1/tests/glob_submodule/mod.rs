#![cfg(feature = "glob")]

#[test]
fn test_basic_globbing_parent_dir() {
    insta::glob!("../inputs", "*.txt", |path| {
        let contents = std::fs::read_to_string(path).unwrap();
        insta::assert_json_snapshot!(&contents);
    });
}

#[test]
fn test_basic_globbing_nested_parent_dir_base_path() {
    insta::glob!("../inputs-nested", "*/*.txt", |path| {
        let contents = std::fs::read_to_string(path).unwrap();
        insta::assert_snapshot!(&contents);
    });
}

#[test]
fn test_basic_globbing_nested_parent_glob() {
    insta::glob!("..", "inputs-nested/*/*.txt", |path| {
        let contents = std::fs::read_to_string(path).unwrap();
        insta::assert_snapshot!(&contents);
    });
}

#[test]
fn test_globs_follow_links_parent_dir_base_path() {
    insta::glob!("../link-to-inputs", "*.txt", |path| {
        let contents = std::fs::read_to_string(path).unwrap();
        insta::assert_json_snapshot!(&contents);
    });
}

#[test]
fn test_globs_follow_links_parent_dir_glob() {
    insta::glob!("..", "link-to-inputs/*.txt", |path| {
        let contents = std::fs::read_to_string(path).unwrap();
        insta::assert_json_snapshot!(&contents);
    });
}

#[test]
fn test_basic_globbing_absolute_dir() {
    insta::glob!(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/inputs"),
        "*.txt",
        |path| {
            let contents = std::fs::read_to_string(path).unwrap();
            insta::assert_json_snapshot!(&contents);
        }
    );
}
