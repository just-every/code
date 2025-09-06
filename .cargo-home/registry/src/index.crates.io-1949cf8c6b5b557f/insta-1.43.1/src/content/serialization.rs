use std::cmp::Ordering;
use std::marker::PhantomData;

use crate::content::Content;

use serde::{ser, Serialize, Serializer};

#[derive(PartialEq, Debug)]
pub enum Key<'a> {
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    U128(u128),
    I128(i128),
    Str(&'a str),
    Bytes(&'a [u8]),
    Other,
}

impl Key<'_> {
    /// Needed because [`std::mem::discriminant`] is not [`Ord`]
    fn discriminant(&self) -> usize {
        match self {
            Key::Bool(_) => 1,
            Key::U64(_) => 2,
            Key::I64(_) => 3,
            Key::F64(_) => 4,
            Key::U128(_) => 5,
            Key::I128(_) => 6,
            Key::Str(_) => 7,
            Key::Bytes(_) => 8,
            Key::Other => 9,
        }
    }
}

impl Eq for Key<'_> {}

impl Ord for Key<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        let self_discriminant = self.discriminant();
        let other_discriminant = other.discriminant();
        match Ord::cmp(&self_discriminant, &other_discriminant) {
            Ordering::Equal => match (self, other) {
                (Key::Bool(a), Key::Bool(b)) => Ord::cmp(a, b),
                (Key::U64(a), Key::U64(b)) => Ord::cmp(a, b),
                (Key::I64(a), Key::I64(b)) => Ord::cmp(a, b),
                (Key::F64(a), Key::F64(b)) => f64_total_cmp(*a, *b),
                (Key::U128(a), Key::U128(b)) => Ord::cmp(a, b),
                (Key::I128(a), Key::I128(b)) => Ord::cmp(a, b),
                (Key::Str(a), Key::Str(b)) => Ord::cmp(a, b),
                (Key::Bytes(a), Key::Bytes(b)) => Ord::cmp(a, b),
                _ => Ordering::Equal,
            },
            cmp => cmp,
        }
    }
}

impl PartialOrd for Key<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn f64_total_cmp(left: f64, right: f64) -> Ordering {
    // this is taken from f64::total_cmp on newer rust versions
    let mut left = left.to_bits() as i64;
    let mut right = right.to_bits() as i64;
    left ^= (((left >> 63) as u64) >> 1) as i64;
    right ^= (((right >> 63) as u64) >> 1) as i64;
    left.cmp(&right)
}

impl Content {
    pub(crate) fn as_key(&self) -> Key<'_> {
        match *self.resolve_inner() {
            Content::Bool(val) => Key::Bool(val),
            Content::Char(val) => Key::U64(val as u64),
            Content::U16(val) => Key::U64(val.into()),
            Content::U32(val) => Key::U64(val.into()),
            Content::U64(val) => Key::U64(val),
            Content::U128(val) => Key::U128(val),
            Content::I16(val) => Key::I64(val.into()),
            Content::I32(val) => Key::I64(val.into()),
            Content::I64(val) => Key::I64(val),
            Content::I128(val) => Key::I128(val),
            Content::F32(val) => Key::F64(val.into()),
            Content::F64(val) => Key::F64(val),
            Content::String(ref val) => Key::Str(val.as_str()),
            Content::Bytes(ref val) => Key::Bytes(&val[..]),
            _ => Key::Other,
        }
    }

    pub(crate) fn sort_maps(&mut self) {
        self.walk(&mut |content| {
            if let Content::Map(ref mut items) = content {
                // try to compare by key first, if that fails compare by the
                // object value.  That way some values normalize, and if we
                // can't normalize we still have a stable order.
                items.sort_by(|a, b| match (a.0.as_key(), b.0.as_key()) {
                    (Key::Other, _) | (_, Key::Other) => {
                        a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal)
                    }
                    (ref a, ref b) => a.cmp(b),
                })
            }
            true
        })
    }
}

