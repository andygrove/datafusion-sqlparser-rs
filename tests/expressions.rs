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

//! `NULL` and boolean literals, `IS`, `LIKE`, `BETWEEN`, `CASE`, and aggregate
//! and window function syntax.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::UnaryOp::*;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift};
use sqlparser::{parse_expr, parse_query, parse_sql};

type E<'a> = Expr<'a, Builtin>;

fn expr(sql: &str) -> E<'_> {
    parse_expr::<Core>(sql).unwrap()
}

fn rejected(sql: &str) -> bool {
    parse_expr::<Core>(sql).is_err()
}

fn is<'a>(item: E<'a>, test: IsTest<'a, Builtin>, negated: bool) -> E<'a> {
    ExprKind::Is {
        expr: Box::new(item),
        test,
        negated,
    }
    .into()
}

fn like<'a>(item: E<'a>, pattern: E<'a>, negated: bool, kind: LikeKind<'a>) -> E<'a> {
    ExprKind::Like {
        expr: Box::new(item),
        pattern: Box::new(pattern),
        negated,
        kind,
        escape: None,
    }
    .into()
}

fn between<'a>(item: E<'a>, low: E<'a>, high: E<'a>, negated: bool) -> E<'a> {
    ExprKind::Between {
        expr: Box::new(item),
        low: Box::new(low),
        high: Box::new(high),
        negated,
    }
    .into()
}

fn options_of<'q, 'a>(function: &'q E<'a>) -> &'q FunctionOptions<'a, Builtin> {
    let ExprKind::Function {
        options: Some(options),
        ..
    } = &function.kind
    else {
        panic!("expected a function with options, found {function:?}");
    };
    options
}

fn window_of<'q, 'a>(function: &'q E<'a>) -> &'q WindowSpec<'a, Builtin> {
    match &options_of(function).over {
        Some(WindowOver::Spec(spec)) => spec,
        other => panic!("expected a window specification, found {other:?}"),
    }
}

// Literals

#[test]
fn null_is_a_literal_not_a_column() {
    assert_eq!(expr("NULL"), ExprKind::Literal(Literal::Null).into());
    assert_eq!(expr("null"), ExprKind::Literal(Literal::Null).into());
}

#[test]
fn booleans_are_literals_not_columns() {
    assert_eq!(
        expr("TRUE"),
        ExprKind::Literal(Literal::Boolean(true)).into()
    );
    assert_eq!(
        expr("false"),
        ExprKind::Literal(Literal::Boolean(false)).into()
    );
}

#[test]
fn literal_words_cannot_be_aliases() {
    for sql in ["SELECT a null", "SELECT a true", "SELECT a FROM t false"] {
        assert!(parse_query::<Core>(sql).is_err(), "{sql}");
    }
}

#[test]
fn quoted_literal_words_are_identifiers() {
    assert_eq!(
        expr("\"null\""),
        ExprKind::Ident(quoted("null", '"')).into()
    );
}

#[test]
fn default_null_is_the_literal() {
    let statements = parse_sql::<Core>("CREATE TABLE t (a int DEFAULT NULL NOT NULL)").unwrap();
    let StatementKind::CreateTable(create) = &statements[0].kind else {
        panic!("expected a CREATE TABLE");
    };
    assert_eq!(
        create.columns[0].options,
        vec![
            ColumnOption::Default(ExprKind::Literal(Literal::Null).into()),
            ColumnOption::NotNull,
        ]
    );
}

// IS

#[test]
fn is_null_tests() {
    assert_eq!(expr("a IS NULL"), is(col("a"), IsTest::Null, false));
    assert_eq!(expr("a IS NOT NULL"), is(col("a"), IsTest::Null, true));
}

#[test]
fn is_boolean_tests() {
    assert_eq!(expr("a IS TRUE"), is(col("a"), IsTest::True, false));
    assert_eq!(expr("a IS NOT FALSE"), is(col("a"), IsTest::False, true));
    assert_eq!(expr("a IS UNKNOWN"), is(col("a"), IsTest::Unknown, false));
}

