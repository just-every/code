use std::fmt::{Display, Write};

use crate::content::Content;

/// The maximum number of characters to print in a single line
/// when [`to_string_pretty`] is used.
const COMPACT_MAX_CHARS: usize = 120;

pub fn format_float<T: Display>(value: T) -> String {
    let mut rv = format!("{}", value);
    if !rv.contains('.') {
        rv.push_str(".0");
    }
    rv
}

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Format {
    Condensed,
    SingleLine,
    Pretty,
}

/// Serializes a serializable to JSON.
pub struct Serializer {
    out: String,
    format: Format,
    indentation: usize,
}

impl Serializer {
    /// Creates a new [`Serializer`] that writes into the given writer.
    pub fn new() -> Serializer {
        Serializer {
            out: String::new(),
            format: Format::Condensed,
            indentation: 0,
        }
    }

    pub fn into_result(self) -> String {
        self.out
    }

    fn write_indentation(&mut self) {
        if self.format == Format::Pretty {
            write!(self.out, "{: ^1$}", "", self.indentation * 2).unwrap();
        }
    }

    fn start_container(&mut self, c: char) {
        self.write_char(c);
        self.indentation += 1;
    }

    fn end_container(&mut self, c: char, empty: bool) {
        self.indentation -= 1;
        if self.format == Format::Pretty && !empty {
            self.write_char('\n');
            self.write_indentation();
        }
        self.write_char(c);
    }

    fn write_comma(&mut self, first: bool) {
        match self.format {
            Format::Pretty => {
                if first {
                    self.write_char('\n');
                } else {
                    self.write_str(",\n");
                }
                self.write_indentation();
            }
            Format::Condensed => {
                if !first {
                    self.write_char(',');
                }
            }
            Format::SingleLine => {
                if !first {
                    self.write_str(", ");
                }
            }
        }
    }

    fn write_colon(&mut self) {
        match self.format {
            Format::Pretty | Format::SingleLine => self.write_str(": "),
            Format::Condensed => self.write_char(':'),
        }
    }

    fn serialize_array(&mut self, items: &[Content]) {
        self.start_container('[');
        for (idx, item) in items.iter().enumerate() {
            self.write_comma(idx == 0);
            self.serialize(item);
        }
        self.end_container(']', items.is_empty());
    }

    fn serialize_object(&mut self, fields: &[(&str, Content)]) {
        self.start_container('{');
        for (idx, (key, value)) in fields.iter().enumerate() {
            self.write_comma(idx == 0);
            self.write_escaped_str(key);
            self.write_colon();
            self.serialize(value);
        }
        self.end_container('}', fields.is_empty());
    }

    pub fn serialize(&mut self, value: &Content) {
        match value {
            Content::Bool(true) => self.write_str("true"),
            Content::Bool(false) => self.write_str("false"),
            Content::U8(n) => write!(self.out, "{}", n).unwrap(),
            Content::U16(n) => write!(self.out, "{}", n).unwrap(),
            Content::U32(n) => write!(self.out, "{}", n).unwrap(),
            Content::U64(n) => write!(self.out, "{}", n).unwrap(),
            Content::U128(n) => write!(self.out, "{}", n).unwrap(),
            Content::I8(n) => write!(self.out, "{}", n).unwrap(),
            Content::I16(n) => write!(self.out, "{}", n).unwrap(),
            Content::I32(n) => write!(self.out, "{}", n).unwrap(),
            Content::I64(n) => write!(self.out, "{}", n).unwrap(),
            Content::I128(n) => write!(self.out, "{}", n).unwrap(),
            Content::F32(f) => {
                if f.is_finite() {
                    self.write_str(&format_float(f));
                } else {
                    self.write_str("null")
                }
            }
            Content::F64(f) => {
                if f.is_finite() {
                    self.write_str(&format_float(f));
                } else {
                    self.write_str("null")
                }
            }
            Content::Char(c) => self.write_escaped_str(&(*c).to_string()),
            Content::String(s) => self.write_escaped_str(s),
            Content::Bytes(bytes) => {
                self.start_container('[');
                for (idx, byte) in bytes.iter().enumerate() {
                    self.write_comma(idx == 0);
                    self.write_str(&byte.to_string());
                }
                self.end_container(']', bytes.is_empty());
            }
            Content::None | Content::Unit | Content::UnitStruct(_) => self.write_str("null"),
            Content::Some(content) => self.serialize(content),
            Content::UnitVariant(_, _, variant) => self.write_escaped_str(variant),
            Content::NewtypeStruct(_, content) => self.serialize(content),
            Content::NewtypeVariant(_, _, variant, content) => {
                self.start_container('{');
                self.write_comma(true);
                self.write_escaped_str(variant);
                self.write_colon();
                self.serialize(content);
                self.end_container('}', false);
            }
            Content::Seq(seq) | Content::Tuple(seq) | Content::TupleStruct(_, seq) => {
                self.serialize_array(seq);
            }
            Content::TupleVariant(_, _, variant, seq) => {
                self.start_container('{');
                self.write_comma(true);
                self.write_escaped_str(variant);
                self.write_colon();
                self.serialize_array(seq);
                self.end_container('}', false);
            }
            Content::Map(map) => {
                self.start_container('{');
                for (idx, (key, value)) in map.iter().enumerate() {
                    self.write_comma(idx == 0);
                    let real_key = key.resolve_inner();
                    if let Content::String(ref s) = real_key {
                        self.write_escaped_str(s);
                    } else if let Some(num) = real_key.as_i64() {
                        self.write_escaped_str(&num.to_string());
                    } else if let Some(num) = real_key.as_i128() {
                        self.write_escaped_str(&num.to_string());
                    } else {
                        panic!("cannot serialize maps without string keys to JSON");
                    }
                    self.write_colon();
                    self.serialize(value);
                }
                self.end_container('}', map.is_empty());
            }
            Content::Struct(_, fields) => {
                self.serialize_object(fields);
            }
            Content::StructVariant(_, _, variant, fields) => {
                self.start_container('{');
                self.write_comma(true);
                self.write_escaped_str(variant);
                self.write_colon();
                self.serialize_object(fields);
                self.end_container('}', false);
            }
        }
    }

