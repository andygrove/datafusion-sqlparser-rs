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

//! Spans recorded while parsing: exact ranges for common shapes, and, for
//! every corpus statement, that a node's span really is that node's text.

#[cfg(feature = "visitor")]
#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use sqlparser::ast::{
    Expr, ExprKind, Location, SelectItem, Span, Spanned, Statement, StatementKind,
};
use sqlparser::dialect::{Builtin, MySql, Postgres};
use sqlparser::{parse_expr, parse_sql};

fn expr_text(sql: &str) -> &str {
    let expr: Expr<'_, Builtin> = parse_expr::<Postgres>(sql).unwrap();
    expr.span.text(sql)
}

/// The sub-expressions' text, for a binary expression.
fn operands(sql: &str) -> (&str, &str) {
    let expr: Expr<'_, Builtin> = parse_expr::<Postgres>(sql).unwrap();
    let ExprKind::Binary { left, right, .. } = expr.kind else {
        panic!("not binary: {sql}");
    };
    (left.span.text(sql), right.span.text(sql))
}

fn query_projection(sql: &str) -> Vec<SelectItem<'_, Builtin>> {
    let mut statements = parse_sql::<Postgres>(sql).unwrap();
    let StatementKind::Query(query) = statements.remove(0).kind else {
        panic!("not a query: {sql}");
    };
    let sqlparser::ast::SetExpr::Select(select) = query.body else {
        panic!("not a select: {sql}");
    };
    select.projection
}

#[test]
fn an_expression_covers_all_of_its_tokens() {
    for sql in [
        "a",
        "1",
        "a + b * c",
        "(a + b)",
        "((a))",
        "f(a, b)",
        "f()",
        "CASE WHEN a THEN b ELSE c END",
        "CAST(a AS int)",
        "a::int",
        "NOT a",
        "- a",
        "a IS NOT NULL",
        "a IN (1, 2)",
        "a BETWEEN 1 AND 2",
        "EXISTS (SELECT 1)",
        "(SELECT 1)",
        "'text'",
        "a.b.c",
    ] {
        assert_eq!(expr_text(sql), sql);
    }
}

#[test]
fn surrounding_whitespace_and_comments_are_not_included() {
    assert_eq!(expr_text("  a + b  "), "a + b");
    assert_eq!(expr_text("a + b -- trailing"), "a + b");
}

#[test]
fn operands_have_their_own_spans() {
    assert_eq!(operands("a + b"), ("a", "b"));
    assert_eq!(operands("(a) + (b)"), ("(a)", "(b)"));
    assert_eq!(operands("f(x) * g(y)"), ("f(x)", "g(y)"));
    // Left-associative: the left operand is the whole earlier chain.
    assert_eq!(operands("a + b + c"), ("a + b", "c"));
}

#[test]
fn a_statement_stops_before_its_semicolon() {
    let sql = " SELECT 1 ; INSERT INTO t VALUES (2);";
    let statements = parse_sql::<Postgres>(sql).unwrap();
    assert_eq!(statements[0].span.text(sql), "SELECT 1");
    assert_eq!(statements[1].span.text(sql), "INSERT INTO t VALUES (2)");
}

#[test]
fn a_select_item_includes_its_alias() {
    let sql = "SELECT a + 1 AS b, c d, *, t.* FROM t";
    let items: Vec<&str> = query_projection(sql)
        .iter()
        .map(|item| item.span.text(sql))
        .collect();
    assert_eq!(items, ["a + 1 AS b", "c d", "*", "t.*"]);
}

#[test]
fn a_table_reference_includes_its_alias() {
    let sql = "SELECT 1 FROM orders AS o JOIN (SELECT 2) s ON true";
    let mut statements = parse_sql::<Postgres>(sql).unwrap();
    let StatementKind::Query(query) = statements.remove(0).kind else {
        panic!("not a query");
    };
    let sqlparser::ast::SetExpr::Select(select) = query.body else {
        panic!("not a select");
    };
    let from = &select.from[0];
    assert_eq!(from.relation.span.text(sql), "orders AS o");
    assert_eq!(from.joins[0].table.span.text(sql), "(SELECT 2) s");
}

#[test]
fn dialect_syntax_is_covered_too() {
    let sql = "SELECT `a` DIV 2 FROM t LIMIT 1, 2";
    let statements = parse_sql::<MySql>(sql).unwrap();
    assert_eq!(statements[0].span().text(sql), sql);
}

#[test]
fn spans_are_ignored_by_equality() {
    let a: Vec<Statement<'_, Builtin>> = parse_sql::<Postgres>("SELECT 1").unwrap();
    let b: Vec<Statement<'_, Builtin>> = parse_sql::<Postgres>("  SELECT    1").unwrap();
    assert_ne!(a[0].span, b[0].span);
    assert_eq!(a, b);
}

#[test]
fn a_node_built_by_hand_has_an_empty_span() {
    let expr: Expr<'_, Builtin> = ExprKind::Placeholder("$1").into();
    assert_eq!(expr.span, Span::default());
}

