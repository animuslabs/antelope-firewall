use core::{num, fmt};
use core::convert::TryInto;
use std::ops::{AddAssign, MulAssign, Neg};

use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};

use std;
use std::fmt::{Display};

use serde::{de as ser_de, ser as ser_ser};

pub type Result<T> = std::result::Result<T, Error>;

// This is a bare-bones implementation. A real library would provide additional
// information in its error type, for example the line and column at which the
// error occurred, the byte offset into the input, or the current key being
// processed.
#[derive(Debug)]
pub enum Error {
    // One or more variants that can be created by data structures through the
    // `ser::Error` and `de::Error` traits. For example the Serialize impl for
    // Mutex<T> might return an error because the mutex is poisoned, or the
    // Deserialize impl for a struct may return an error because a required
    // field is missing.
    Message(String),

    // Zero or more variants that can be created directly by the Serializer and
    // Deserializer without going through `ser::Error` and `de::Error`. These
    // are specific to the format, in this case JSON.
    Eof,
    Syntax,
    ExpectedBoolean,
    ExpectedInteger,
    ExpectedString,
    ExpectedNull,
    ExpectedArray,
    ExpectedArrayComma,
    ExpectedArrayEnd,
    ExpectedMap,
    ExpectedMapColon,
    ExpectedMapComma,
    ExpectedMapEnd,
    ExpectedEnum,
    TrailingCharacters,
}

impl ser_ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl ser_de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Message(msg) => formatter.write_str(msg),
            Error::Eof => formatter.write_str("unexpected end of input"),
            _ => todo!()
        }
    }
}

impl std::error::Error for Error {}

pub struct Deserializer<'de> {
    // This string starts with the input data and characters are truncated off
    // the beginning as data is parsed.
    input: &'de [u8],
}

impl<'de> Deserializer<'de> {
    // By convention, `Deserializer` constructors are named like `from_xyz`.
    // That way basic use cases are satisfied by something like
    // `serde_json::from_str(...)` while advanced use cases that require a
    // deserializer can make one with `serde_json::Deserializer::from_str(...)`.
    pub fn from_bytes(input: &'de [u8]) -> Self {
        Deserializer { input }
    }
}

// By convention, the public API of a Serde deserializer is one or more
// `from_xyz` methods such as `from_str`, `from_bytes`, or `from_reader`
// depending on what Rust types the deserializer is able to consume as input.
//
// This basic deserializer supports only `from_str`.
pub fn from_bytes<'a, T>(s: &'a [u8]) -> Result<T>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_bytes(s);
    let t = T::deserialize(&mut deserializer)?;
    Ok(t)
    //if deserializer.input.is_empty() {
    //    Ok(t)
    //} else {
    //    Err(Error::TrailingCharacters)
    //}
}

// SERDE IS NOT A PARSING LIBRARY. This impl block defines a few basic parsing
// functions from scratch. More complicated formats may wish to use a dedicated
// parsing library to help implement their Serde deserializer.
impl<'de> Deserializer<'de> {
    // Look at the first character in the input without consuming it.
    fn peek_byte(&mut self) -> Result<&u8> {
        self.input.first().ok_or(Error::Eof)
    }

    // Consume the first character in the input.
    fn next_byte(&mut self) -> Result<u8> {
        match self.input.split_first() {
            Some((consumed, left)) => {
                self.input = left;
                Ok(*consumed)
            },
            None => Err(Error::Eof)
        }
    }

    fn next_bytes(&mut self, n: usize) -> Result<&[u8]> {
        if self.input.len() < n {
            Err(Error::Eof)
        } else {
            let (consumed, left) = &self.input.split_at(n);
            self.input = left;
            Ok(consumed)
        }
    }

    // Parse 1 as true, 0 as false, else error
    fn parse_bool(&mut self) -> Result<bool> {
        self.next_byte()
            .and_then(|b| match b {
                1 => Ok(true),
                0 => Ok(false),
                _ => Err(Error::ExpectedBoolean)
            })
    }

    // TODO: Abstract to macro
    fn parse_u8(&mut self) -> Result<u8> {
        self.next_byte()
            .map(|u8| u8::from_le_bytes([u8]))
    }

