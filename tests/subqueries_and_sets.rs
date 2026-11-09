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

//! Subqueries, set operations, `HAVING`, `t.*` and reserved words after a dot.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::SetOperator::*;
use sqlparser::ast::UnaryOp::*;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinSelect};
use sqlparser::dialect::{Core, MySql, Postgres};
use sqlparser::{parse_expr, parse_query};

type E<'a> = Expr<'a, Builtin>;
type Q<'a> = Query<'a, Builtin>;
type S<'a> = SetExpr<'a, Builtin>;

fn expr(sql: &str) -> E<'_> {
    parse_expr::<Core>(sql).unwrap()
}

fn query(sql: &str) -> Q<'_> {
    parse_query::<Core>(sql).unwrap()
}

/// The set expression for `SELECT <number>`.
fn sel(number: &str) -> S<'_> {
    SetExpr::Select(Box::new(Select {
        modifiers: BuiltinSelect::default(),
        quantifier: SetQuantifier::None,
        projection: vec![
            SelectItemKind::Expr {
                expr: num(number),
                alias: None,
            }
            .into(),
        ],
        into: None,
        from: vec![],
        selection: None,
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
        lateral_views: Vec::new(),
    }))
}

fn set<'a>(left: S<'a>, op: SetOperator, quantifier: SetQuantifier, right: S<'a>) -> S<'a> {
    SetExpr::SetOperation {
        left: Box::new(left),
        op,
        quantifier,
        by_name: false,
        right: Box::new(right),
    }
}

fn union<'a>(left: S<'a>, right: S<'a>) -> S<'a> {
    set(left, Union, SetQuantifier::None, right)
}

/// The query for `SELECT <number>`.
fn q(number: &str) -> Q<'_> {
    Query {
        with: vec![],
        body: sel(number),
        order_by: vec![],
        distribution: None,
        limit: None,
        fetch: None,
        locks: Vec::new(),
        tail: Default::default(),
        pipes: Vec::new(),
    }
}

fn scalar(number: &str) -> E<'_> {
    ExprKind::Subquery(Box::new(q(number))).into()
}

// HAVING

#[test]
fn having_keeps_its_condition() {
    let parsed = query("SELECT 1 FROM t GROUP BY a HAVING a > 1");
    assert_eq!(body(&parsed).having, Some(bin(col("a"), Gt, num("1"))));
}

#[test]
fn having_comes_before_order_by() {
    let parsed = query("SELECT 1 FROM t GROUP BY a HAVING h ORDER BY o");
    assert_eq!(body(&parsed).having, Some(col("h")));
    assert_eq!(parsed.order_by[0].expr, col("o"));
}

#[test]
fn having_is_not_taken_as_a_table_alias() {
    let parsed = query("SELECT 1 FROM t HAVING h");
    assert_eq!(body(&parsed).from[0].relation.alias, None);
}

// Set operations

#[test]
fn union_combines_two_selects() {
    assert_eq!(
        query("SELECT 1 UNION SELECT 2").body,
        union(sel("1"), sel("2"))
    );
}

#[test]
fn quantifier_is_recorded() {
    assert_eq!(
        query("SELECT 1 UNION ALL SELECT 2").body,
        set(sel("1"), Union, SetQuantifier::All, sel("2"))
    );
    assert_eq!(
        query("SELECT 1 UNION DISTINCT SELECT 2").body,
        set(sel("1"), Union, SetQuantifier::Distinct, sel("2"))
    );
}

#[test]
fn except_and_intersect_are_set_operators() {
    assert_eq!(
        query("SELECT 1 EXCEPT SELECT 2").body,
        set(sel("1"), Except, SetQuantifier::None, sel("2"))
    );
    assert_eq!(
        query("SELECT 1 INTERSECT SELECT 2").body,
        set(sel("1"), Intersect, SetQuantifier::None, sel("2"))
    );
}

