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

//! `Deserialize` for the AST: nodes borrow from the serialized text, strings
//! that cannot be borrowed go in a `StringArena`, and every corpus statement
//! survives the round trip.

#![cfg(feature = "serde")]

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use std::fmt;

use corpus::{Kind, V2_DIALECTS, parse_corpus};
use serde::{Deserialize, Serialize};
use sqlparser::ast::{Ident, Span, Statement, StatementKind, StringArena, deserialize_with_arena};
use sqlparser::dialect::builtin::parse_sql_as;
use sqlparser::dialect::{AstExt, Builtin, Dialect, DialectKind, NoExt, Postgres};
use sqlparser::grammar;
use sqlparser::parser::Parser;
use sqlparser::{PResult, parse_sql};

type Statements<'a> = Vec<Statement<'a, Builtin>>;

/// Reads `json` with no limit on its depth, so that it can hold any tree the
/// parser accepts.
fn from_json<'j>(json: &'j str, arena: &'j StringArena) -> serde_json::Result<Statements<'j>> {
    let mut deserializer = serde_json::Deserializer::from_str(json);
    deserializer.disable_recursion_limit();
    let statements = deserialize_with_arena(arena, &mut deserializer)?;
    deserializer.end()?;
    Ok(statements)
}

#[test]
fn a_select_round_trips() {
    let statements = parse_sql::<Postgres>("SELECT a + 1 FROM t WHERE b > 2").unwrap();
    let json = serde_json::to_string(&statements).unwrap();
    let back: Statements = serde_json::from_str(&json).unwrap();
    assert_eq!(back, statements);
}

#[test]
fn identifiers_borrow_from_the_serialized_text() {
    let ident = Ident {
        text: "abc",
        quote: None,
        span: Span::default(),
    };
    let json = serde_json::to_string(&ident).unwrap();
    let back: Ident = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ident);
    assert!(json.as_bytes().as_ptr_range().contains(&back.text.as_ptr()));
}

#[test]
fn borrowing_alone_rejects_a_string_with_an_escape() {
    let statements = parse_sql::<Postgres>(r#"SELECT 'a"b'"#).unwrap();
    let json = serde_json::to_string(&statements).unwrap();
    assert!(serde_json::from_str::<Statements>(&json).is_err());
}

#[test]
fn the_arena_holds_strings_with_escapes() {
    let sql = "SELECT 'a\"b\nc\\d' FROM \"q\"\"t\"";
    let statements = parse_sql::<Postgres>(sql).unwrap();
    let json = serde_json::to_string(&statements).unwrap();
    let arena = StringArena::new();
    assert_eq!(from_json(&json, &arena).unwrap(), statements);
}

#[test]
fn the_arena_reads_from_a_reader() {
    let statements = parse_sql::<Postgres>("SELECT a FROM t").unwrap();
    let json = serde_json::to_vec(&statements).unwrap();
    let arena = StringArena::new();
    let mut deserializer = serde_json::Deserializer::from_reader(json.as_slice());
    let back: Statements = deserialize_with_arena(&arena, &mut deserializer).unwrap();
    assert_eq!(back, statements);
}

#[test]
fn keyword_labels_and_named_operators_round_trip() {
    let sql = "SELECT a ~ b, c ILIKE d FROM t CROSS JOIN u";
    let statements = parse_sql::<Postgres>(sql).unwrap();
    let json = serde_json::to_string(&statements).unwrap();
    let back: Statements = serde_json::from_str(&json).unwrap();
    assert_eq!(back[0].to_string(), sql);
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "visitor",
    derive(sqlparser::ast::Visit, sqlparser::ast::VisitMut)
)]
struct Ping<'a> {
    #[serde(borrow)]
    host: Ident<'a>,
}

impl fmt::Display for Ping<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PING {}", self.host)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Acme;

impl AstExt for Acme {
    type Expr<'a> = NoExt;
    type Select<'a> = NoExt;
    type Statement<'a> = Ping<'a>;
    type Insert<'a> = NoExt;
    type TableFactor<'a> = NoExt;
    type JoinConstraint<'a> = NoExt;
    type OrderByItem<'a> = NoExt;
    type QueryTail<'a> = NoExt;
}

impl Dialect for Acme {
    type Ext = Self;

    fn parse_statement<'a>(p: &mut Parser<'a, Self>) -> PResult<Statement<'a, Self>> {
        if p.eat_keyword("PING") {
            let host = p.parse_ident()?;
            return Ok(StatementKind::Ext(Ping { host }).into());
        }
        grammar::parse_statement(p)
    }
}

#[test]
fn a_third_party_extension_round_trips() {
    let statements = parse_sql::<Acme>("PING db1; SELECT 1").unwrap();
    let json = serde_json::to_string(&statements).unwrap();
    let back: Vec<Statement<'_, Acme>> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, statements);
}

#[test]
fn every_corpus_statement_round_trips() {
    let cases = parse_corpus(include_str!("corpus/v1.corpus"));
    let mut checked = 0usize;
    for dialect in V2_DIALECTS {
        let kind = if dialect == "ansi" {
            DialectKind::Core
        } else {
            DialectKind::from_name(dialect).unwrap_or_else(|| panic!("unknown {dialect}"))
        };
        for case in cases.iter().filter(|case| case.kind == Kind::Statement) {
            let Ok(statements) = parse_sql_as(kind, &case.sql) else {
                continue;
            };
            let json = serde_json::to_string(&statements).unwrap();
            let arena = StringArena::new();
            let back = from_json(&json, &arena)
                .unwrap_or_else(|e| panic!("[{dialect}] {}: {e}", case.sql));
            assert_eq!(back, statements, "[{dialect}] {}", case.sql);
            // Unlike `==`, this also compares spans.
            assert_eq!(
                serde_json::to_string(&back).unwrap(),
                json,
                "[{dialect}] {}",
                case.sql
            );
            checked += 1;
        }
    }
    assert!(checked > 10_000, "only {checked} checked");
}