    fn parse_u16(&mut self) -> Result<u16> {
        let bytes = self.next_bytes(core::mem::size_of::<u16>())?;
        let mut le_bytes = [0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(u16::from_le_bytes(le_bytes))
    }

    fn parse_u32(&mut self) -> Result<u32> {
        let bytes = self.next_bytes(core::mem::size_of::<u32>())?;
        let mut le_bytes = [0,0,0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(u32::from_le_bytes(le_bytes))
    }

    fn parse_u64(&mut self) -> Result<u64> {
        let bytes = self.next_bytes(core::mem::size_of::<u64>())?;
        let mut le_bytes = [0,0,0,0,0,0,0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(le_bytes))
    }

    fn parse_u128(&mut self) -> Result<u128> {
        let bytes = self.next_bytes(core::mem::size_of::<u128>())?;
        let mut le_bytes = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(u128::from_le_bytes(le_bytes))
    }

    fn parse_i8(&mut self) -> Result<i8> {
        self.next_byte()
            .map(|u8| i8::from_le_bytes([u8]))
    }

    fn parse_i16(&mut self) -> Result<i16> {
        let bytes = self.next_bytes(core::mem::size_of::<i16>())?;
        let mut le_bytes = [0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(i16::from_le_bytes(le_bytes))
    }

    fn parse_i32(&mut self) -> Result<i32> {
        let bytes = self.next_bytes(core::mem::size_of::<i32>())?;
        let mut le_bytes = [0,0,0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(i32::from_le_bytes(le_bytes))
    }

    fn parse_i64(&mut self) -> Result<i64> {
        let bytes = self.next_bytes(core::mem::size_of::<i32>())?;
        let mut le_bytes = [0,0,0,0,0,0,0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(i64::from_le_bytes(le_bytes))
    }

    fn parse_i128(&mut self) -> Result<i128> {
        let bytes = self.next_bytes(core::mem::size_of::<i32>())?;
        let mut le_bytes = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0];
        le_bytes.copy_from_slice(bytes);
        Ok(i128::from_le_bytes(le_bytes))
    }

    fn parse_varuint(&mut self) -> Result<u32> {
        let mut value: u32 = 0;
        let mut bit: u32 = 0;
        loop {
            let x = self.parse_u8()?;
            value = value | ((x & 0x7f) as u32) << bit;
            bit += 7;
            if (x & 0x80) == 0 {
                break;
            }
        }
        println!("parsed {}", value);
        Ok(value)
    }

    // Parse a possible minus sign followed by a group of decimal digits as a
    // signed integer of type T.
    fn parse_signed<T>(&mut self) -> Result<T>
    where
        T: Neg<Output = T> + AddAssign<T> + MulAssign<T> + From<i8>,
    {
        // Optional minus sign, delegate to `parse_unsigned`, negate if negative.
        unimplemented!()
    }

    // Parse a string until the next '"' character.
    //
    // Makes no attempt to handle escape sequences. What did you expect? This is
    // example code!
    fn parse_string(&mut self) -> Result<&'de str> {
        unimplemented!()
        //if self.next_char()? != '"' {
        //    return Err(Error::ExpectedString);
        //}
        //match self.input.find('"') {
        //    Some(len) => {
        //        let s = &self.input[..len];
        //        self.input = &self.input[len + 1..];
        //        Ok(s)
        //    }
        //    None => Err(Error::Eof),
        //}
    }
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = Error;

    // Look at the input data to decide what Serde data model type to
    // deserialize as. Not all data formats are able to support this operation.
    // Formats that support `deserialize_any` are known as self-describing.
    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!();
    }

    // Uses the `parse_bool` parsing function defined above to read the JSON
    // identifier `true` or `false` from the input.
    //
    // Parsing refers to looking at the input and deciding that it contains the
    // JSON value `true` or `false`.
    //
    // Deserialization refers to mapping that JSON value into Serde's data
    // model by invoking one of the `Visitor` methods. In the case of JSON and
    // bool that mapping is straightforward so the distinction may seem silly,
    // but in other cases Deserializers sometimes perform non-obvious mappings.
    // For example the TOML format has a Datetime type and Serde's data model
    // does not. In the `toml` crate, a Datetime in the input is deserialized by
    // mapping it to a Serde data model "struct" type with a special name and a
    // single field containing the Datetime represented as a string.
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_bool(self.parse_bool()?)
    }

    // The `parse_signed` function is generic over the integer type `T` so here
    // it is invoked with `T=i8`. The next 8 methods are similar.
    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i8(self.parse_i8()?)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i16(self.parse_i16()?)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.parse_i32()?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(self.parse_i64()?)
    }

