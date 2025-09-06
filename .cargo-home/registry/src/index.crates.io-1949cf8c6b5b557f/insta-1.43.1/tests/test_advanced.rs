use insta::{allow_duplicates, assert_debug_snapshot};

#[cfg(feature = "filters")]
#[test]
fn test_basic_filter() {
    use insta::{assert_snapshot, with_settings};
    with_settings!({filters => vec![
        (r"\b[[:xdigit:]]{8}\b", "[SHORT_HEX]")
    ]}, {
        assert_snapshot!("Hello DEADBEEF!", @"Hello [SHORT_HEX]!");
    })
}

#[cfg(feature = "json")]
#[test]
fn test_basic_suffixes() {
    for value in [1, 2, 3] {
        insta::with_settings!({snapshot_suffix => value.to_string()}, {
            insta::assert_json_snapshot!(&value);
        });
    }
}

#[test]
fn test_basic_duplicates_passes() {
    allow_duplicates! {
        for x in (0..10).step_by(2) {
            let is_even = x % 2 == 0;
            assert_debug_snapshot!(is_even, @"true");
        }
    }
}

#[test]
#[should_panic = "snapshot assertion for 'basic_duplicates_assertion_failed' failed in line"]
fn test_basic_duplicates_assertion_failed() {
    allow_duplicates! {
        for x in (0..10).step_by(3) {
            let is_even = x % 2 == 0;
            assert_debug_snapshot!(is_even, @"true");
        }
    }
}
