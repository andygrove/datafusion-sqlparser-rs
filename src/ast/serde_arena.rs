// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! Storage for strings that cannot be borrowed while deserializing.
//!
//! AST nodes hold `&str`, so `Deserialize` borrows every string from the
//! serialized text. A format cannot lend a string it had to decode, such as a
//! JSON string containing `\"` or `\n`, or anything read from an `io::Read`.
//! [`deserialize_with_arena`] copies just those strings into a [`StringArena`].
//!
//! The arena is applied by wrapping the deserializer rather than through a
//! thread-local, so that the borrow checker can see the arena must outlive the
//! tree: the arena is borrowed for `'de`, the same lifetime as the input.

use std::fmt;

use serde::Deserialize;
use serde::de::{
    self, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor,
};

/// Holds strings that a deserialized AST borrows but the input could not lend.
///
/// See [`deserialize_with_arena`]. It frees them all when dropped.
#[derive(Default)]
pub struct StringArena(typed_arena::Arena<u8>);

impl StringArena {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc(&self, text: &str) -> &str {
        self.0.alloc_str(text)
    }
}

impl fmt::Debug for StringArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringArena").finish_non_exhaustive()
    }
}

/// Deserializes `T`, keeping any string the input cannot lend in `arena`.
///
/// `serde_json::from_str` alone rejects a string containing an escape, because
/// it cannot lend the decoded text. Use this instead unless the JSON is known
/// to contain no escapes.
///
/// A tree that did not come from the parser can be deeper than the parser would
/// allow, and dropping one that is too deep can overflow the stack. A format's
/// own depth limit, such as `serde_json`'s default of 128, prevents that, so
/// keep it on for untrusted input.
///
/// ```
/// use sqlparser::ast::{Statement, StringArena, deserialize_with_arena};
/// use sqlparser::dialect::{Builtin, Postgres};
/// use sqlparser::parse_sql;
///
/// let statements = parse_sql::<Postgres>(r#"SELECT 'say "hi"'"#).unwrap();
/// let json = serde_json::to_string(&statements).unwrap();
///
/// let arena = StringArena::new();
/// let mut deserializer = serde_json::Deserializer::from_str(&json);
/// let back: Vec<Statement<'_, Builtin>> =
///     deserialize_with_arena(&arena, &mut deserializer).unwrap();
/// assert_eq!(back, statements);
/// ```
pub fn deserialize_with_arena<'de, D, T>(
    arena: &'de StringArena,
    deserializer: D,
) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(Arena {
        arena,
        inner: deserializer,
    })
}

/// A deserializer, visitor, access or seed that passes the arena inward.
struct Arena<'de, I> {
    arena: &'de StringArena,
    inner: I,
}

impl<'de, I> Arena<'de, I> {
    fn wrap<J>(&self, inner: J) -> Arena<'de, J> {
        Arena {
            arena: self.arena,
            inner,
        }
    }
}

/// Methods that do not produce a `&str` or contain anything that might.
macro_rules! forward {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
            self.inner.$method(visitor)
        }
    )*};
}

/// Methods whose visitor may be given a string or a container.
macro_rules! wrap {
    ($($method:ident)*) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
            let visitor = self.wrap(visitor);
            self.inner.$method(visitor)
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Arena<'de, D> {
    type Error = D::Error;

    forward!(
        deserialize_bool deserialize_i8 deserialize_i16 deserialize_i32 deserialize_i64
        deserialize_i128 deserialize_u8 deserialize_u16 deserialize_u32 deserialize_u64
        deserialize_u128 deserialize_f32 deserialize_f64 deserialize_char deserialize_string
        deserialize_bytes deserialize_byte_buf deserialize_unit deserialize_identifier
        deserialize_ignored_any
    );

    wrap!(deserialize_any deserialize_str deserialize_option deserialize_seq deserialize_map);

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        self.inner.deserialize_unit_struct(name, visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.wrap(visitor);
        self.inner.deserialize_newtype_struct(name, visitor)
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.wrap(visitor);
        self.inner.deserialize_tuple(len, visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.wrap(visitor);
        self.inner.deserialize_tuple_struct(name, len, visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.wrap(visitor);
        self.inner.deserialize_struct(name, fields, visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.wrap(visitor);
        self.inner.deserialize_enum(name, variants, visitor)
    }

    fn is_human_readable(&self) -> bool {
        self.inner.is_human_readable()
    }
}

macro_rules! forward_visit {
    ($($method:ident $ty:ty,)*) => {$(
        fn $method<E: de::Error>(self, value: $ty) -> Result<Self::Value, E> {
            self.inner.$method(value)
        }
    )*};
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Arena<'de, V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.expecting(f)
    }

    forward_visit!(
        visit_bool bool, visit_i8 i8, visit_i16 i16, visit_i32 i32, visit_i64 i64,
        visit_i128 i128, visit_u8 u8, visit_u16 u16, visit_u32 u32, visit_u64 u64,
        visit_u128 u128, visit_f32 f32, visit_f64 f64, visit_char char,
        visit_borrowed_str &'de str, visit_bytes &[u8], visit_borrowed_bytes &'de [u8],
        visit_byte_buf Vec<u8>,
    );

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.inner.visit_borrowed_str(self.arena.alloc(value))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        self.visit_str(&value)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_none()
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.inner.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let deserializer = self.wrap(deserializer);
        self.inner.visit_some(deserializer)
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        let deserializer = self.wrap(deserializer);
        self.inner.visit_newtype_struct(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        let access = self.wrap(access);
        self.inner.visit_seq(access)
    }

    fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        let access = self.wrap(access);
        self.inner.visit_map(access)
    }

    fn visit_enum<A: EnumAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        let access = self.wrap(access);
        self.inner.visit_enum(access)
    }
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Arena<'de, S> {
    type Value = S::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<S::Value, D::Error> {
        let deserializer = self.wrap(deserializer);
        self.inner.deserialize(deserializer)
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Arena<'de, A> {
    type Error = A::Error;

    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        let seed = self.wrap(seed);
        self.inner.next_element_seed(seed)
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Arena<'de, A> {
    type Error = A::Error;

    fn next_key_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, A::Error> {
        let seed = self.wrap(seed);
        self.inner.next_key_seed(seed)
    }

    fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, A::Error> {
        let seed = self.wrap(seed);
        self.inner.next_value_seed(seed)
    }

    fn size_hint(&self) -> Option<usize> {
        self.inner.size_hint()
    }
}

impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for Arena<'de, A> {
    type Error = A::Error;
    type Variant = Arena<'de, A::Variant>;

    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Self::Variant), A::Error> {
        let arena = self.arena;
        let (value, variant) = self.inner.variant_seed(Arena { arena, inner: seed })?;
        Ok((
            value,
            Arena {
                arena,
                inner: variant,
            },
        ))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Arena<'de, A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), A::Error> {
        self.inner.unit_variant()
    }

    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, A::Error> {
        let seed = self.wrap(seed);
        self.inner.newtype_variant_seed(seed)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        let visitor = self.wrap(visitor);
        self.inner.tuple_variant(len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        let visitor = self.wrap(visitor);
        self.inner.struct_variant(fields, visitor)
    }
}
