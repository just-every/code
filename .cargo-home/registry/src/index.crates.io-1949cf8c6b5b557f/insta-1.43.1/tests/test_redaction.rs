#![cfg(feature = "redactions")]

use insta::_macro_support::Selector;
#[cfg(feature = "csv")]
use insta::assert_csv_snapshot;
#[cfg(feature = "json")]
use insta::assert_json_snapshot;
#[cfg(feature = "ron")]
use insta::assert_ron_snapshot;
#[cfg(feature = "toml")]
use insta::assert_toml_snapshot;
#[cfg(feature = "yaml")]
use insta::assert_yaml_snapshot;

use insta::assert_debug_snapshot;
use serde::Serialize;

#[test]
fn test_selector_parser() {
    macro_rules! assert_selector_snapshot {
        ($short:expr, $sel:expr) => {
            assert_debug_snapshot!($short, Selector::parse($sel).unwrap());
        };
    }

    assert_selector_snapshot!("foo_bar", ".foo.bar");
    assert_selector_snapshot!("foo_bar_alt", ".foo[\"bar\"]");
    assert_selector_snapshot!("foo_bar_full_range", ".foo.bar[]");
    assert_selector_snapshot!("foo_bar_range_to", ".foo.bar[:10]");
    assert_selector_snapshot!("foo_bar_range_from", ".foo.bar[10:]");
    assert_selector_snapshot!("foo_bar_range", ".foo.bar[10:20]");
    assert_selector_snapshot!("foo_bar_deep", ".foo.bar.**");
}

#[derive(Serialize)]
pub struct Email(String);

#[derive(Serialize)]
pub struct User {
    id: u32,
    username: String,
    email: Email,
    extra: String,
}

#[cfg(feature = "yaml")]
#[test]
fn test_with_random_value() {
    assert_yaml_snapshot!(&User {
        id: 42,
        username: "john_doe".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    }, {
        ".id" => "[id]"
    });
}

#[cfg(feature = "yaml")]
#[test]
fn test_with_random_value_inline_callback() {
    assert_yaml_snapshot!(&User {
        id: 23,
        username: "john_doe".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    }, {
        ".id" => insta::dynamic_redaction(|value, path| {
            similar_asserts::assert_eq!(path.to_string(), ".id");
            similar_asserts::assert_eq!(value.as_u64().unwrap(), 23);
            "[id]"
        }),
    });
}

#[cfg(feature = "yaml")]
#[test]
fn test_with_random_value_and_trailing_comma() {
    assert_yaml_snapshot!(&User {
        id: 11,
        username: "john_doe".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    }, {
        ".id" => "[id]",
    });
}

#[cfg(feature = "yaml")]
#[test]
fn test_with_random_value_and_match_comma() {
    assert_yaml_snapshot!(
        &User {
            id: 11,
            username: "john_doe".to_string(),
            email: Email("john@example.com".to_string()),
            extra: "".to_string(),
        },
        match .. {
            ".id" => "[id]",
        }
    );
    assert_yaml_snapshot!(
        &User {
            id: 11,
            username: "john_doe".to_string(),
            email: Email("john@example.com".to_string()),
            extra: "".to_string(),
        },
        match .. {
            ".id" => "[id]",
        }, // comma here
    );
    assert_yaml_snapshot!(
        &User {
            id: 11,
            username: "john_doe".to_string(),
            email: Email("john@example.com".to_string()),
            extra: "".to_string(),
        },
        match .. {
            ".id" => "[id]",
        },
        @r#"
    id: "[id]"
    username: john_doe
    email: john@example.com
    extra: ""
    "#, // comma here
    );
}

#[cfg(feature = "csv")]
#[test]
fn test_with_random_value_csv() {
    assert_csv_snapshot!("user_csv", &User {
        id: 44,
        username: "julius_csv".to_string(),
        email: Email("julius@example.com".to_string()),
        extra: "".to_string(),
    }, {
        ".id" => "[id]"
    });
}

#[cfg(feature = "csv")]
#[test]
fn test_with_random_value_csv_match() {
    assert_csv_snapshot!(
        &User {
            id: 44,
            username: "julius_csv".to_string(),
            email: Email("julius@example.com".to_string()),
            extra: "".to_string(),
        },
        match .. {
            ".id" => "[id]",
        }
    );
}

#[cfg(feature = "ron")]
#[test]
fn test_with_random_value_ron() {
    assert_ron_snapshot!("user_ron", &User {
        id: 53,
        username: "john_ron".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    }, {
        ".id" => "[id]"
    });
}

#[cfg(feature = "ron")]
#[test]
fn test_with_random_value_ron_match() {
    assert_ron_snapshot!(
        &User {
            id: 53,
            username: "john_ron".to_string(),
            email: Email("john@example.com".to_string()),
            extra: "".to_string(),
        },
        match .. {
            ".id" => "[id]",
        }
    );
}

#[cfg(feature = "toml")]
#[test]
fn test_with_random_value_toml() {
    assert_toml_snapshot!("user_toml", &User {
        id: 53,
        username: "john_ron".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    }, {
        ".id" => "[id]"
    });
}

