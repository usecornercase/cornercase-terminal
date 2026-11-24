use serde::Serialize;
use serde::ser::{self, Serializer};
use serde_json::{Error, Map, Value};

pub(super) fn every_field(value: &impl Serialize) -> Result<Value, Error> {
    value.serialize(EveryField)
}

struct EveryField;

macro_rules! plain {
    ($($method:ident($kind:ty)),* $(,)?) => {
        $(fn $method(self, value: $kind) -> Result<Value, Error> {
            serde_json::value::Serializer.$method(value)
        })*
    };
}

impl Serializer for EveryField {
    type Ok = Value;
    type Error = Error;
    type SerializeSeq = Items;
    type SerializeTuple = Items;
    type SerializeTupleStruct = Items;
    type SerializeTupleVariant = Items;
    type SerializeMap = Fields;
    type SerializeStruct = Fields;
    type SerializeStructVariant = Fields;

    plain!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_bytes(&[u8]),
    );

    fn serialize_none(self) -> Result<Value, Error> {
        Ok(Value::Null)
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Value, Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Value, Error> {
        Ok(Value::Null)
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Error> {
        Ok(Value::Null)
    }

    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Result<Value, Error> {
        Ok(Value::String(variant.to_string()))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _: &'static str, value: &T) -> Result<Value, Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, Error> {
        Ok(wrapped(Some(variant), value.serialize(self)?))
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Items, Error> {
        Ok(Items::new(None))
    }

    fn serialize_tuple(self, _: usize) -> Result<Items, Error> {
        Ok(Items::new(None))
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Items, Error> {
        Ok(Items::new(None))
    }

    fn serialize_tuple_variant(self, _: &'static str, _: u32, variant: &'static str, _: usize) -> Result<Items, Error> {
        Ok(Items::new(Some(variant)))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Fields, Error> {
        Ok(Fields::new(None))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Fields, Error> {
        Ok(Fields::new(None))
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Fields, Error> {
        Ok(Fields::new(Some(variant)))
    }
}

fn wrapped(variant: Option<&'static str>, value: Value) -> Value {
    match variant {
        Some(variant) => Value::Object(Map::from_iter([(variant.to_string(), value)])),
        None => value,
    }
}

struct Items {
    variant: Option<&'static str>,
    items: Vec<Value>,
}

impl Items {
    const fn new(variant: Option<&'static str>) -> Self {
        Self { variant, items: Vec::new() }
    }

    fn push<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.items.push(value.serialize(EveryField)?);
        Ok(())
    }

    fn done(self) -> Value {
        wrapped(self.variant, Value::Array(self.items))
    }
}

macro_rules! items {
    ($($trait:ident::$method:ident),*) => {
        $(impl ser::$trait for Items {
            type Ok = Value;
            type Error = Error;

            fn $method<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
                self.push(value)
            }

            fn end(self) -> Result<Value, Error> {
                Ok(self.done())
            }
        })*
    };
}

items!(
    SerializeSeq::serialize_element,
    SerializeTuple::serialize_element,
    SerializeTupleStruct::serialize_field,
    SerializeTupleVariant::serialize_field
);

struct Fields {
    variant: Option<&'static str>,
    entries: Map<String, Value>,
    key: String,
}

impl Fields {
    fn new(variant: Option<&'static str>) -> Self {
        Self { variant, entries: Map::new(), key: String::new() }
    }

    fn insert<T: ?Sized + Serialize>(&mut self, key: &str, value: &T) -> Result<(), Error> {
        self.entries.insert(key.to_string(), value.serialize(EveryField)?);
        Ok(())
    }

    fn done(self) -> Value {
        wrapped(self.variant, Value::Object(self.entries))
    }
}

impl ser::SerializeMap for Fields {
    type Ok = Value;
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        self.key = match key.serialize(EveryField)? {
            Value::String(key) => key,
            other => other.to_string(),
        };
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        let key = std::mem::take(&mut self.key);
        self.insert(&key, value)
    }

    fn end(self) -> Result<Value, Error> {
        Ok(self.done())
    }
}

macro_rules! fields {
    ($($trait:ident),*) => {
        $(impl ser::$trait for Fields {
            type Ok = Value;
            type Error = Error;

            fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
                self.insert(key, value)
            }

            fn skip_field(&mut self, key: &'static str) -> Result<(), Error> {
                self.entries.insert(key.to_string(), Value::Null);
                Ok(())
            }

            fn end(self) -> Result<Value, Error> {
                Ok(self.done())
            }
        })*
    };
}

fields!(SerializeStruct, SerializeStructVariant);
