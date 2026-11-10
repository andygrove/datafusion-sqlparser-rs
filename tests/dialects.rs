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

//! The built-in dialects. Each test pins a behavior that differs by dialect.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::UnaryOp::*;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinSelect};
use sqlparser::dialect::mysql::MySqlExpr;
use sqlparser::dialect::{Core, MySql, Postgres, Redshift};
use sqlparser::{parse_expr, parse_query};

type E<'a> = Expr<'a, Builtin>;

fn int_div<'a>(left: E<'a>, right: E<'a>) -> E<'a> {
    ExprKind::Ext(BuiltinExpr::MySql(MySqlExpr::IntDiv {
        left: Box::new(left),
        right: Box::new(right),
    }))
    .into()
}

fn double_colon<'a>(expr: E<'a>, ty: &'a str) -> E<'a> {
    common::cast(expr, ty, CastStyle::DoubleColon)
}

fn my(sql: &str) -> E<'_> {
    parse_expr::<MySql>(sql).unwrap()
}

fn pg(sql: &str) -> E<'_> {
    parse_expr::<Postgres>(sql).unwrap()
}

fn limit_of(query: Query<'_, Builtin>) -> Limit<'_, Builtin> {
    *query.limit.expect("query has a LIMIT")
}

// Tokenizer level

#[test]
fn mysql_backticks_quote_identifiers() {
    assert_eq!(my("`a b`"), ExprKind::Ident(quoted("a b", '`')).into());
}

#[test]
fn mysql_doubled_backtick_is_resolved_by_value() {
    let ExprKind::Ident(id) = my("`a``b`").kind else {
        panic!("expected an identifier");
    };
    assert_eq!(id.value(), "a`b");
}

#[test]
fn mysql_double_quotes_delimit_strings() {
    assert_eq!(
        my("\"a\""),
        ExprKind::Literal(Literal::String(StringLiteral {
            text: "a".into(),
            quote: '"',
            backslash_escapes: true,
            national: false,
            prefix: Default::default(),
            triple: false,
        }))
        .into()
    );
}

#[test]
fn postgres_double_quotes_quote_identifiers() {
    assert_eq!(pg("\"a\""), ExprKind::Ident(quoted("a", '"')).into());
}

#[test]
fn postgres_rejects_backticks() {
    assert!(parse_expr::<Postgres>("`a`").is_err());
}

// Expression level

#[test]
fn mysql_div_builds_the_dialect_node() {
    assert_eq!(my("a DIV b"), int_div(col("a"), col("b")));
}

#[test]
fn mysql_div_is_case_insensitive() {
    assert_eq!(my("a div b"), int_div(col("a"), col("b")));
}

#[test]
fn mysql_div_binds_tighter_than_addition() {
    assert_eq!(
        my("a + b DIV c"),
        bin(col("a"), Add, int_div(col("b"), col("c")))
    );
}

#[test]
fn mysql_div_is_left_associative_with_multiplication() {
    assert_eq!(
        my("a DIV b * c"),
        bin(int_div(col("a"), col("b")), Mul, col("c"))
    );
}

#[test]
fn mysql_div_is_reserved() {
    assert!(parse_query::<MySql>("SELECT 1 FROM t div").is_err());
}

#[test]
fn postgres_treats_div_as_an_ordinary_alias() {
    let select = into_select(parse_query::<Postgres>("SELECT a div FROM t").unwrap());
    assert_eq!(
        select.projection,
        vec![
            SelectItemKind::Expr {
                expr: col("a"),
                alias: Some(ident("div"))
            }
            .into()
        ]
    );
}

#[test]
fn postgres_cast_builds_the_dialect_node() {
    assert_eq!(pg("x::int"), double_colon(col("x"), "int"));
}

#[test]
fn postgres_casts_chain_left_to_right() {
    assert_eq!(
        pg("x::int::text"),
        double_colon(double_colon(col("x"), "int"), "text")
    );
}

#[test]
fn postgres_cast_binds_tighter_than_addition() {
    assert_eq!(
        pg("a + b::int"),
        bin(col("a"), Add, double_colon(col("b"), "int"))
    );
}

#[test]
fn postgres_cast_binds_tighter_than_unary_minus() {
    assert_eq!(pg("-x::int"), unary(Neg, double_colon(col("x"), "int")));
}

#[test]
fn mysql_rejects_postgres_cast() {
    assert!(parse_expr::<MySql>("x::int").is_err());
}

// Clause level

#[test]
fn mysql_limit_comma_puts_offset_first() {
    let limit = limit_of(parse_query::<MySql>("SELECT a FROM t LIMIT 5, 10").unwrap());
    assert_eq!(
        limit,
        Limit {
            count: Some(num("10")),
            offset: Some(num("5")),
            offset_rows: OffsetRows::None,
            offset_comma: true,
            all: false,
            by: Vec::new(),
        }
    );
}

#[test]
fn mysql_keeps_standard_limit_offset() {
    let limit = limit_of(parse_query::<MySql>("SELECT a FROM t LIMIT 10 OFFSET 5").unwrap());
    assert_eq!(
        limit,
        Limit {
            count: Some(num("10")),
            offset: Some(num("5")),
            offset_rows: OffsetRows::None,
            offset_comma: false,
            all: false,
            by: Vec::new(),
        }
    );
}

#[test]
fn mysql_limit_without_offset_has_no_offset() {
    let limit = limit_of(parse_query::<MySql>("SELECT a FROM t LIMIT 10").unwrap());
    assert_eq!(
        limit,
        Limit {
            count: Some(num("10")),
            offset: None,
            offset_rows: OffsetRows::None,
            offset_comma: false,
            all: false,
            by: Vec::new(),
        }
    );
}

#[test]
fn postgres_rejects_limit_comma() {
    assert!(parse_query::<Postgres>("SELECT a FROM t LIMIT 5, 10").is_err());
}

// Building one dialect on another

#[test]
fn redshift_inherits_postgres_cast() {
    assert_eq!(
        parse_expr::<Redshift>("x::int").unwrap(),
        double_colon(col("x"), "int")
    );
}

#[test]
fn redshift_parses_select_top() {
    let select = into_select(parse_query::<Redshift>("SELECT TOP 10 a FROM t").unwrap());
    assert_eq!(
        select.modifiers,
        BuiltinSelect {
            top: Some("10".into()),
            ..BuiltinSelect::default()
        }
    );
    assert_eq!(
        select.projection,
        vec![
            SelectItemKind::Expr {
                expr: col("a"),
                alias: None
            }
            .into()
        ]
    );
}

#[test]
fn redshift_select_without_top_has_no_modifier() {
    let select = into_select(parse_query::<Redshift>("SELECT a FROM t").unwrap());
    assert_eq!(select.modifiers, BuiltinSelect::default());
}

#[test]
fn redshift_top_requires_a_number() {
    let err = parse_query::<Redshift>("SELECT TOP a b FROM t").unwrap_err();
    assert_eq!(err.span, Span { start: 11, end: 12 });
}

#[test]
fn only_redshift_accepts_select_top() {
    assert!(parse_query::<Postgres>("SELECT TOP 10 a FROM t").is_err());
    assert!(parse_query::<Core>("SELECT TOP 10 a FROM t").is_err());
}

// The cast node is shared by every dialect

#[test]
fn mysql_accepts_the_standard_cast() {
    assert_eq!(
        my("CAST(a AS int)"),
        common::cast(col("a"), "int", CastStyle::Function)
    );
}

#[test]
fn postgres_double_colon_can_follow_a_standard_cast() {
    assert_eq!(
        pg("CAST(x AS int)::text"),
        double_colon(common::cast(col("x"), "int", CastStyle::Function), "text")
    );
}
