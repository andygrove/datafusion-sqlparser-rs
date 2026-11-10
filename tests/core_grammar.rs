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

//! The core grammar, exercised through the extension-free `Core` dialect.

mod common;

use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use std::borrow::Cow;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::UnaryOp::*;
use sqlparser::ast::*;
use sqlparser::dialect::Core;
use sqlparser::dialect::builtin::{Builtin, BuiltinSelect};
use sqlparser::{grammar, parse_expr, parse_query};

type E<'a> = Expr<'a, Builtin>;

fn expr(sql: &str) -> E<'_> {
    parse_expr::<Core>(sql).unwrap()
}

fn c(name: &str) -> E<'_> {
    col(name)
}

#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_eq!(
        expr("a + b * c"),
        bin(c("a"), Add, bin(c("b"), Mul, c("c")))
    );
}

#[test]
fn operators_of_equal_precedence_are_left_associative() {
    assert_eq!(
        expr("a - b - c"),
        bin(bin(c("a"), Sub, c("b")), Sub, c("c"))
    );
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(
        expr("(a + b) * c"),
        bin(parens(bin(c("a"), Add, c("b"))), Mul, c("c"))
    );
}

#[test]
fn and_binds_tighter_than_or() {
    assert_eq!(
        expr("a OR b AND c"),
        bin(c("a"), Or, bin(c("b"), And, c("c")))
    );
}

#[test]
fn comparison_binds_tighter_than_and() {
    assert_eq!(
        expr("a = b AND c <> d"),
        bin(bin(c("a"), Eq, c("b")), And, bin(c("c"), NotEq, c("d")))
    );
}

#[test]
fn multi_character_comparisons_are_single_operators() {
    assert_eq!(expr("a <= b"), bin(c("a"), LtEq, c("b")));
    assert_eq!(expr("a >= b"), bin(c("a"), GtEq, c("b")));
    assert_eq!(expr("a != b"), bin(c("a"), NotEq, c("b")));
}

#[test]
fn not_binds_looser_than_comparison() {
    assert_eq!(expr("NOT a = b"), unary(Not, bin(c("a"), Eq, c("b"))));
}

#[test]
fn unary_minus_binds_tighter_than_multiplication() {
    assert_eq!(expr("-a * b"), bin(unary(Neg, c("a")), Mul, c("b")));
}

#[test]
fn keywords_are_case_insensitive() {
    assert_eq!(expr("a oR b"), bin(c("a"), Or, c("b")));
}

#[test]
fn full_select_produces_every_clause() {
    let select = parse_query::<Core>(
        "SELECT a, b AS c, d e, * FROM t x WHERE a > 1 ORDER BY a DESC, b ASC, f LIMIT 10 OFFSET 5;",
    )
    .unwrap();

    let expected: Query<'_, Builtin> = Query {
        with: None,
        body: SetExpr::Select(Box::new(Select {
            modifiers: BuiltinSelect::default(),
            quantifier: SetQuantifier::None,
            projection: vec![
                SelectItemKind::Expr {
                    expr: c("a"),
                    alias: None,
                }
                .into(),
                SelectItemKind::Expr {
                    expr: c("b"),
                    alias: Some(ident("c")),
                }
                .into(),
                SelectItemKind::Expr {
                    expr: c("d"),
                    alias: Some(ident("e")),
                }
                .into(),
                SelectItemKind::Wildcard.into(),
            ],
            into: None,
            from: vec![TableWithJoins {
                relation: TableRef {
                    factor: named(&["t"]),
                    alias: Some(implicit_alias("x")),
                    span: Span::default(),
                },
                joins: vec![],
            }],
            selection: Some(bin(c("a"), Gt, num("1"))),
            connect_by: Vec::new(),
            group_by: vec![],
            group_by_all: false,
            group_by_modifier: None,
            grouping_sets: None,
            prewhere: None,
            having: None,
            qualify: None,
            windows: Vec::new(),
            from_first: false,
            span: Span::default(),
            lateral_views: Vec::new(),
        })),
        order_by: vec![
            OrderByItem {
                using: None,
                expr: c("a"),
                asc: Some(false),
                nulls_first: None,
                ext: Default::default(),
            },
            OrderByItem {
                using: None,
                expr: c("b"),
                asc: Some(true),
                nulls_first: None,
                ext: Default::default(),
            },
            OrderByItem {
                using: None,
                expr: c("f"),
                asc: None,
                nulls_first: None,
                ext: Default::default(),
            },
        ],
        distribution: None,
        limit: Some(Box::new(Limit {
            count: Some(num("10")),
            offset: Some(num("5")),
            offset_rows: OffsetRows::None,
            offset_comma: false,
            all: false,
            by: Vec::new(),
        })),
        fetch: None,
        locks: Vec::new(),
        tail: Default::default(),
        pipes: Vec::new(),
    };
    assert_eq!(select, expected);
}

#[test]
fn select_without_from_is_allowed() {
    let select = into_select(parse_query::<Core>("SELECT 1").unwrap());
    assert!(select.from.is_empty());
    assert_eq!(select.projection.len(), 1);
}

#[test]
fn reserved_word_is_not_taken_as_a_column() {
    assert!(parse_query::<Core>("SELECT FROM t").is_err());
}

#[test]
fn trailing_tokens_are_rejected() {
    let err = parse_query::<Core>("SELECT a FROM t x y").unwrap_err();
    assert_eq!(err.span, Span { start: 18, end: 19 });
}

#[test]
fn unterminated_string_reports_where_it_starts() {
    let err = parse_expr::<Core>("a = 'oops").unwrap_err();
    assert_eq!(err.span.start, 4);
}

