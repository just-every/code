#[cfg(feature = "json")]
use insta::assert_json_snapshot;
#[cfg(feature = "yaml")]
use insta::assert_yaml_snapshot;
#[allow(deprecated)]
use insta::{assert_debug_snapshot, assert_display_snapshot, assert_snapshot};
use std::fmt;

#[test]
fn test_debug_vector() {
    assert_debug_snapshot!("debug_vector", vec![1, 2, 3]);
}

#[test]
fn test_unnamed_debug_vector() {
    assert_debug_snapshot!(vec![1, 2, 3]);
    assert_debug_snapshot!(vec![1, 2, 3, 4]);
    assert_debug_snapshot!(vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_unnamed_nested_closure() {
    #![allow(clippy::redundant_closure_call)]
    (|| {
        (|| {
            assert_debug_snapshot!(vec![1, 2, 3]);
        })();
    })();
}

#[cfg(feature = "yaml")]
#[test]
fn test_yaml_vector() {
    assert_yaml_snapshot!("yaml_vector", vec![1, 2, 3]);
}

#[cfg(feature = "yaml")]
#[test]
fn test_unnamed_yaml_vector() {
    assert_yaml_snapshot!(vec![1, 2, 3]);
    assert_yaml_snapshot!(vec![1, 2, 3, 4]);
    assert_yaml_snapshot!(vec![1, 2, 3, 4, 5]);
}

#[cfg(feature = "json")]
#[test]
fn test_json_vector() {
    assert_json_snapshot!("json_vector", vec![1, 2, 3]);
}

#[cfg(feature = "json")]
#[test]
fn test_unnamed_json_vector() {
    assert_json_snapshot!(vec![1, 2, 3]);
    assert_json_snapshot!(vec![1, 2, 3, 4]);
    assert_json_snapshot!(vec![1, 2, 3, 4, 5]);
}

mod nested {
    #[test]
    fn test_nested_module() {
        insta::assert_snapshot!("aoeu");
    }
}

#[test]
fn test_trailing_commas() {
    assert_snapshot!("Testing",);
    assert_snapshot!("Testing", "name",);
    assert_snapshot!("Testing", "name", "expr",);
    #[cfg(feature = "yaml")]
    assert_yaml_snapshot!(vec![1, 2, 3, 4, 5],);
}

struct TestDisplay;

impl fmt::Display for TestDisplay {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "TestDisplay struct")
    }
}

#[test]
#[allow(deprecated)]
fn test_display() {
    let td = TestDisplay;
    assert_display_snapshot!("display", td);
}

#[test]
#[allow(deprecated)]
fn test_unnamed_display() {
    let td = TestDisplay;
    assert_display_snapshot!(td);
    assert_display_snapshot!("whatever");
}

#[cfg(feature = "json")]
#[test]
fn test_u128_json() {
    let x: u128 = u128::from(u64::MAX) * 2;
    assert_json_snapshot!(&x, @"36893488147419103230");
}

#[cfg(feature = "yaml")]
#[test]
fn insta_sort_order() {
    use std::collections::HashMap;
    let mut m = HashMap::new();
    m.insert((1, 3), 4);
    m.insert((2, 3), 4);
    m.insert((1, 4), 4);
    m.insert((3, 3), 4);
    m.insert((9, 3), 4);
    insta::with_settings!({sort_maps =>true}, {
        insta::assert_yaml_snapshot!(m);
    });
}

#[test]
fn test_crlf() {
    insta::assert_snapshot!("foo\r\nbar\r\nbaz");
}

#[test]
fn test_trailing_crlf() {
    insta::assert_snapshot!("foo\r\nbar\r\nbaz\r\n");
}

#[test]
fn test_trailing_crlf_inline() {
    insta::assert_snapshot!("foo\r\nbar\r\nbaz\r\n", @r"
    foo
    bar
    baz
    ");
}