    fn deserialize_i128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i128(self.parse_i128()?)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u8(self.parse_u8()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u16(self.parse_u16()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.parse_u32()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(self.parse_u64()?)
    }
    fn deserialize_u128<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u128(self.parse_u128()?)
    }

    // Float parsing is stupidly hard.
    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // Float parsing is stupidly hard.
    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        // This is a hack, OK for now
        visitor.visit_u32(self.parse_varuint()?)
    }

    // Refer to the "Understanding deserializer lifetimes" page for information
    // about the three deserialization flavors of strings in Serde.
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_borrowed_str(self.parse_string()?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    // The `Serializer` implementation on the previous page serialized byte
    // arrays as JSON arrays of bytes. Handle that representation here.
    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
    }

    // An absent optional is represented as the JSON `null` and a present
    // optional is represented as just the contained value.
    //
    // As commented in `Serializer` implementation, this is a lossy
    // representation. For example the values `Some(())` and `None` both
    // serialize as just `null`. Unfortunately this is typically what people
    // expect when working with JSON. Other formats are encouraged to behave
    // more intelligently if possible.
    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
        //if self.input.starts_with("null") {
        //    self.input = &self.input["null".len()..];
        //    visitor.visit_none()
        //} else {
        //    visitor.visit_some(self)
        //}
    }

    // In Serde, unit means an anonymous value containing no data.
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
        //if self.input.starts_with("null") {
        //    self.input = &self.input["null".len()..];
        //    visitor.visit_unit()
        //} else {
        //    Err(Error::ExpectedNull)
        //}
    }

    // Unit struct means a named value containing no data.
    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    // As is done here, serializers are encouraged to treat newtype structs as
    // insignificant wrappers around the data they contain. That means not
    // parsing anything other than the contained value.
    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    // Deserialization of compound types like sequences and maps happens by
    // passing the visitor an "Access" object that gives it the ability to
    // iterate through the data contained in the sequence.
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let array_length = self.parse_varuint()?;
        visitor.visit_seq(CommaSeparated::new(self, array_length))
        //// Parse the opening bracket of the sequence.
        //if self.next_char()? == '[' {
        //    // Give the visitor access to each element of the sequence.
        //    let value = visitor.visit_seq(CommaSeparated::new(self))?;
        //    // Parse the closing bracket of the sequence.
        //    if self.next_char()? == ']' {
        //        Ok(value)
        //    } else {
        //        Err(Error::ExpectedArrayEnd)
        //    }
        //} else {
        //    Err(Error::ExpectedArray)
        //}
    }

    // Tuples look just like sequences in JSON. Some formats may be able to
    // represent tuples more efficiently.
    //
    // As indicated by the length parameter, the `Deserialize` implementation
    // for a tuple in the Serde data model is required to know the length of the
    // tuple before even looking at the input data.
    fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    // Tuple structs look just like sequences in JSON.
    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_seq(visitor)
    }

    // Much like `deserialize_seq` but calls the visitors `visit_map` method
    // with a `MapAccess` implementation, rather than the visitor's `visit_seq`
    // method with a `SeqAccess` implementation.
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
        //let value = visitor.visit_seq(CommaSeparated::new(self))?;
        //Ok(value)
    }

    // Structs look just like maps in JSON.
    //
    // Notice the `fields` parameter - a "struct" in the Serde data model means
    // that the `Deserialize` implementation is required to know what the fields
    // are before even looking at the input data. Any key-value pairing in which
    // the fields cannot be known ahead of time is probably a map.
    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        println!("{}: {:?}", _name, _fields);
        visitor.visit_seq(CommaSeparated::new(self, _fields.len() as u32))
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        unimplemented!()
        //if self.peek_char()? == '"' {
        //    // Visit a unit variant.
        //    visitor.visit_enum(self.parse_string()?.into_deserializer())
        //} else if self.next_char()? == '{' {
        //    // Visit a newtype variant, tuple variant, or struct variant.
        //    let value = visitor.visit_enum(Enum::new(self))?;
        //    // Parse the matching close brace.
        //    if self.next_char()? == '}' {
        //        Ok(value)
        //    } else {
        //        Err(Error::ExpectedMapEnd)
        //    }
        //} else {
        //    Err(Error::ExpectedEnum)
        //}
    }

    // An identifier in Serde is the type that identifies a field of a struct or
    // the variant of an enum. In JSON, struct fields and enum variants are
    // represented as strings. In other formats they may be represented as
    // numeric indices.
    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    // Like `deserialize_any` but indicates to the `Deserializer` that it makes
    // no difference which `Visitor` method is called because the data is
    // ignored.
    //
    // Some deserializers are able to implement this more efficiently than
    // `deserialize_any`, for example by rapidly skipping over matched
    // delimiters without paying close attention to the data in between.
    //
    // Some formats are not able to implement this at all. Formats that can
    // implement `deserialize_any` and `deserialize_ignored_any` are known as
    // self-describing.
    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}