#[test]
fn is_distinct_from_holds_its_operand() {
    assert_eq!(
        expr("a IS DISTINCT FROM b"),
        is(col("a"), IsTest::DistinctFrom(Box::new(col("b"))), false)
    );
    assert_eq!(
        expr("a IS NOT DISTINCT FROM b + 1"),
        is(
            col("a"),
            IsTest::DistinctFrom(Box::new(bin(col("b"), Add, num("1")))),
            true
        )
    );
}

#[test]
fn is_binds_looser_than_comparison() {
    assert_eq!(
        expr("a = b IS NULL"),
        is(bin(col("a"), Eq, col("b")), IsTest::Null, false)
    );
}

#[test]
fn is_binds_tighter_than_not() {
    assert_eq!(
        expr("NOT a IS NULL"),
        unary(Not, is(col("a"), IsTest::Null, false))
    );
}

#[test]
fn is_binds_tighter_than_and() {
    assert_eq!(
        expr("a IS NULL AND b"),
        bin(is(col("a"), IsTest::Null, false), And, col("b"))
    );
}

#[test]
fn is_requires_a_test() {
    assert!(rejected("a IS"));
    assert!(rejected("a IS 1"));
    assert!(rejected("a IS DISTINCT b"));
}

#[test]
fn is_null_is_not_taken_as_an_alias() {
    let parsed = parse_query::<Core>("SELECT a IS NULL FROM t").unwrap();
    assert_eq!(
        body(&parsed).projection,
        vec![
            SelectItemKind::Expr {
                expr: is(col("a"), IsTest::Null, false),
                alias: None,
            }
            .into()
        ]
    );
}

// LIKE

#[test]
fn like_holds_its_pattern() {
    assert_eq!(
        expr("a LIKE b"),
        like(col("a"), col("b"), false, LikeKind::Like)
    );
    assert_eq!(
        expr("a NOT LIKE b"),
        like(col("a"), col("b"), true, LikeKind::Like)
    );
}

#[test]
fn like_keeps_its_escape() {
    let ExprKind::Like { escape, .. } = expr("a LIKE b ESCAPE c").kind else {
        panic!("expected LIKE");
    };
    assert_eq!(escape, Some(Box::new(col("c"))));
}

#[test]
fn like_binds_tighter_than_and() {
    assert_eq!(
        expr("a LIKE b AND c"),
        bin(
            like(col("a"), col("b"), false, LikeKind::Like),
            And,
            col("c")
        )
    );
}

#[test]
fn like_requires_a_pattern() {
    assert!(rejected("a LIKE"));
}

#[test]
fn ilike_is_for_the_postgres_family() {
    let expected: E<'_> = like(col("a"), col("b"), false, LikeKind::ILike);
    assert_eq!(parse_expr::<Postgres>("a ILIKE b").unwrap(), expected);
    assert_eq!(parse_expr::<Redshift>("a ILIKE b").unwrap(), expected);
    assert_eq!(
        parse_expr::<Postgres>("a NOT ILIKE b").unwrap(),
        like(col("a"), col("b"), true, LikeKind::ILike)
    );
    assert!(parse_expr::<Core>("a ILIKE b").is_err());
    assert!(parse_expr::<MySql>("a NOT ILIKE b").is_err());
}

// BETWEEN

#[test]
fn between_holds_both_bounds() {
    assert_eq!(
        expr("a BETWEEN 1 AND 2"),
        between(col("a"), num("1"), num("2"), false)
    );
    assert_eq!(
        expr("a NOT BETWEEN 1 AND 2"),
        between(col("a"), num("1"), num("2"), true)
    );
}

#[test]
fn between_and_is_not_a_logical_and() {
    assert_eq!(
        expr("a BETWEEN 1 AND 2 AND b"),
        bin(between(col("a"), num("1"), num("2"), false), And, col("b"))
    );
}