#[cfg(feature = "toml")]
#[test]
fn test_with_random_value_toml_match() {
    assert_toml_snapshot!(
        &User {
            id: 53,
            username: "john_ron".to_string(),
            email: Email("john@example.com".to_string()),
            extra: "".to_string(),
        },
        match .. {
            ".id" => "[id]",
        }
    );
}

#[cfg(feature = "json")]
#[test]
fn test_with_random_value_json() {
    assert_json_snapshot!("user_json", &User {
        id: 9999,
        username: "jason_doe".to_string(),
        email: Email("jason@example.com".to_string()),
        extra: "ssn goes here".to_string(),
    }, {
        ".id" => "[id]",
        ".extra" => "[extra]"
    });
}

#[cfg(feature = "json")]
#[test]
fn test_with_random_value_json_match() {
    assert_json_snapshot!(
        &User {
            id: 9999,
            username: "jason_doe".to_string(),
            email: Email("jason@example.com".to_string()),
            extra: "ssn goes here".to_string(),
        },
        match .. {
            ".id" => "[id]",
            ".extra" => "[extra]",
        }
    );
}

#[cfg(feature = "json")]
#[test]
fn test_with_random_value_json_settings() {
    let mut settings = insta::Settings::new();
    settings.add_redaction(".id", "[id]");
    settings.add_redaction(".extra", "[extra]");
    settings.bind(|| {
        assert_json_snapshot!(
            "user_json_settings",
            &User {
                id: 122,
                username: "jason_doe".to_string(),
                email: Email("jason@example.com".to_string()),
                extra: "ssn goes here".to_string(),
            }
        );
    });
}

#[cfg(feature = "json")]
#[test]
fn test_with_callbacks() {
    let mut settings = insta::Settings::new();
    settings.add_dynamic_redaction(".id", |value, path| {
        similar_asserts::assert_eq!(path.to_string(), ".id");
        similar_asserts::assert_eq!(value.as_u64().unwrap(), 1234);
        "[id]"
    });
    settings.bind(|| {
        assert_json_snapshot!(
            "user_json_settings_callback",
            &User {
                id: 1234,
                username: "jason_doe".to_string(),
                email: Email("jason@example.com".to_string()),
                extra: "extra here".to_string(),
            }
        );
    });
}

#[cfg(feature = "json")]
#[test]
fn test_with_random_value_json_settings2() {
    insta::with_settings!({redactions => vec![
        (".id", "[id]".into()),
        (".extra", "[extra]".into()),
    ]}, {
        assert_json_snapshot!(
            &User {
                id: 975,
                username: "jason_doe".to_string(),
                email: Email("jason@example.com".to_string()),
                extra: "ssn goes here".to_string(),
            }
        );
    });
}