#[test]
fn locations_count_lines_and_characters_from_one() {
    let sql = "SELECT 1,\n  é + b";
    let expr = Span { start: 12, end: 18 };
    assert_eq!(expr.text(sql), "é + b");
    assert_eq!(expr.start_location(sql), Location { line: 2, column: 3 });
    assert_eq!(expr.end_location(sql), Location { line: 2, column: 8 });
}

#[cfg(feature = "visitor")]
mod corpus_checks {
    use std::collections::BTreeMap;
    use std::ops::ControlFlow;

    use sqlparser::ast::{ExprKind, Span, SpecialExpr, visit_expressions};
    use sqlparser::dialect::DialectKind;
    use sqlparser::dialect::builtin::parse_sql_as;

    use super::corpus::{Kind, V2_DIALECTS, parse_corpus};

    fn kind_of(dialect: &str) -> DialectKind {
        if dialect == "ansi" {
            DialectKind::Core
        } else {
            DialectKind::from_name(dialect).unwrap_or_else(|| panic!("unknown {dialect}"))
        }
    }

    fn inside(inner: Span, outer: Span) -> bool {
        inner.start >= outer.start && inner.end <= outer.end
    }

    #[test]
    fn every_span_in_the_corpus_is_that_nodes_text() {
        let cases = parse_corpus(include_str!("corpus/v1.corpus"));
        let mut problems: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let mut note = |kind: String, sample: String| {
            let entry = problems.entry(kind).or_insert((0, sample));
            entry.0 += 1;
        };
        let (mut statements_seen, mut exprs_seen) = (0usize, 0usize);
        for dialect in V2_DIALECTS {
            let kind = kind_of(dialect);
            for case in cases.iter().filter(|case| case.kind == Kind::Statement) {
                let sql = case.sql.as_str();
                let Ok(statements) = parse_sql_as(kind, sql) else {
                    continue;
                };
                for statement in &statements {
                    statements_seen += 1;
                    if statement.span == Span::default() {
                        note("statement: empty".into(), sql.to_string());
                        continue;
                    }
                    let text = statement.span.text(sql);
                    let again = parse_sql_as(kind, text);
                    if !matches!(&again, Ok(v) if v.len() == 1 && v[0].to_string() == statement.to_string())
                    {
                        note(
                            "statement: slice differs".into(),
                            format!("[{dialect}] {sql}"),
                        );
                    }
                    let _ = visit_expressions(statement, |expr| {
                        exprs_seen += 1;
                        let variant = format!("{:?}", expr.kind);
                        let variant: String = variant
                            .chars()
                            .take_while(|c| c.is_alphanumeric())
                            .collect();
                        if expr.span == Span::default() {
                            note(
                                format!("expr empty: {variant}"),
                                format!("[{dialect}] {sql}"),
                            );
                        } else if !inside(expr.span, statement.span) {
                            note(
                                format!("expr outside statement: {variant}"),
                                format!("[{dialect}] {sql}"),
                            );
                        } else {
                            let text = expr.span.text(sql);
                            let select = format!("SELECT {text}");
                            let rendered = expr.to_string();
                            // Syntax valid only in its own context, like
                            // `GROUPING SETS`, can't be re-parsed alone.
                            let is_grouping = matches!(
                                &expr.kind,
                                ExprKind::Special(special)
                                    if matches!(**special, SpecialExpr::Grouping { .. })
                            );
                            // Rendering can reorder clauses, so ignore order and spacing.
                            let letters = |sql: &str| {
                                let mut letters: Vec<char> =
                                    sql.chars().filter(|c| !c.is_whitespace()).collect();
                                letters.sort_unstable();
                                letters
                            };
                            let verbatim = is_grouping
                                || letters(text) == letters(&rendered)
                                || format!("({text})") == rendered;
                            // Parentheses, for syntax that means something else
                            // directly after `SELECT`, like Redshift's `top`.
                            let bracketed = format!("SELECT ({text})");
                            let same = verbatim
                                || parse_sql_as(kind, &select).is_ok_and(|v| {
                                    v.len() == 1 && v[0].to_string() == format!("SELECT {expr}")
                                })
                                || parse_sql_as(kind, &bracketed).is_ok_and(|v| {
                                    v.len() == 1 && v[0].to_string() == format!("SELECT ({expr})")
                                });
                            if !same {
                                note(
                                    format!("expr slice differs: {variant}"),
                                    format!("[{dialect}] `{text}` vs `{expr}` in {sql}"),
                                );
                            }
                        }
                        ControlFlow::<()>::Continue(())
                    });
                }
            }
        }
        assert!(statements_seen > 10_000 && exprs_seen > 10_000);
        let total: usize = problems.values().map(|(count, _)| count).sum();
        let report: Vec<String> = problems
            .iter()
            .map(|(kind, (count, sample))| {
                let sample: String = sample.chars().take(150).collect();
                format!("{count:6} {kind}: {sample}")
            })
            .collect();
        assert_eq!(
            total,
            0,
            "{statements_seen} statements, {exprs_seen} exprs\n{}",
            report.join("\n")
        );
    }
}