#[test]
fn between_bounds_can_be_arithmetic() {
    assert_eq!(
        expr("a BETWEEN b + 1 AND c * 2"),
        between(
            col("a"),
            bin(col("b"), Add, num("1")),
            bin(col("c"), Mul, num("2")),
            false
        )
    );
}

#[test]
fn between_requires_and() {
    assert!(rejected("a BETWEEN 1"));
    assert!(rejected("a BETWEEN 1 OR 2"));
}

#[test]
fn infix_not_still_requires_a_known_operator() {
    assert!(rejected("a NOT b"));
}

// CASE

fn when<'a>(condition: E<'a>, result: E<'a>) -> CaseWhen<'a, Builtin> {
    CaseWhen { condition, result }
}

#[test]
fn searched_case_keeps_every_branch() {
    assert_eq!(
        expr("CASE WHEN a THEN 1 WHEN b THEN 2 ELSE 3 END"),
        ExprKind::Case(Box::new(CaseExpr {
            operand: None,
            branches: vec![when(col("a"), num("1")), when(col("b"), num("2"))],
            else_result: Some(num("3")),
        }))
        .into()
    );
}

#[test]
fn simple_case_keeps_its_operand() {
    assert_eq!(
        expr("CASE x WHEN 1 THEN 2 END"),
        ExprKind::Case(Box::new(CaseExpr {
            operand: Some(col("x")),
            branches: vec![when(num("1"), num("2"))],
            else_result: None,
        }))
        .into()
    );
}

#[test]
fn case_is_an_operand() {
    let ExprKind::Binary { op, .. } = expr("CASE WHEN a THEN 1 END + 2").kind else {
        panic!("expected an addition");
    };
    assert_eq!(op, Add);
}

#[test]
fn case_nests() {
    let ExprKind::Case(outer) = expr("CASE WHEN CASE WHEN a THEN b END THEN 1 END").kind else {
        panic!("expected CASE");
    };
    assert!(matches!(
        outer.branches[0].condition.kind,
        ExprKind::Case(_)
    ));
}

#[test]
fn case_requires_a_branch_and_an_end() {
    assert!(rejected("CASE END"));
    assert!(rejected("CASE x END"));
    assert!(rejected("CASE WHEN a THEN 1"));
    assert!(rejected("CASE WHEN a END"));
}

#[test]
fn case_is_reserved() {
    assert!(parse_query::<Core>("SELECT case FROM t").is_err());
}

#[test]
fn end_can_still_name_things() {
    assert!(parse_query::<Core>("SELECT end FROM t").is_ok());
}

// Function details

#[test]
fn plain_calls_have_no_options() {
    assert!(matches!(
        expr("f(a)").kind,
        ExprKind::Function { options: None, .. }
    ));
    assert!(matches!(
        expr("f()").kind,
        ExprKind::Function { options: None, .. }
    ));
}

#[test]
fn function_arguments_can_be_distinct() {
    let function = expr("COUNT(DISTINCT a, b)");
    assert_eq!(options_of(&function).quantifier, SetQuantifier::Distinct);
    let ExprKind::Function { args, .. } = &function.kind else {
        panic!("expected a function");
    };
    assert_eq!(args.len(), 2);
}

#[test]
fn function_arguments_can_be_all() {
    assert_eq!(options_of(&expr("f(ALL a)")).quantifier, SetQuantifier::All);
}

#[test]
fn distinct_requires_an_argument() {
    assert!(rejected("COUNT(DISTINCT)"));
}

#[test]
fn qualified_wildcard_is_an_argument() {
    let ExprKind::Function { args, .. } = expr("f(t.*)").kind else {
        panic!("expected a function");
    };
    assert_eq!(
        args,
        vec![FunctionArg::QualifiedWildcard(object_name(&["t"]))]
    );
}

#[test]
fn aggregate_keeps_its_order_by() {
    let function = expr("ARRAY_AGG(a ORDER BY b DESC, c)");
    let order_by = &options_of(&function).order_by;
    assert_eq!(order_by.len(), 2);
    assert_eq!(order_by[0].expr, col("b"));
    assert_eq!(order_by[0].asc, Some(false));
}

