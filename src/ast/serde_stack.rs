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

//! `Serialize` and `Deserialize` for the recursive AST types, which grow the
//! stack when needed.
//!
//! Both are deep: a level of the tree is many frames of the serializer's
//! own code, which a debug build makes large, so the deepest trees the parser
//! accepts need more than the stack a thread usually has. These impls continue
//! on a new stack segment when the current one is nearly full, and so do not
//! depend on the serializer's frame sizes.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ast::{
    DataType, ExprKind, MatchPattern, Query, SetExpr, SpecialExpr, StatementKind, TableFactor,
};
use crate::dialect::{AstExt, DeserializeExt};

const RED_ZONE: usize = 128 * 1024;
const STACK_SEGMENT: usize = 2 * 1024 * 1024;

macro_rules! growing_serialize {
    ($(impl[$($generics:tt)*] $ty:ty),* $(,)?) => {$(
        impl<$($generics)*> Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                stacker::maybe_grow(RED_ZONE, STACK_SEGMENT, || {
                    <$ty>::serialize(self, serializer)
                })
            }
        }
    )*};
}

growing_serialize!(
    impl['a, X: AstExt] ExprKind<'a, X>,
    impl['a, X: AstExt] SpecialExpr<'a, X>,
    impl['a, X: AstExt] SetExpr<'a, X>,
    impl['a, X: AstExt] Query<'a, X>,
    impl['a, X: AstExt] TableFactor<'a, X>,
    impl['a, X: AstExt] StatementKind<'a, X>,
    impl['a] DataType<'a>,
    impl['a] MatchPattern<'a>,
);

macro_rules! growing_deserialize {
    ($(impl[$($generics:tt)*] $ty:ty $(where $bound:ty)?),* $(,)?) => {$(
        impl<'de, 'a $(, $generics)*> Deserialize<'de> for $ty
        $(where $bound: DeserializeExt<'de, 'a>)?
        {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                stacker::maybe_grow(RED_ZONE, STACK_SEGMENT, || {
                    <$ty>::deserialize(deserializer)
                })
            }
        }
    )*};
}

growing_deserialize!(
    impl[X] ExprKind<'a, X> where X,
    impl[X] SpecialExpr<'a, X> where X,
    impl[X] SetExpr<'a, X> where X,
    impl[X] Query<'a, X> where X,
    impl[X] TableFactor<'a, X> where X,
    impl[X] StatementKind<'a, X> where X,
    impl[] DataType<'a>,
    impl[] MatchPattern<'a>,
);
