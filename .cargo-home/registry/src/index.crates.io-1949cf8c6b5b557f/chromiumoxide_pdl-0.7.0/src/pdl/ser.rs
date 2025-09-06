use serde::{
    ser::{SerializeMap, SerializeSeq},
    Serialize, Serializer,
};

use crate::pdl::*;

pub fn serialize_usize<S>(n: &usize, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&n.to_string())
}

pub fn serialize_enum<S>(variants: &[Variant], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(variants.len()))?;

    for variant in variants {
        seq.serialize_element(variant.name.as_ref())?;
    }

    seq.end()
}

pub fn serialize_redirect<S>(redirect: &Option<Redirect>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if let Some(redirect) = redirect {
        if let Some(name) = redirect.name.as_ref() {
            return serializer.serialize_str(name.as_ref());
        }
    }
    serializer.serialize_none()
}

pub fn is_false(v: &bool) -> bool {
    !*v
}

impl Protocol<'_> {
    /// Serialize the `Protocol` data structure as a String of JSON.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Serialize the `Protocol` data structure as a pretty-printed String of
    /// JSON.
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

impl Serialize for Type<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        match self {
            Type::Integer => {
                map.serialize_entry("type", "integer")?;
            }
            Type::Number => {
                map.serialize_entry("type", "number")?;
            }
            Type::Boolean => {
                map.serialize_entry("type", "boolean")?;
            }
            Type::String => {
                map.serialize_entry("type", "string")?;
            }
            Type::Object => {
                map.serialize_entry("type", "object")?;
            }
            Type::Any => {
                map.serialize_entry("type", "any")?;
            }
            Type::Binary => {
                map.serialize_entry("type", "binary")?;
            }
            Type::Enum(variants) => {
                map.serialize_entry("type", "string")?;
                map.serialize_entry(
                    "enum",
                    &variants
                        .iter()
                        .map(|variant| variant.name.as_ref())
                        .collect::<Vec<_>>(),
                )?;
            }
            Type::ArrayOf(ty) => {
                map.serialize_entry("type", "array")?;
                map.serialize_entry("items", &ty)?;
            }
            Type::Ref(id) => {
                map.serialize_entry("$ref", &id)?;
            }
        }

        map.end()
    }
}
