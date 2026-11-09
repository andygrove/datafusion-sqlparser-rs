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

//! Derive macros for sqlparser's AST visitors.
//!
//! `#[derive(Visit, VisitMut)]` generates traversal for an AST type. The
//! `#[visit(...)]` attribute adjusts what is generated:
//!
//! - `with = "visit_foo"` on a type, or on a field of a type, calls the
//!   visitor's `pre_visit_foo` and `post_visit_foo` hooks around that value.
//!   On a field holding `Option`, `Vec`, `Box` or a tuple, the hooks apply to
//!   each value inside it. Together with `when = "path"` on a struct field, the
//!   hooks fire only if the predicate, called with the whole struct, returns
//!   true; otherwise the field is visited plainly.
//! - `whole = "visit_foo"` on a field calls the hooks once around the field's
//!   entire value.
//! - `nonempty = "visit_foo"` on a `Vec` field calls the hooks once around the
//!   whole list, and not at all for an empty list.
//! - `skip_if = "path"` on a struct field leaves the field unvisited if the
//!   predicate, called with the whole struct, returns true.
//! - `ext = "path::Ext"` on a type with no type parameter, for one that holds
//!   nodes of a particular AST such as `Expr<'a, Builtin>`: generates impls
//!   for that AST only, rather than for every `X: AstExt`.

use proc_macro::TokenStream;

mod visit;

/// Generates [`Visit`] for an AST type. See the crate documentation.
#[proc_macro_derive(Visit, attributes(visit))]
pub fn derive_visit(input: TokenStream) -> TokenStream {
    visit::derive(input, visit::Mode::Shared)
}

/// Generates [`VisitMut`] for an AST type. See the crate documentation.
#[proc_macro_derive(VisitMut, attributes(visit))]
pub fn derive_visit_mut(input: TokenStream) -> TokenStream {
    visit::derive(input, visit::Mode::Mutable)
}