#[test]
fn union_is_left_associative() {
    assert_eq!(
        query("SELECT 1 UNION SELECT 2 UNION SELECT 3").body,
        union(union(sel("1"), sel("2")), sel("3"))
    );
}

#[test]
fn intersect_binds_tighter_than_union() {
    assert_eq!(
        query("SELECT 1 UNION SELECT 2 INTERSECT SELECT 3").body,
        union(
            sel("1"),
            set(sel("2"), Intersect, SetQuantifier::None, sel("3"))
        )
    );
}

#[test]
fn intersect_binds_tighter_than_except() {
    assert_eq!(
        query("SELECT 1 INTERSECT SELECT 2 EXCEPT SELECT 3").body,
        set(
            set(sel("1"), Intersect, SetQuantifier::None, sel("2")),
            Except,
            SetQuantifier::None,
            sel("3")
        )
    );
}

#[test]
fn parentheses_group_set_operations() {
    assert_eq!(
        query("(SELECT 1 UNION SELECT 2) INTERSECT SELECT 3").body,
        set(
            SetExpr::Query(Box::new(Query {
                with: vec![],
                body: union(sel("1"), sel("2")),
                order_by: vec![],
                distribution: None,
                limit: None,
                fetch: None,
                locks: Vec::new(),
                tail: Default::default(),
                pipes: Vec::new(),
            })),
            Intersect,
            SetQuantifier::None,
            sel("3")
        )
    );
}

#[test]
fn order_by_and_limit_apply_to_the_whole_union() {
    let parsed = query("SELECT 1 UNION SELECT 2 ORDER BY a LIMIT 3");
    assert_eq!(parsed.body, union(sel("1"), sel("2")));
    assert_eq!(parsed.order_by[0].expr, col("a"));
    assert_eq!(parsed.limit.unwrap().count, Some(num("3")));
}

#[test]
fn order_by_cannot_precede_a_set_operator_without_parentheses() {
    assert!(parse_query::<Core>("SELECT 1 ORDER BY a UNION SELECT 2").is_err());
}

#[test]
fn parenthesized_branch_keeps_its_own_limit() {
    let parsed = query("(SELECT 1 LIMIT 5) UNION SELECT 2");
    let SetExpr::SetOperation { left, .. } = parsed.body else {
        panic!("expected a set operation");
    };
    let SetExpr::Query(inner) = *left else {
        panic!("expected a parenthesized query");
    };
    assert_eq!(inner.limit.unwrap().count, Some(num("5")));
    assert_eq!(parsed.limit, None);
}

#[test]
fn with_applies_to_the_whole_set_operation() {
    let parsed = query("WITH c AS (SELECT 1) SELECT 2 UNION SELECT 3");
    assert_eq!(parsed.with.len(), 1);
    assert_eq!(parsed.body, union(sel("2"), sel("3")));
}

