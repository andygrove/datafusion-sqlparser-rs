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

//! Choosing a built-in dialect at runtime.

mod common;

use common::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::*;
use sqlparser::dialect::DialectKind::{self, *};
use sqlparser::dialect::builtin::{Builtin, parse_query_as};

#[test]
fn there_is_no_ansi_dialect() {
    assert_eq!(DialectKind::from_name("ansi"), None);
}

#[test]
fn dialects_are_found_by_name() {
    let cases = [
        ("core", Core),
        ("mysql", MySql),
        ("postgres", Postgres),
        ("postgresql", Postgres),
        ("redshift", Redshift),
        ("snowflake", Snowflake),
        ("MySQL", MySql),
    ];
    for (name, kind) in cases {
        assert_eq!(DialectKind::from_name(name), Some(kind), "{name}");
    }
}

#[test]
fn unknown_dialect_name_is_not_found() {
    assert_eq!(DialectKind::from_name("oracle-ish"), None);
}

#[test]
fn chosen_dialect_controls_accepted_syntax() {
    let cases = [
        (MySql, "SELECT `a` FROM t", true),
        (Postgres, "SELECT `a` FROM t", false),
        (Core, "SELECT `a` FROM t", false),
        (Postgres, "SELECT x::int FROM t", true),
        (Redshift, "SELECT x::int FROM t", true),
        (MySql, "SELECT x::int FROM t", false),
        (Redshift, "SELECT TOP 10 a FROM t", true),
        (Postgres, "SELECT TOP 10 a FROM t", false),
        (MySql, "SELECT a FROM t LIMIT 5, 10", true),
        (Redshift, "SELECT a FROM t LIMIT 5, 10", false),
    ];
    for (kind, sql, accepted) in cases {
        assert_eq!(
            parse_query_as(kind, sql).is_ok(),
            accepted,
            "{kind:?}: {sql}"
        );
    }
}

#[test]
fn mysql_chosen_at_runtime_applies_its_limit_rule() {
    let select = parse_query_as(MySql, "SELECT a FROM t LIMIT 5, 10").unwrap();
    assert_eq!(
        select.limit,
        Some(Box::new(Limit {
            count: Some(num("10")),
            offset: Some(num("5")),
            offset_rows: OffsetRows::None,
            offset_comma: true,
            all: false,
            by: Vec::new(),
        }))
    );
}

#[test]
fn shared_syntax_produces_equal_trees_across_dialects() {
    let sql = "SELECT a + 1 AS b FROM t WHERE a > 2 LIMIT 3";
    let expected = parse_query_as(Core, sql).unwrap();
    for kind in [MySql, Postgres, Redshift] {
        assert_eq!(parse_query_as(kind, sql).unwrap(), expected, "{kind:?}");
    }
}

/// Stands in for a downstream tool: one non-generic function handles the
/// output of every built-in dialect.
fn cast_target<'q>(query: &'q Query<'_, Builtin>) -> Option<&'q str> {
    match &body(query).projection[0].kind {
        SelectItemKind::Expr {
            expr:
                Expr {
                    kind: ExprKind::Cast { ty, .. },
                    ..
                },
            ..
        } => Some(ty.name.text.as_ref()),
        _ => None,
    }
}

#[test]
fn one_non_generic_consumer_handles_any_builtin_dialect() {
    let sql = "SELECT x::int FROM t";
    for kind in [Postgres, Redshift] {
        let select = parse_query_as(kind, sql).unwrap();
        assert_eq!(cast_target(&select), Some("int"), "{kind:?}");
    }
    let plain = parse_query_as(MySql, "SELECT x FROM t").unwrap();
    assert_eq!(cast_target(&plain), None);
}

#[test]
fn identifiers_chosen_at_runtime_still_borrow_from_the_input() {
    let sql = String::from("SELECT a FROM tbl");
    let select = parse_query_as(Postgres, &sql).unwrap();
    let name = table_name(&body(&select).from[0].relation).parts[0]
        .text
        .clone();
    assert!(std::ptr::eq(name.as_ptr(), sql[14..].as_ptr()));
}