#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl Serialize for Content {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match *self {
            Content::Bool(b) => serializer.serialize_bool(b),
            Content::U8(u) => serializer.serialize_u8(u),
            Content::U16(u) => serializer.serialize_u16(u),
            Content::U32(u) => serializer.serialize_u32(u),
            Content::U64(u) => serializer.serialize_u64(u),
            Content::U128(u) => serializer.serialize_u128(u),
            Content::I8(i) => serializer.serialize_i8(i),
            Content::I16(i) => serializer.serialize_i16(i),
            Content::I32(i) => serializer.serialize_i32(i),
            Content::I64(i) => serializer.serialize_i64(i),
            Content::I128(i) => serializer.serialize_i128(i),
            Content::F32(f) => serializer.serialize_f32(f),
            Content::F64(f) => serializer.serialize_f64(f),
            Content::Char(c) => serializer.serialize_char(c),
            Content::String(ref s) => serializer.serialize_str(s),
            Content::Bytes(ref b) => serializer.serialize_bytes(b),
            Content::None => serializer.serialize_none(),
            Content::Some(ref c) => serializer.serialize_some(&**c),
            Content::Unit => serializer.serialize_unit(),
            Content::UnitStruct(n) => serializer.serialize_unit_struct(n),
            Content::UnitVariant(n, i, v) => serializer.serialize_unit_variant(n, i, v),
            Content::NewtypeStruct(n, ref c) => serializer.serialize_newtype_struct(n, &**c),
            Content::NewtypeVariant(n, i, v, ref c) => {
                serializer.serialize_newtype_variant(n, i, v, &**c)
            }
            Content::Seq(ref elements) => elements.serialize(serializer),
            Content::Tuple(ref elements) => {
                use serde::ser::SerializeTuple;
                let mut tuple = serializer.serialize_tuple(elements.len())?;
                for e in elements {
                    tuple.serialize_element(e)?;
                }
                tuple.end()
            }
            Content::TupleStruct(n, ref fields) => {
                use serde::ser::SerializeTupleStruct;
                let mut ts = serializer.serialize_tuple_struct(n, fields.len())?;
                for f in fields {
                    ts.serialize_field(f)?;
                }
                ts.end()
            }
            Content::TupleVariant(n, i, v, ref fields) => {
                use serde::ser::SerializeTupleVariant;
                let mut tv = serializer.serialize_tuple_variant(n, i, v, fields.len())?;
                for f in fields {
                    tv.serialize_field(f)?;
                }
                tv.end()
            }
            Content::Map(ref entries) => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (k, v) in entries {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            Content::Struct(n, ref fields) => {
                use serde::ser::SerializeStruct;
                let mut s = serializer.serialize_struct(n, fields.len())?;
                for &(k, ref v) in fields {
                    s.serialize_field(k, v)?;
                }
                s.end()
            }
            Content::StructVariant(n, i, v, ref fields) => {
                use serde::ser::SerializeStructVariant;
                let mut sv = serializer.serialize_struct_variant(n, i, v, fields.len())?;
                for &(k, ref v) in fields {
                    sv.serialize_field(k, v)?;
                }
                sv.end()
            }
        }
    }
}

pub struct ContentSerializer<E> {
    error: PhantomData<E>,
}

impl<E> ContentSerializer<E> {
    pub fn new() -> Self {
        ContentSerializer { error: PhantomData }
    }
}

