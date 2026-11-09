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

//! `Serialize` for the AST: a few nodes' shapes, and that every corpus
//! statement that parses serializes without error.

#![cfg(feature = "serde")]

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{Kind, V2_DIALECTS, parse_corpus};
use serde_json::json;
use sqlparser::dialect::builtin::parse_sql_as;
use sqlparser::dialect::{DialectKind, MySql, Postgres, Snowflake, Spark};
use sqlparser::parse_sql;

#[test]
fn a_select_has_the_expected_shape() {
    let statements = parse_sql::<Postgres>("SELECT a FROM t").unwrap();
    let value = serde_json::to_value(&statements).unwrap();
    assert_eq!(
        value[0]["kind"]["Query"]["body"]["Select"]["projection"][0]["kind"]["Expr"]["expr"]["kind"]
            ["Ident"]["text"],
        json!("a")
    );
    assert_eq!(
        value[0]["kind"]["Query"]["body"]["Select"]["from"][0]["relation"]["factor"]["Named"]["parts"]
            [0]["text"],
        json!("t")
    );
}

#[test]
fn a_dialect_extension_serializes_through_builtin() {
    let statements = parse_sql::<Snowflake>("CREATE STAGE s URL='x'").unwrap();
    let value = serde_json::to_value(&statements).unwrap();
    assert!(value[0]["kind"]["Ext"]["Snowflake"].is_object(), "{value}");
}

#[test]
fn an_expression_extension_keeps_its_dialect_name() {
    let statements = parse_sql::<MySql>("SELECT CONVERT(1, CHAR)").unwrap();
    let value = serde_json::to_value(&statements).unwrap();
    assert!(
        value[0]["kind"]["Query"]["body"]["Select"]["projection"][0]["kind"]["Expr"]["expr"]["kind"]["Ext"]["MySql"]
            .is_object(),
        "{value}"
    );
}

#[test]
fn identifiers_keep_their_spans_and_quotes() {
    let statements = parse_sql::<Spark>("SELECT `a` FROM t").unwrap();
    let value = serde_json::to_value(&statements).unwrap();
    let ident = &value[0]["kind"]["Query"]["body"]["Select"]["projection"][0]["kind"]["Expr"]["expr"]
        ["kind"]["Ident"];
    assert_eq!(ident["text"], json!("a"));
    assert_eq!(ident["quote"], json!("`"));
    assert_eq!(ident["span"]["start"], json!(7));
}

#[test]
fn every_corpus_statement_that_parses_serializes() {
    let cases = parse_corpus(include_str!("corpus/v1.corpus"));
    let mut serialized = 0usize;
    for dialect in V2_DIALECTS {
        let kind = if dialect == "ansi" {
            DialectKind::Core
        } else {
            DialectKind::from_name(dialect).unwrap_or_else(|| panic!("unknown {dialect}"))
        };
        for case in &cases {
            if case.kind != Kind::Statement {
                continue;
            }
            let Ok(statements) = parse_sql_as(kind, &case.sql) else {
                continue;
            };
            serde_json::to_string(&statements)
                .unwrap_or_else(|e| panic!("[{dialect}] {}: {e}", case.sql));
            serialized += 1;
        }
    }
    assert!(serialized > 10_000, "only {serialized} serialized");
}
