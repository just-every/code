#[test]
fn test_binary_snapshot() {
    insta::assert_binary_snapshot!(".txt", b"test".to_vec());
}

#[test]
#[should_panic(expected = "'.new' is not allowed as a file extension")]
fn test_new_extension() {
    insta::assert_binary_snapshot!(".new", b"test".to_vec());
}

#[test]
#[should_panic(expected = "\"test\" does not match the format \"name.extension\"")]
fn test_malformed_name_and_extension() {
    insta::assert_binary_snapshot!("test", b"test".to_vec());
}

#[test]
#[should_panic(expected = "file extensions starting with 'new.' are not allowed")]
fn test_extension_starting_with_new() {
    insta::assert_binary_snapshot!(".new.gz", b"test".to_vec());
}

#[test]
fn test_multipart_extension() {
    insta::assert_binary_snapshot!(".tar.gz", b"test".to_vec());
}

#[test]
fn test_named() {
    insta::assert_binary_snapshot!("name.json", b"null".to_vec());
}
