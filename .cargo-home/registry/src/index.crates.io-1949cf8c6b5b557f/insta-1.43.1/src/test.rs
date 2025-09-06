#[test]
fn test_embedded_test() {
    assert_snapshot!("embedded", "Just a string");
}