#[test]
fn set_operators_are_not_taken_as_aliases() {
    let parsed = query("SELECT a UNION SELECT b FROM t EXCEPT SELECT c FROM u INTERSECT SELECT 1");
    let SetExpr::SetOperation { left, .. } = parsed.body else {
        panic!("expected a set operation");
    };
    let SetExpr::SetOperation { left: first, .. } = *left else {
        panic!("expected a nested set operation");
    };
    let SetExpr::Select(select) = *first else {
        panic!("expected a select");
    };
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
fn union_requires_a_right_side() {
    assert!(parse_query::<Core>("SELECT 1 UNION").is_err());
}

// Scalar subqueries

#[test]
fn parenthesized_select_is_a_scalar_subquery() {
    assert_eq!(expr("(SELECT 1)"), scalar("1"));
}

#[test]
fn scalar_subquery_is_an_operand() {
    assert_eq!(expr("a + (SELECT 1)"), bin(col("a"), Add, scalar("1")));
}

#[test]
fn extra_parentheses_around_a_subquery_are_allowed() {
    assert_eq!(expr("((SELECT 1))"), parens(scalar("1")));
}

#[test]
fn scalar_subquery_may_start_with_with() {
    let ExprKind::Subquery(inner) = expr("(WITH c AS (SELECT 1) SELECT 2)").kind else {
        panic!("expected a subquery");
    };
    assert_eq!(inner.with[0].name, ident("c"));
}

#[test]
fn scalar_subquery_may_contain_a_union() {
    let ExprKind::Subquery(inner) = expr("(SELECT 1 UNION SELECT 2)").kind else {
        panic!("expected a subquery");
    };
    assert_eq!(inner.body, union(sel("1"), sel("2")));
}

#[test]
fn subquery_works_as_a_function_argument() {
    assert_eq!(
        expr("f((SELECT 1))"),
        call(&["f"], vec![FunctionArg::Expr(scalar("1"))])
    );
}

#[test]
fn subquery_works_in_where() {
    let parsed = query("SELECT 1 FROM t WHERE a = (SELECT 1)");
    assert_eq!(
        body(&parsed).selection,
        Some(bin(col("a"), Eq, scalar("1")))
    );
}

// EXISTS

#[test]
fn exists_holds_its_query() {
    assert_eq!(
        expr("EXISTS (SELECT 1)"),
        ExprKind::Exists(Box::new(q("1"))).into()
    );
}

#[test]
fn not_exists_negates_exists() {
    assert_eq!(
        expr("NOT EXISTS (SELECT 1)"),
        unary(Not, ExprKind::Exists(Box::new(q("1"))).into())
    );
}

#[test]
fn exists_without_parenthesis_is_an_ordinary_column() {
    assert_eq!(expr("exists + 1"), bin(col("exists"), Add, num("1")));
}

#[test]
fn exists_requires_a_query() {
    assert!(parse_expr::<Core>("EXISTS (a)").is_err());
}

// IN

fn in_list<'a>(item: E<'a>, list: Vec<E<'a>>, negated: bool) -> E<'a> {
    ExprKind::InList {
        expr: Box::new(item),
        list,
        negated,
    }
    .into()
}

#[test]
fn in_list_keeps_every_item() {
    assert_eq!(
        expr("a IN (1, b)"),
        in_list(col("a"), vec![num("1"), col("b")], false)
    );
}

#[test]
fn not_in_list_is_negated() {
    assert_eq!(
        expr("a NOT IN (1)"),
        in_list(col("a"), vec![num("1")], true)
    );
}

#[test]
fn in_subquery_holds_its_query() {
    assert_eq!(
        expr("a IN (SELECT 1)"),
        ExprKind::InSubquery {
            expr: Box::new(col("a")),
            subquery: Box::new(q("1")),
            negated: false,
        }
        .into()
    );
}

#[test]
fn not_in_subquery_is_negated() {
    assert_eq!(
        expr("a NOT IN (SELECT 1)"),
        ExprKind::InSubquery {
            expr: Box::new(col("a")),
            subquery: Box::new(q("1")),
            negated: true,
        }
        .into()
    );
}

#[test]
fn in_binds_tighter_than_and() {
    assert_eq!(
        expr("a IN (1) AND b"),
        bin(in_list(col("a"), vec![num("1")], false), And, col("b"))
    );
}

#[test]
fn in_binds_looser_than_addition() {
    assert_eq!(
        expr("a + 1 IN (2)"),
        in_list(bin(col("a"), Add, num("1")), vec![num("2")], false)
    );
}

#[test]
fn not_prefix_wraps_the_whole_in_expression() {
    assert_eq!(
        expr("NOT a IN (1)"),
        unary(Not, in_list(col("a"), vec![num("1")], false))
    );
}

#[test]
fn empty_in_list_is_rejected() {
    assert!(parse_expr::<Core>("a IN ()").is_err());
}

#[test]
fn in_requires_a_closing_parenthesis() {
    assert!(parse_expr::<Core>("a IN (1").is_err());
}

#[test]
fn infix_not_requires_in() {
    assert!(parse_expr::<Core>("a NOT b").is_err());
}

// Derived tables

#[test]
fn derived_table_holds_its_query_and_alias() {
    let parsed = query("SELECT 1 FROM (SELECT 2) AS x");
    assert_eq!(
        body(&parsed).from[0].relation,
        TableRef {
            factor: TableFactor::Derived(Box::new(q("2"))),
            alias: Some(explicit_alias("x")),
            span: Span::default()
        }
    );
}

#[test]
fn derived_table_can_be_joined() {
    let parsed = query("SELECT 1 FROM (SELECT 2) x JOIN (SELECT 3) y ON a");
    let from = &body(&parsed).from[0];
    assert_eq!(from.joins[0].table.alias, Some(implicit_alias("y")));
    assert_eq!(
        from.joins[0].table.factor,
        TableFactor::Derived(Box::new(q("3")))
    );
}

#[test]
fn derived_tables_nest() {
    let parsed = query("SELECT 1 FROM (SELECT 2 FROM (SELECT 3) a) b");
    let TableFactor::Derived(outer) = &body(&parsed).from[0].relation.factor else {
        panic!("expected a derived table");
    };
    assert_eq!(
        body(outer).from[0].relation.factor,
        TableFactor::Derived(Box::new(q("3")))
    );
}

#[test]
fn derived_table_requires_a_closing_parenthesis() {
    assert!(parse_query::<Core>("SELECT 1 FROM (SELECT 2").is_err());
}

// Dialects inside nested queries

#[test]
fn mysql_rules_apply_inside_a_derived_table() {
    let parsed = parse_query::<MySql>("SELECT 1 FROM (SELECT 2 LIMIT 5, 10) x").unwrap();
    let TableFactor::Derived(inner) = &body(&parsed).from[0].relation.factor else {
        panic!("expected a derived table");
    };
    let limit = inner.limit.as_ref().unwrap();
    assert_eq!(limit.count, Some(num("10")));
    assert_eq!(limit.offset, Some(num("5")));
}

#[test]
fn postgres_rules_apply_inside_a_scalar_subquery() {
    assert!(parse_expr::<Postgres>("(SELECT x::int)").is_ok());
    assert!(parse_expr::<MySql>("(SELECT x::int)").is_err());
}

// Stack safety: every recursive path must hit the nesting limit.

const DEEP: usize = 100_000;
const MODERATE: usize = 20;

fn nested(open: &str, center: &str, close: &str, depth: usize) -> String {
    format!("{}{center}{}", open.repeat(depth), close.repeat(depth))
}

#[test]
fn deep_scalar_subqueries_error_instead_of_overflowing_the_stack() {
    assert!(parse_expr::<Core>(&nested("(SELECT ", "1", ")", DEEP)).is_err());
}

#[test]
fn deep_exists_errors_instead_of_overflowing_the_stack() {
    assert!(parse_expr::<Core>(&nested("EXISTS (SELECT ", "1", ")", DEEP)).is_err());
}

#[test]
fn deep_in_subqueries_error_instead_of_overflowing_the_stack() {
    assert!(parse_expr::<Core>(&nested("a IN (SELECT ", "1", ")", DEEP)).is_err());
}

#[test]
fn deep_derived_tables_error_instead_of_overflowing_the_stack() {
    let sql = nested("SELECT 1 FROM (", "SELECT 1", ") x", DEEP);
    assert!(parse_query::<Core>(&sql).is_err());
}

#[test]
fn deep_parenthesized_queries_error_instead_of_overflowing_the_stack() {
    assert!(parse_query::<Core>(&nested("(", "SELECT 1", ")", DEEP)).is_err());
}

#[test]
fn moderately_nested_queries_parse() {
    assert!(parse_expr::<Core>(&nested("(SELECT ", "1", ")", MODERATE)).is_ok());
    assert!(parse_expr::<Core>(&nested("EXISTS (SELECT ", "1", ")", MODERATE)).is_ok());
    assert!(parse_expr::<Core>(&nested("a IN (SELECT ", "1", ")", MODERATE)).is_ok());
    let derived = nested("SELECT 1 FROM (", "SELECT 1", ") x", MODERATE);
    assert!(parse_query::<Core>(&derived).is_ok());
    assert!(parse_query::<Core>(&nested("(", "SELECT 1", ")", MODERATE)).is_ok());
}

// Qualified wildcards

#[test]
fn qualified_wildcard_keeps_its_qualifier() {
    let parsed = query("SELECT t.* FROM t");
    assert_eq!(
        body(&parsed).projection,
        vec![SelectItemKind::QualifiedWildcard(object_name(&["t"])).into()]
    );
}

#[test]
fn qualified_wildcard_may_have_several_qualifiers() {
    let parsed = query("SELECT s.t.* FROM s.t");
    assert_eq!(
        body(&parsed).projection,
        vec![SelectItemKind::QualifiedWildcard(object_name(&["s", "t"])).into()]
    );
}

#[test]
fn qualified_wildcard_mixes_with_other_items() {
    let parsed = query("SELECT a, t.*, b.c FROM t");
    assert_eq!(
        body(&parsed).projection,
        vec![
            SelectItemKind::Expr {
                expr: col("a"),
                alias: None
            }
            .into(),
            SelectItemKind::QualifiedWildcard(object_name(&["t"])).into(),
            SelectItemKind::Expr {
                expr: ExprKind::Compound(object_name(&["b", "c"])).into(),
                alias: None
            }
            .into(),
        ]
    );
}

#[test]
fn qualified_wildcard_works_with_quoted_names() {
    let parsed = parse_query::<MySql>("SELECT `t`.* FROM t").unwrap();
    let SelectItemKind::QualifiedWildcard(name) = &body(&parsed).projection[0].kind else {
        panic!("expected a qualified wildcard");
    };
    assert_eq!(name.parts, vec![quoted("t", '`')]);
}

#[test]
fn qualified_wildcard_is_rejected_inside_an_expression() {
    assert!(parse_query::<Core>("SELECT 1 + t.* FROM t").is_err());
}

#[test]
fn qualified_wildcard_must_end_the_name() {
    assert!(parse_query::<Core>("SELECT t.*.a FROM t").is_err());
}

#[test]
fn multiplication_after_a_qualified_name_is_not_a_wildcard() {
    let parsed = query("SELECT t.a * 2 FROM t");
    assert_eq!(
        body(&parsed).projection,
        vec![
            SelectItemKind::Expr {
                expr: bin(
                    ExprKind::Compound(object_name(&["t", "a"])).into(),
                    Mul,
                    num("2")
                ),
                alias: None
            }
            .into()
        ]
    );
}

// Reserved words after a dot

#[test]
fn reserved_word_is_allowed_after_a_dot() {
    assert_eq!(
        expr("t.order"),
        ExprKind::Compound(object_name(&["t", "order"])).into()
    );
}

#[test]
fn several_reserved_words_are_allowed_after_dots() {
    assert_eq!(
        expr("s.from.select"),
        ExprKind::Compound(object_name(&["s", "from", "select"])).into()
    );
}

#[test]
fn reserved_word_is_allowed_as_a_qualified_function_name() {
    assert_eq!(expr("s.left()"), call(&["s", "left"], vec![]));
}

#[test]
fn reserved_word_is_allowed_after_a_dot_in_a_table_name() {
    let parsed = query("SELECT 1 FROM s.group");
    assert_eq!(
        body(&parsed).from[0].relation.factor,
        named(&["s", "group"])
    );
}

#[test]
fn reserved_word_is_still_rejected_before_a_dot() {
    assert!(parse_expr::<Core>("order.t").is_err());
}

#[test]
fn number_is_rejected_after_a_dot() {
    assert!(parse_expr::<Core>("t.1").is_err());
}