#[test]
fn deeply_nested_parentheses_error_instead_of_overflowing_the_stack() {
    let sql = format!("{}a{}", "(".repeat(100_000), ")".repeat(100_000));
    assert!(parse_expr::<Core>(&sql).is_err());
}

#[test]
fn moderately_nested_parentheses_parse() {
    let sql = format!("{}a{}", "(".repeat(50), ")".repeat(50));
    let mut tree = parse_expr::<Core>(&sql).unwrap();
    for _ in 0..50 {
        let ExprKind::Nested(inner) = tree.kind else {
            panic!("expected a parenthesized expression");
        };
        tree = *inner;
    }
    assert_eq!(tree, c("a"));
}

#[test]
fn identifiers_borrow_from_the_input() {
    let sql = String::from("big_column_name");
    let ExprKind::Ident(id) = parse_expr::<Core>(&sql).unwrap().kind else {
        panic!("expected an identifier");
    };
    assert!(std::ptr::eq(id.text.as_ptr(), sql.as_ptr()));
}

#[test]
fn identifier_records_its_span() {
    let select = into_select(parse_query::<Core>("SELECT a FROM tbl").unwrap());
    assert_eq!(
        table_name(&select.from[0].relation).parts[0].span,
        Span { start: 14, end: 17 }
    );
}

#[test]
fn quoted_identifier_without_escapes_value_does_not_allocate() {
    let ExprKind::Ident(id) = expr("\"plain\"").kind else {
        panic!("expected an identifier");
    };
    assert_eq!(id, quoted("plain", '"'));
    assert!(matches!(id.value(), Cow::Borrowed("plain")));
}

#[test]
fn doubled_quote_in_identifier_is_resolved_by_value() {
    let ExprKind::Ident(id) = expr("\"a\"\"b\"").kind else {
        panic!("expected an identifier");
    };
    assert_eq!(id.text, "a\"\"b");
    assert_eq!(id.value(), "a\"b");
}

#[test]
fn doubled_quote_in_string_is_resolved_by_value() {
    let ExprKind::Literal(Literal::String(s)) = expr("'it''s'").kind else {
        panic!("expected a string");
    };
    assert_eq!(s.text, "it''s");
    assert_eq!(s.value(), "it's");
}

#[test]
fn quoted_reserved_word_is_an_identifier() {
    assert_eq!(
        expr("\"from\""),
        ExprKind::Ident(quoted("from", '"')).into()
    );
}

#[test]
fn non_ascii_identifiers_are_supported() {
    assert_eq!(expr("größe"), c("größe"));
}

#[test]
fn decimal_number_is_a_single_literal() {
    assert_eq!(expr("1.5"), num("1.5"));
}

#[test]
fn division_and_modulo_map_to_their_operators() {
    assert_eq!(expr("a / b"), bin(c("a"), Div, c("b")));
    assert_eq!(expr("a % b"), bin(c("a"), Mod, c("b")));
}

#[test]
fn cast_function_builds_a_cast_node() {
    assert_eq!(
        expr("CAST(a AS int)"),
        cast(c("a"), "int", CastStyle::Function)
    );
}

#[test]
fn cast_keywords_are_case_insensitive() {
    assert_eq!(
        expr("cast(a as int)"),
        cast(c("a"), "int", CastStyle::Function)
    );
}

#[test]
fn cast_accepts_a_full_expression() {
    assert_eq!(
        expr("CAST(a + b AS int)"),
        cast(bin(c("a"), Add, c("b")), "int", CastStyle::Function)
    );
}

#[test]
fn cast_without_parenthesis_is_an_ordinary_column() {
    assert_eq!(expr("cast + 1"), bin(c("cast"), Add, num("1")));
}

#[test]
fn cast_requires_as() {
    assert!(parse_expr::<Core>("CAST(a int)").is_err());
}

#[test]
fn cast_requires_a_closing_parenthesis() {
    assert!(parse_expr::<Core>("CAST(a AS int").is_err());
}

#[test]
fn dialect_syntax_is_rejected_by_the_core_grammar() {
    assert!(parse_expr::<Core>("`a`").is_err());
    assert!(parse_expr::<Core>("a::int").is_err());
    assert!(parse_query::<Core>("SELECT a FROM t LIMIT 5, 10").is_err());
}

const CORE_RESERVED_WORDS: [&str; 37] = [
    "NULL",
    "TRUE",
    "FALSE",
    "CASE",
    "DISTINCT",
    "ALL",
    "NATURAL",
    "SET",
    "HAVING",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "IN",
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP",
    "ORDER",
    "BY",
    "LIMIT",
    "OFFSET",
    "AS",
    "AND",
    "OR",
    "NOT",
    "ASC",
    "DESC",
    "WITH",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "OUTER",
    "CROSS",
    "ON",
    "USING",
];

#[test]
fn every_core_reserved_word_matches_in_any_case() {
    for word in CORE_RESERVED_WORDS {
        let lower = word.to_ascii_lowercase();
        let mut mixed = lower.clone();
        mixed[..1].make_ascii_uppercase();
        assert!(grammar::is_reserved(word), "{word}");
        assert!(grammar::is_reserved(&lower), "{lower}");
        assert!(grammar::is_reserved(&mixed), "{mixed}");
    }
}

#[test]
fn near_misses_are_not_reserved() {
    let words = [
        "",
        "A",
        "SELEC",
        "SELECTS",
        "OFFSETS",
        "INTERSECTS",
        "ORDERS",
        "AS1",
        "_AS",
        "ÄS",
        "sélect",
        "a_very_long_identifier_that_exceeds_every_keyword",
    ];
    for word in words {
        assert!(!grammar::is_reserved(word), "{word}");
    }
}