// In order to handle commas correctly when deserializing a JSON array or map,
// we need to track whether we are on the first element or past the first
// element.
struct CommaSeparated<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
    len: u32
}

impl<'a, 'de> CommaSeparated<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>, len: u32) -> Self {
        CommaSeparated {
            de,
            len
        }
    }
}

// `SeqAccess` is provided to the `Visitor` to give it the ability to iterate
// through elements of the sequence.
impl<'de, 'a> SeqAccess<'de> for CommaSeparated<'a, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        //// Check if there are no more elements.
        if self.len > 0 {
            self.len -= 1;
            seed.deserialize(&mut *self.de).map(Some)
        } else {
            Result::Ok(None)
        }
    }
}

// `MapAccess` is provided to the `Visitor` to give it the ability to iterate
// through entries of the map.
impl<'de, 'a> MapAccess<'de> for CommaSeparated<'a, 'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
    where
        K: DeserializeSeed<'de>,
    {
        unimplemented!()
        //// Check if there are no more entries.
        //if self.de.peek_char()? == '}' {
        //    return Ok(None);
        //}
        //// Comma is required before every entry except the first.
        //if !self.first && self.de.next_char()? != ',' {
        //    return Err(Error::ExpectedMapComma);
        //}
        //self.first = false;
        //// Deserialize a map key.
        //seed.deserialize(&mut *self.de).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
    where
        V: DeserializeSeed<'de>,
    {
        unimplemented!()
        //// It doesn't make a difference whether the colon is parsed at the end
        //// of `next_key_seed` or at the beginning of `next_value_seed`. In this
        //// case the code is a bit simpler having it here.
        //if self.de.next_char()? != ':' {
        //    return Err(Error::ExpectedMapColon);
        //}
        //// Deserialize a map value.
        //seed.deserialize(&mut *self.de)
    }
}

struct Enum<'a, 'de: 'a> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> Enum<'a, 'de> {
    fn new(de: &'a mut Deserializer<'de>) -> Self {
        Enum { de }
    }
}

// `EnumAccess` is provided to the `Visitor` to give it the ability to determine
// which variant of the enum is supposed to be deserialized.
//
// Note that all enum deserialization methods in Serde refer exclusively to the
// "externally tagged" enum representation.
impl<'de, 'a> EnumAccess<'de> for Enum<'a, 'de> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant)>
    where
        V: DeserializeSeed<'de>,
    {
        unimplemented!()
        //// The `deserialize_enum` method parsed a `{` character so we are
        //// currently inside of a map. The seed will be deserializing itself from
        //// the key of the map.
        //let val = seed.deserialize(&mut *self.de)?;
        //// Parse the colon separating map key from value.
        //if self.de.next_char()? == ':' {
        //    Ok((val, self))
        //} else {
        //    Err(Error::ExpectedMapColon)
        //}
    }
}