    fn write_str(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn write_char(&mut self, c: char) {
        self.out.push(c);
    }

    fn write_escaped_str(&mut self, value: &str) {
        self.write_char('"');

        let bytes = value.as_bytes();
        let mut start = 0;

        for (i, &byte) in bytes.iter().enumerate() {
            let escape = ESCAPE[byte as usize];
            if escape == 0 {
                continue;
            }

            if start < i {
                self.write_str(&value[start..i]);
            }

            match escape {
                self::BB => self.write_str("\\b"),
                self::TT => self.write_str("\\t"),
                self::NN => self.write_str("\\n"),
                self::FF => self.write_str("\\f"),
                self::RR => self.write_str("\\r"),
                self::QU => self.write_str("\\\""),
                self::BS => self.write_str("\\\\"),
                self::U => {
                    static HEX_DIGITS: [u8; 16] = *b"0123456789abcdef";
                    self.write_str("\\u00");
                    self.write_char(HEX_DIGITS[(byte >> 4) as usize] as char);
                    self.write_char(HEX_DIGITS[(byte & 0xF) as usize] as char);
                }
                _ => unreachable!(),
            }

            start = i + 1;
        }

        if start != bytes.len() {
            self.write_str(&value[start..]);
        }

        self.write_char('"');
    }
}

const BB: u8 = b'b'; // \x08
const TT: u8 = b't'; // \x09
const NN: u8 = b'n'; // \x0A
const FF: u8 = b'f'; // \x0C
const RR: u8 = b'r'; // \x0D
const QU: u8 = b'"'; // \x22
const BS: u8 = b'\\'; // \x5C
const U: u8 = b'u'; // \x00...\x1F except the ones above

// Lookup table of escape sequences. A value of b'x' at index i means that byte
// i is escaped as "\x" in JSON. A value of 0 means that byte i is not escaped.
#[rustfmt::skip]
static ESCAPE: [u8; 256] = [
    //  1   2   3   4   5   6   7   8   9   A   B   C   D   E   F
    U,  U,  U,  U,  U,  U,  U,  U, BB, TT, NN,  U, FF, RR,  U,  U, // 0
    U,  U,  U,  U,  U,  U,  U,  U,  U,  U,  U,  U,  U,  U,  U,  U, // 1
    0,  0, QU,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 2
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 3
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 4
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, BS,  0,  0,  0, // 5
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 6
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 7
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 8
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // 9
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // A
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // B
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // C
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // D
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // E
    0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0,  0, // F
];

/// Serializes a value to JSON.
pub fn to_string(value: &Content) -> String {
    let mut ser = Serializer::new();
    ser.serialize(value);
    ser.into_result()
}

/// Serializes a value to JSON in single-line format.
#[allow(unused)]
pub fn to_string_compact(value: &Content) -> String {
    let mut ser = Serializer::new();
    ser.format = Format::SingleLine;
    ser.serialize(value);
    let rv = ser.into_result();
    // this is pretty wasteful as we just format twice
    // but it's acceptable for the way this is used in
    // insta.
    if rv.chars().count() > COMPACT_MAX_CHARS {
        to_string_pretty(value)
    } else {
        rv
    }
}

/// Serializes a value to JSON pretty
#[allow(unused)]
pub fn to_string_pretty(value: &Content) -> String {
    let mut ser = Serializer::new();
    ser.format = Format::Pretty;
    ser.serialize(value);
    ser.into_result()
}

#[test]
fn test_to_string() {
    let json = to_string(&Content::Map(vec![
        (
            Content::from("environments"),
            Content::Seq(vec![
                Content::from("development"),
                Content::from("production"),
            ]),
        ),
        (Content::from("cmdline"), Content::Seq(vec![])),
        (Content::from("extra"), Content::Map(vec![])),
    ]));
    crate::assert_snapshot!(&json, @r#"{"environments":["development","production"],"cmdline":[],"extra":{}}"#);
}

#[test]
fn test_to_string_pretty() {
    let json = to_string_pretty(&Content::Map(vec![
        (
            Content::from("environments"),
            Content::Seq(vec![
                Content::from("development"),
                Content::from("production"),
            ]),
        ),
        (Content::from("cmdline"), Content::Seq(vec![])),
        (Content::from("extra"), Content::Map(vec![])),
    ]));
    crate::assert_snapshot!(&json, @r#"
    {
      "environments": [
        "development",
        "production"
      ],
      "cmdline": [],
      "extra": {}
    }
    "#);
}

#[test]
fn test_to_string_num_keys() {
    let content = Content::Map(vec![
        (Content::from(42u32), Content::from(true)),
        (Content::from(-23i32), Content::from(false)),
    ]);
    let json = to_string_pretty(&content);
    crate::assert_snapshot!(&json, @r#"
    {
      "42": true,
      "-23": false
    }
    "#);
}

#[test]
fn test_to_string_pretty_complex() {
    let content = Content::Map(vec![
        (
            Content::from("is_alive"),
            Content::NewtypeStruct("Some", Content::from(true).into()),
        ),
        (
            Content::from("newtype_variant"),
            Content::NewtypeVariant(
                "Foo",
                0,
                "variant_a",
                Box::new(Content::Struct(
                    "VariantA",
                    vec![
                        ("field_a", Content::String("value_a".into())),
                        ("field_b", 42u32.into()),
                    ],
                )),
            ),
        ),
        (
            Content::from("struct_variant"),
            Content::StructVariant(
                "Foo",
                0,
                "variant_b",
                vec![
                    ("field_a", Content::String("value_a".into())),
                    ("field_b", 42u32.into()),
                ],
            ),
        ),
        (
            Content::from("tuple_variant"),
            Content::TupleVariant(
                "Foo",
                0,
                "variant_c",
                vec![(Content::String("value_a".into())), (42u32.into())],
            ),
        ),
        (Content::from("empty_array"), Content::Seq(vec![])),
        (Content::from("empty_object"), Content::Map(vec![])),
        (Content::from("array"), Content::Seq(vec![true.into()])),
        (
            Content::from("object"),
            Content::Map(vec![("foo".into(), true.into())]),
        ),
        (
            Content::from("array_of_objects"),
            Content::Seq(vec![Content::Struct(
                "MyType",
                vec![
                    ("foo", Content::from("bar".to_string())),
                    ("bar", Content::from("xxx".to_string())),
                ],
            )]),
        ),
        (
            Content::from("unit_variant"),
            Content::UnitVariant("Stuff", 0, "value"),
        ),
        (Content::from("u8"), Content::U8(8)),
        (Content::from("u16"), Content::U16(16)),
        (Content::from("u32"), Content::U32(32)),
        (Content::from("u64"), Content::U64(64)),
        (Content::from("u128"), Content::U128(128)),
        (Content::from("i8"), Content::I8(8)),
        (Content::from("i16"), Content::I16(16)),
        (Content::from("i32"), Content::I32(32)),
        (Content::from("i64"), Content::I64(64)),
        (Content::from("i128"), Content::I128(128)),
        (Content::from("f32"), Content::F32(32.0)),
        (Content::from("f64"), Content::F64(64.0)),
        (Content::from("char"), Content::Char('A')),
        (Content::from("bytes"), Content::Bytes(b"hehe".to_vec())),
        (Content::from("null"), Content::None),
        (Content::from("unit"), Content::Unit),
        (
            Content::from("crazy_string"),
            Content::String((0u8..=126).map(|x| x as char).collect()),
        ),
    ]);
    let json = to_string_pretty(&content);

    crate::assert_snapshot!(&json, @r##"
    {
      "is_alive": true,
      "newtype_variant": {
        "variant_a": {
          "field_a": "value_a",
          "field_b": 42
        }
      },
      "struct_variant": {
        "variant_b": {
          "field_a": "value_a",
          "field_b": 42
        }
      },
      "tuple_variant": {
        "variant_c": [
          "value_a",
          42
        ]
      },
      "empty_array": [],
      "empty_object": {},
      "array": [
        true
      ],
      "object": {
        "foo": true
      },
      "array_of_objects": [
        {
          "foo": "bar",
          "bar": "xxx"
        }
      ],
      "unit_variant": "value",
      "u8": 8,
      "u16": 16,
      "u32": 32,
      "u64": 64,
      "u128": 128,
      "i8": 8,
      "i16": 16,
      "i32": 32,
      "i64": 64,
      "i128": 128,
      "f32": 32.0,
      "f64": 64.0,
      "char": "A",
      "bytes": [
        104,
        101,
        104,
        101
      ],
      "null": null,
      "unit": null,
      "crazy_string": "\u0000\u0001\u0002\u0003\u0004\u0005\u0006\u0007\b\t\n\u000b\f\r\u000e\u000f\u0010\u0011\u0012\u0013\u0014\u0015\u0016\u0017\u0018\u0019\u001a\u001b\u001c\u001d\u001e\u001f !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~"
    }
    "##);
}