#[test]
fn filter_keeps_its_condition() {
    let function = expr("COUNT(*) FILTER (WHERE a > 1)");
    assert_eq!(
        options_of(&function).filter,
        Some(bin(col("a"), Gt, num("1")))
    );
}

#[test]
fn filter_needs_where_to_be_a_filter() {
    let parsed = parse_query::<Core>("SELECT f() filter FROM t").unwrap();
    let SelectItemKind::Expr { alias, .. } = &body(&parsed).projection[0].kind else {
        panic!("expected an expression");
    };
    assert_eq!(*alias, Some(ident("filter")));
}

// Window functions

#[test]
fn over_can_name_a_window() {
    assert_eq!(
        options_of(&expr("f() OVER w")).over,
        Some(WindowOver::Named(ident("w")))
    );
}

#[test]
fn over_can_be_empty() {
    let function = expr("f() OVER ()");
    let spec = window_of(&function);
    assert!(spec.partition_by.is_empty());
    assert!(spec.order_by.is_empty());
    assert_eq!(spec.frame, None);
}

#[test]
fn window_keeps_partition_and_order() {
    let function = expr("f() OVER (PARTITION BY a, b ORDER BY c)");
    let spec = window_of(&function);
    assert_eq!(spec.partition_by, vec![col("a"), col("b")]);
    assert_eq!(spec.order_by[0].expr, col("c"));
}

#[test]
fn frame_units_are_recorded() {
    let cases = [
        ("f() OVER (ROWS CURRENT ROW)", FrameUnits::Rows),
        ("f() OVER (RANGE CURRENT ROW)", FrameUnits::Range),
        ("f() OVER (GROUPS CURRENT ROW)", FrameUnits::Groups),
    ];
    for (sql, units) in cases {
        let function = expr(sql);
        assert_eq!(window_of(&function).frame.as_ref().unwrap().units, units);
    }
}

#[test]
fn frame_with_a_single_bound_has_no_end() {
    let function = expr("f() OVER (ROWS 1 PRECEDING)");
    let frame = window_of(&function).frame.as_ref().unwrap();
    assert_eq!(frame.start, FrameBound::Preceding(num("1")));
    assert_eq!(frame.end, None);
}

#[test]
fn frame_between_keeps_both_bounds() {
    let function = expr("f() OVER (ROWS BETWEEN UNBOUNDED PRECEDING AND 2 FOLLOWING)");
    let frame = window_of(&function).frame.as_ref().unwrap();
    assert_eq!(frame.start, FrameBound::UnboundedPreceding);
    assert_eq!(frame.end, Some(FrameBound::Following(num("2"))));
}

#[test]
fn frame_between_can_end_at_the_current_row_or_unbounded() {
    let function = expr("f() OVER (ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)");
    let frame = window_of(&function).frame.as_ref().unwrap();
    assert_eq!(frame.start, FrameBound::CurrentRow);
    assert_eq!(frame.end, Some(FrameBound::UnboundedFollowing));
}

#[test]
fn frame_bound_requires_a_direction() {
    assert!(rejected("f() OVER (ROWS 1)"));
    assert!(rejected("f() OVER (ROWS UNBOUNDED)"));
    assert!(rejected("f() OVER (ROWS CURRENT)"));
}

#[test]
fn over_requires_a_window() {
    assert!(rejected("f() OVER"));
    assert!(rejected("f() OVER (PARTITION a)"));
}

// ORDER BY

fn order_item(sql: &str) -> OrderByItem<'_, Builtin> {
    parse_query::<Core>(sql).unwrap().order_by.remove(0)
}

#[test]
fn nulls_ordering_is_recorded() {
    assert_eq!(
        order_item("SELECT 1 ORDER BY a NULLS FIRST").nulls_first,
        Some(true)
    );
    assert_eq!(
        order_item("SELECT 1 ORDER BY a DESC NULLS LAST").nulls_first,
        Some(false)
    );
    assert_eq!(order_item("SELECT 1 ORDER BY a").nulls_first, None);
}