// `VariantAccess` is provided to the `Visitor` to give it the ability to see
// the content of the single variant that it decided to deserialize.
impl<'de, 'a> VariantAccess<'de> for Enum<'a, 'de> {
    type Error = Error;

    // If the `Visitor` expected this variant to be a unit variant, the input
    // should have been the plain string case handled in `deserialize_enum`.
    fn unit_variant(self) -> Result<()> {
        Err(Error::ExpectedString)
    }

    // Newtype variants are represented in JSON as `{ NAME: VALUE }` so
    // deserialize the value here.
    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value>
    where
        T: DeserializeSeed<'de>,
    {
        seed.deserialize(self.de)
    }

    // Tuple variants are represented in JSON as `{ NAME: [DATA...] }` so
    // deserialize the sequence of data here.
    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_seq(self.de, visitor)
    }

    // Struct variants are represented in JSON as `{ NAME: { K: V, ... } }` so
    // deserialize the inner map here.
    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        de::Deserializer::deserialize_map(self.de, visitor)
    }
}


struct VaruintVisitor;

impl<'de> Visitor<'de> for VaruintVisitor {
    type Value = Varuint;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer between -2^31 and 2^31")
    }

    fn visit_u32<E>(self, value: u32) -> core::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        core::result::Result::Ok(Varuint::new(value))
    }
}

#[derive(Debug)]
pub struct Varuint {
    value: u32
}

impl Varuint {
    fn new(value: u32) -> Self {
        Varuint { value }
    }
}

impl From<Varuint> for u32 {
    fn from(value: Varuint) -> Self {
        value.value
    }
}

impl<'de> Deserialize<'de> for Varuint {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Varuint, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_char(VaruintVisitor)
    }
}

#[derive(Debug)]
pub struct Name {
    value: String
}

impl Name {
    fn new(value: String) -> Self {
        Name { value }
    }
}

pub fn name_to_string(n: u64) -> String {
    let bytes = n.to_le_bytes();
    let mut result = String::new();
    let mut bit: i64 = 63;
    while bit >= 0 {
        let mut c: u32 = 0;
        for _ in 0..5 {
            if bit >= 0 {
                c = (c << 1u32) | (((bytes[bit as usize / 8]) as u32 >> (bit as u32 % 8)) & 1);
                bit -= 1;
            }
        }
        if c >= 6 {
            result.push(char::from_u32(c as u32 + 97 - 6).unwrap())
        } else if c >= 1 {
            result.push(char::from_u32(c as u32 + 49 - 1).unwrap())
        } else {
            result.push_str(".");
        }
    }
    result.trim_end_matches(".").to_string()
}

#[cfg(test)]
mod tests {
    use crate::de::name_to_string;

    fn parse_u64(x: String) -> u64 {
        let packed_bytes = hex::decode(x).unwrap();
        let mut le_bytes = [0,0,0,0,0,0,0,0];
        le_bytes.copy_from_slice(&packed_bytes[..]);
        u64::from_le_bytes(le_bytes)
    }
    #[test]
    fn it_works() {
        println!("hello hello{:?}", name_to_string(parse_u64("00a6823403ea3055".into())));
    }
}

impl<'de> Deserialize<'de> for Name {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Name, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct NameVisitor;

        impl<'de> Visitor<'de> for NameVisitor {
            type Value = Name;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an integer between -2^31 and 2^31")
            }

            fn visit_u64<E>(self, value: u64) -> core::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                core::result::Result::Ok(Name::new(name_to_string(value)))
            }
        }
        deserializer.deserialize_u64(NameVisitor)
    }
}

#[derive(Deserialize, Debug)]
pub struct Transaction {
    pub expiration: u32,
    pub ref_block_num: u16,
    pub ref_block_prefix: u32,
    pub max_net_usage_words: Varuint,
    pub max_cpu_usage_ms: u8,
    pub delay_sec: Varuint,
    pub ctx_free_actions: Vec<Action>,
    pub actions: Vec<Action>,
}

#[derive(Deserialize, Debug)]
pub struct Action {
    pub account: Name,
    pub name: Name,
    pub authorization: Vec<Authorization>
}

#[derive(Deserialize, Debug)]
pub struct Authorization {
    pub actor: Name,
    pub permission: Name
}