use rstest::*;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[rstest]
fn start_with_name(
    #[files("files/**/*.txt")]
    #[exclude("exclude")]
    #[files("../files_test_sub_folder/**/*.txt")]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn start_with_name_file_mode(
    #[files("files/**/*.txt")]
    #[exclude("exclude")]
    #[files("../files_test_sub_folder/**/*.txt")]
    #[mode = path]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn start_with_name_with_include(
    #[files("files/**/*.txt")]
    #[exclude("exclude")]
    #[include_dot_files]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn ignore_missing_env_vars(
    #[ignore_missing_env_vars]
    #[files("files/**/${__SHOULD_NOT_BE_DECLARED__}*.txt")]
    #[exclude("exclude")]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn env_vars(
    #[files("${FILES_ENV_VAR}/**/*.txt")]
    #[exclude("exclude")]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn env_vars_unknown(
    #[files("${__UNKNOWN__:-files}/**/*.txt")]
    #[exclude("exclude")]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn env_vars_base_dir(
    #[files("**/*.txt")]
    #[base_dir = "files"]
    #[exclude("exclude")]
    path: PathBuf,
) {
    let name = path.file_name().unwrap();
    let mut f = File::open(&path).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    assert!(contents.starts_with(name.to_str().unwrap()))
}

#[rstest]
fn include_str(
    #[files("files/**/*.txt")]
    #[exclude("exclude")]
    #[files("../files_test_sub_folder/**/*.txt")]
    #[mode = str]
    contents: &str,
) {
    assert!(contents.len() != 0)
}

#[rstest]
fn include_bytes(
    #[files("files/**/*.txt")]
    #[exclude("exclude")]
    #[files("../files_test_sub_folder/**/*.txt")]
    #[mode = bytes]
    contents: &[u8],
) {
    assert!(contents.len() != 0)
}

mod module {
    #[rstest::rstest]
    fn pathbuf_need_not_be_in_scope(
        #[files("files/**/*.txt")]
        #[exclude("exclude")]
        #[include_dot_files]
        path: std::path::PathBuf,
    ) {
        let _ = path;
    }
}