#[test]
fn nulls_requires_a_position() {
    assert!(parse_query::<Core>("SELECT 1 ORDER BY a NULLS").is_err());
}

// Rendering

fn assert_round_trip<D: Dialect>(sql: &str) {
    assert_eq!(parse_sql::<D>(sql).unwrap()[0].to_string(), sql);
}

#[test]
fn new_expressions_round_trip() {
    let cases = [
        "SELECT NULL",
        "SELECT a IS NULL, b IS NOT NULL",
        "SELECT a IS TRUE, a IS NOT FALSE, a IS UNKNOWN",
        "SELECT a IS DISTINCT FROM b, a IS NOT DISTINCT FROM c",
        "SELECT a LIKE b, a NOT LIKE c",
        "SELECT a LIKE b ESCAPE c",
        "SELECT a BETWEEN 1 AND 2, b NOT BETWEEN 3 AND 4",
        "SELECT CASE WHEN a THEN 1 WHEN b THEN 2 ELSE 3 END",
        "SELECT CASE x WHEN 1 THEN 2 END",
        "SELECT COUNT(DISTINCT a), f(ALL b), g(t.*)",
        "SELECT ARRAY_AGG(a ORDER BY b DESC NULLS LAST, c)",
        "SELECT COUNT(*) FILTER (WHERE a > 1)",
        "SELECT f() OVER w",
        "SELECT f() OVER ()",
        "SELECT f(a) OVER (PARTITION BY b, c ORDER BY d DESC)",
        "SELECT f() OVER (ROWS 1 PRECEDING)",
        "SELECT f() OVER (ORDER BY a ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)",
        "SELECT f() OVER (RANGE BETWEEN 1 FOLLOWING AND UNBOUNDED FOLLOWING)",
        "SELECT f() OVER (GROUPS CURRENT ROW)",
        "SELECT COUNT(DISTINCT a ORDER BY b) FILTER (WHERE c) OVER (PARTITION BY d)",
        "SELECT a FROM t ORDER BY a NULLS FIRST, b ASC NULLS LAST",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn booleans_render_lower_case_like_the_original() {
    let statements = parse_sql::<Core>("SELECT TRUE, False").unwrap();
    assert_eq!(statements[0].to_string(), "SELECT true, false");
}

#[test]
fn ilike_round_trips() {
    assert_round_trip::<Postgres>("SELECT a ILIKE b, c NOT ILIKE d");
}

// Stack safety

fn exercise(sql: &str) {
    let tree = parse_expr::<Core>(sql).unwrap();
    assert!(tree.clone() == tree);
    assert!(!format!("{tree:?}").is_empty());
    assert!(!tree.to_string().is_empty());
}

#[test]
fn tallest_chains_of_new_operators_are_safe_to_use() {
    exercise(&format!("a{}", " IS NULL".repeat(590)));
    exercise(&format!("a{}", " LIKE b".repeat(590)));
    exercise(&format!("a{}", " BETWEEN 1 AND 2".repeat(590)));
}

#[test]
fn huge_chains_of_new_operators_are_rejected() {
    assert!(rejected(&format!("a{}", " IS NULL".repeat(1_000_000))));
    assert!(rejected(&format!("a{}", " LIKE b".repeat(1_000_000))));
    assert!(rejected(&format!(
        "a{}",
        " BETWEEN 1 AND 2".repeat(1_000_000)
    )));
}

#[test]
fn deep_case_expressions_are_rejected() {
    let sql = format!("{}1", "CASE WHEN a THEN ".repeat(100_000));
    assert!(rejected(&sql));
}

#[test]
fn is_chains_to_the_left_of_distinct_from() {
    assert_eq!(
        expr("a IS DISTINCT FROM b IS NULL"),
        is(
            is(col("a"), IsTest::DistinctFrom(Box::new(col("b"))), false),
            IsTest::Null,
            false
        )
    );
}