impl<E> Serializer for ContentSerializer<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    type SerializeSeq = SerializeSeq<E>;
    type SerializeTuple = SerializeTuple<E>;
    type SerializeTupleStruct = SerializeTupleStruct<E>;
    type SerializeTupleVariant = SerializeTupleVariant<E>;
    type SerializeMap = SerializeMap<E>;
    type SerializeStruct = SerializeStruct<E>;
    type SerializeStructVariant = SerializeStructVariant<E>;

    fn serialize_bool(self, v: bool) -> Result<Content, E> {
        Ok(Content::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Content, E> {
        Ok(Content::I8(v))
    }

    fn serialize_i16(self, v: i16) -> Result<Content, E> {
        Ok(Content::I16(v))
    }

    fn serialize_i32(self, v: i32) -> Result<Content, E> {
        Ok(Content::I32(v))
    }

    fn serialize_i64(self, v: i64) -> Result<Content, E> {
        Ok(Content::I64(v))
    }

    fn serialize_i128(self, v: i128) -> Result<Content, E> {
        Ok(Content::I128(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Content, E> {
        Ok(Content::U8(v))
    }

    fn serialize_u16(self, v: u16) -> Result<Content, E> {
        Ok(Content::U16(v))
    }

    fn serialize_u32(self, v: u32) -> Result<Content, E> {
        Ok(Content::U32(v))
    }

    fn serialize_u64(self, v: u64) -> Result<Content, E> {
        Ok(Content::U64(v))
    }

    fn serialize_u128(self, v: u128) -> Result<Content, E> {
        Ok(Content::U128(v))
    }

    fn serialize_f32(self, v: f32) -> Result<Content, E> {
        Ok(Content::F32(v))
    }

    fn serialize_f64(self, v: f64) -> Result<Content, E> {
        Ok(Content::F64(v))
    }

    fn serialize_char(self, v: char) -> Result<Content, E> {
        Ok(Content::Char(v))
    }

    fn serialize_str(self, value: &str) -> Result<Content, E> {
        Ok(Content::String(value.to_owned()))
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Content, E> {
        Ok(Content::Bytes(value.to_owned()))
    }

    fn serialize_none(self) -> Result<Content, E> {
        Ok(Content::None)
    }

    fn serialize_some<T>(self, value: &T) -> Result<Content, E>
    where
        T: Serialize + ?Sized,
    {
        Ok(Content::Some(Box::new(value.serialize(self)?)))
    }

    fn serialize_unit(self) -> Result<Content, E> {
        Ok(Content::Unit)
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Content, E> {
        Ok(Content::UnitStruct(name))
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
    ) -> Result<Content, E> {
        Ok(Content::UnitVariant(name, variant_index, variant))
    }

    fn serialize_newtype_struct<T>(self, name: &'static str, value: &T) -> Result<Content, E>
    where
        T: Serialize + ?Sized,
    {
        Ok(Content::NewtypeStruct(
            name,
            Box::new(value.serialize(self)?),
        ))
    }

    fn serialize_newtype_variant<T>(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Content, E>
    where
        T: Serialize + ?Sized,
    {
        Ok(Content::NewtypeVariant(
            name,
            variant_index,
            variant,
            Box::new(value.serialize(self)?),
        ))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, E> {
        Ok(SerializeSeq {
            elements: Vec::with_capacity(len.unwrap_or(0)),
            error: PhantomData,
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, E> {
        Ok(SerializeTuple {
            elements: Vec::with_capacity(len),
            error: PhantomData,
        })
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, E> {
        Ok(SerializeTupleStruct {
            name,
            fields: Vec::with_capacity(len),
            error: PhantomData,
        })
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, E> {
        Ok(SerializeTupleVariant {
            name,
            variant_index,
            variant,
            fields: Vec::with_capacity(len),
            error: PhantomData,
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, E> {
        Ok(SerializeMap {
            entries: Vec::with_capacity(len.unwrap_or(0)),
            key: None,
            error: PhantomData,
        })
    }

    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct, E> {
        Ok(SerializeStruct {
            name,
            fields: Vec::with_capacity(len),
            error: PhantomData,
        })
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, E> {
        Ok(SerializeStructVariant {
            name,
            variant_index,
            variant,
            fields: Vec::with_capacity(len),
            error: PhantomData,
        })
    }
}

pub struct SerializeSeq<E> {
    elements: Vec<Content>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeSeq for SerializeSeq<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.elements.push(value);
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::Seq(self.elements))
    }
}

pub struct SerializeTuple<E> {
    elements: Vec<Content>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeTuple for SerializeTuple<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.elements.push(value);
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::Tuple(self.elements))
    }
}

pub struct SerializeTupleStruct<E> {
    name: &'static str,
    fields: Vec<Content>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeTupleStruct for SerializeTupleStruct<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.fields.push(value);
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::TupleStruct(self.name, self.fields))
    }
}

pub struct SerializeTupleVariant<E> {
    name: &'static str,
    variant_index: u32,
    variant: &'static str,
    fields: Vec<Content>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeTupleVariant for SerializeTupleVariant<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.fields.push(value);
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::TupleVariant(
            self.name,
            self.variant_index,
            self.variant,
            self.fields,
        ))
    }
}

pub struct SerializeMap<E> {
    entries: Vec<(Content, Content)>,
    key: Option<Content>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeMap for SerializeMap<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let key = key.serialize(ContentSerializer::<E>::new())?;
        self.key = Some(key);
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let key = self
            .key
            .take()
            .expect("serialize_value called before serialize_key");
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.entries.push((key, value));
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::Map(self.entries))
    }

    fn serialize_entry<K, V>(&mut self, key: &K, value: &V) -> Result<(), E>
    where
        K: Serialize + ?Sized,
        V: Serialize + ?Sized,
    {
        let key = key.serialize(ContentSerializer::<E>::new())?;
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.entries.push((key, value));
        Ok(())
    }
}

pub struct SerializeStruct<E> {
    name: &'static str,
    fields: Vec<(&'static str, Content)>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeStruct for SerializeStruct<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.fields.push((key, value));
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::Struct(self.name, self.fields))
    }
}

pub struct SerializeStructVariant<E> {
    name: &'static str,
    variant_index: u32,
    variant: &'static str,
    fields: Vec<(&'static str, Content)>,
    error: PhantomData<E>,
}

impl<E> ser::SerializeStructVariant for SerializeStructVariant<E>
where
    E: ser::Error,
{
    type Ok = Content;
    type Error = E;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T) -> Result<(), E>
    where
        T: Serialize + ?Sized,
    {
        let value = value.serialize(ContentSerializer::<E>::new())?;
        self.fields.push((key, value));
        Ok(())
    }

    fn end(self) -> Result<Content, E> {
        Ok(Content::StructVariant(
            self.name,
            self.variant_index,
            self.variant,
            self.fields,
        ))
    }
}