#[cfg(feature = "json")]
#[test]
fn test_redact_newtype_struct() {
    #[derive(Serialize)]
    pub struct UserWrapper(User);

    let wrapper = UserWrapper(User {
        id: 42,
        username: "john_doe".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    });

    assert_json_snapshot!(wrapper, {
        r#".id"# => "[id]"
    }, @r#"
    {
      "id": "[id]",
      "username": "john_doe",
      "email": "john@example.com",
      "extra": ""
    }
    "#);
}

#[cfg(feature = "yaml")]
#[test]
fn test_redact_newtype_enum() {
    #[derive(Serialize)]
    pub enum Role {
        Admin(User),
        Visitor { id: String, name: String },
    }

    let visitor = Role::Visitor {
        id: "my-id".into(),
        name: "my-name".into(),
    };
    assert_yaml_snapshot!(visitor, {
        r#".id"# => "[id]",
    }, @r#"
    Visitor:
      id: "[id]"
      name: my-name
    "#);

    let admin = Role::Admin(User {
        id: 42,
        username: "john_doe".to_string(),
        email: Email("john@example.com".to_string()),
        extra: "".to_string(),
    });
    assert_yaml_snapshot!(admin, {
        r#".id"# => "[id]",
    }, @r#"
    Admin:
      id: "[id]"
      username: john_doe
      email: john@example.com
      extra: ""
    "#);
}

#[cfg(feature = "json")]
#[test]
fn test_redact_recursive() {
    #[derive(Serialize)]
    pub struct Node {
        id: u64,
        next: Option<Box<Node>>,
    }

    let root = Node {
        id: 0,
        next: Some(Box::new(Node { id: 1, next: None })),
    };

    assert_json_snapshot!(root, {
        ".**.id" => "[id]",
    }, @r#"
    {
      "id": "[id]",
      "next": {
        "id": "[id]",
        "next": null
      }
    }
    "#);
}

#[cfg(feature = "yaml")]
#[test]
fn test_struct_array_redaction() {
    #[derive(Serialize)]
    pub struct Product {
        _id: String,
        product_name: String,
    }

    #[derive(Serialize)]
    pub struct Checkout {
        _id: String,
        products: Vec<Product>,
    }

    let checkout = Checkout {
        _id: "checkout/1".to_string(),
        products: vec![
            Product {
                _id: "product/1".to_string(),
                product_name: "a car".to_string(),
            },
            Product {
                _id: "product/2".to_string(),
                product_name: "a boat".to_string(),
            },
        ],
    };

    assert_yaml_snapshot!(vec![checkout], {
        "[]._id" => "[checkout_id]",
        "[].products[]._id" => "[product_id]",
        "[].products[].product_name" => "[product_name]",
    });
}

#[cfg(feature = "yaml")]
#[test]
fn test_map_key_redaction() {
    #[derive(Serialize, Hash, PartialEq, PartialOrd, Eq, Ord)]
    struct Key {
        bucket: u32,
        value: u32,
    }

    #[derive(Serialize)]
    struct Foo {
        hm: std::collections::HashMap<Key, u32>,
        btm: std::collections::BTreeMap<(u32, u32), u32>,
    }

    let mut hm = std::collections::HashMap::new();
    hm.insert(
        Key {
            bucket: 1,
            value: 0,
        },
        42,
    );
    let mut btm = std::collections::BTreeMap::new();
    btm.insert((0, 0), 23);
    let foo_value = Foo { hm, btm };

    assert_yaml_snapshot!(foo_value, {
        ".hm.$key.bucket" => "[bucket]",
        ".btm.$key" => "[key]",
    });
}

#[cfg(feature = "json")]
#[test]
fn test_ordering() {
    #[derive(Debug, Serialize)]
    pub struct User {
        id: u64,
        username: String,
        flags: std::collections::HashSet<String>,
    }

    let mut settings = insta::Settings::new();
    settings.add_redaction(".id", "[id]");
    settings.sort_selector(".flags");
    settings.bind(|| {
        assert_json_snapshot!(
            "user_json_flags",
            &User {
                id: 122,
                username: "jason_doe".to_string(),
                flags: vec!["zzz".into(), "foo".into(), "aha".into(), "is_admin".into()]
                    .into_iter()
                    .collect(),
            }
        );
    });
}

#[cfg(feature = "json")]
#[test]
fn test_ordering_newtype_set() {
    #[derive(Debug, Serialize)]
    pub struct MySet(std::collections::HashSet<String>);

    #[derive(Debug, Serialize)]
    pub struct User {
        id: u64,
        username: String,
        flags: MySet,
    }

    assert_json_snapshot!(
        "user_json_flags_alt",
        &User {
            id: 122,
            username: "jason_doe".to_string(),
            flags: MySet(vec!["zzz".into(), "foo".into(), "aha".into(), "is_admin".into()]
                .into_iter()
                .collect()),
        },
        {
            "." => insta::sorted_redaction(),
            ".flags" => insta::sorted_redaction()
        }
    );
}

#[cfg(feature = "json")]
#[test]
fn test_rounded_redaction() {
    #[derive(Debug, Serialize)]
    pub struct MyPoint {
        x: f64,
        y: f64,
    }

    assert_json_snapshot!(
        "rounded_redaction",
        &MyPoint {
            x: 1.0 / 3.0,
            y: 6.0 / 3.0,
        },
        {
            ".x" => insta::rounded_redaction(4),
            ".y" => insta::rounded_redaction(4),
        }
    );
}

#[cfg(feature = "yaml")]
#[test]
fn test_named_redacted_with_debug_expr() {
    // This test demonstrates the form with a name, redactions, and debug expression
    // | File, redacted, named, debug expr | `assert_yaml_snapshot!("name", expr, {"." => sorted_redaction()}, "debug_expr")` |

    #[derive(Serialize, Debug)]
    pub struct ComplexObject {
        id: u32,
        items: Vec<String>,
        metadata: std::collections::HashMap<String, u32>,
    }

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("count".to_string(), 42);
    metadata.insert("version".to_string(), 123);

    let complex_obj = ComplexObject {
        id: 12345,
        items: vec!["one".to_string(), "two".to_string(), "three".to_string()],
        metadata,
    };

    // Now that we've added support for this form, we can test it directly
    assert_yaml_snapshot!(
        "named_redacted_debug_expr",
        &complex_obj,
        {
            ".id" => "[id]",
            ".metadata" => insta::sorted_redaction()
        },
        "This is a custom debug expression for the snapshot"
    );
}

#[cfg(feature = "yaml")]
#[test]
fn test_named_redacted_supported_form() {
    #[derive(Serialize, Debug)]
    pub struct ComplexObject {
        id: u32,
        items: Vec<String>,
        metadata: std::collections::HashMap<String, u32>,
    }

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("count".to_string(), 42);
    metadata.insert("version".to_string(), 123);

    let obj = ComplexObject {
        id: 12345,
        items: vec!["one".to_string(), "two".to_string(), "three".to_string()],
        metadata,
    };

    assert_yaml_snapshot!(
        "named_redacted_supported",
        &obj,
        {
            ".id" => "[id]",
            ".metadata" => insta::sorted_redaction()
        }
    );
}
