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

//! Expression and query syntax beyond the basic `SELECT`.

mod common;

use common::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift};
use sqlparser::{parse_expr, parse_sql};

fn renders<D: Dialect>(sql: &str) -> String {
    let statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips<D: Dialect>(sql: &str) {
    assert_eq!(renders::<D>(sql), sql);
}

fn round_trips_everywhere(sql: &str) {
    round_trips::<Core>(sql);
    round_trips::<MySql>(sql);
    round_trips::<Postgres>(sql);
    round_trips::<Redshift>(sql);
}

fn rejected<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

fn binary_op<D: Dialect<Ext = Builtin>>(sql: &str) -> BinaryOp<'_> {
    match parse_expr::<D>(sql).unwrap().kind {
        ExprKind::Binary { op, .. } => op,
        other => panic!("expected a binary expression, found {other:?}"),
    }
}

// Numbers

#[test]
fn numbers_can_start_with_a_point() {
    round_trips_everywhere("SELECT .5");
    round_trips_everywhere("SELECT -.0");
    round_trips_everywhere("SELECT +.5");
}

#[test]
fn numbers_can_end_with_a_point() {
    round_trips_everywhere("SELECT 1.");
    round_trips_everywhere("SELECT 1. + 2");
}

#[test]
fn a_point_after_a_name_is_still_qualification() {
    assert_eq!(
        parse_expr::<Core>("t.a").unwrap(),
        ExprKind::Compound(object_name(&["t", "a"])).into()
    );
    assert!(parse_expr::<Core>("1.5.5").is_err());
}

// Operators

#[test]
fn core_operators_round_trip() {
    for op in ["||", "|", "&", "^"] {
        round_trips_everywhere(&format!("SELECT a {op} b"));
    }
}

#[test]
fn shifts_are_not_in_the_core() {
    for op in ["<<", ">>"] {
        let sql = format!("SELECT a {op} b");
        assert!(rejected::<Core>(&sql), "{sql}");
        round_trips::<MySql>(&sql);
        round_trips::<Postgres>(&sql);
        round_trips::<Redshift>(&sql);
    }
}

#[test]
fn partition_is_not_a_table_alias() {
    assert!(rejected::<Core>("SELECT * FROM t PARTITION (p0)"));
    assert!(rejected::<Postgres>("SELECT * FROM t partition"));
}

#[test]
fn concatenation_binds_like_multiplication_in_the_core() {
    let ExprKind::Binary { op, right, .. } = parse_expr::<Core>("a + b || c").unwrap().kind else {
        panic!("expected a binary expression");
    };
    assert_eq!(op, BinaryOp::Add);
    assert!(matches!(
        right.kind,
        ExprKind::Binary {
            op: BinaryOp::StringConcat,
            ..
        }
    ));
}

#[test]
fn concatenation_binds_looser_than_addition_in_postgres() {
    assert_eq!(binary_op::<Postgres>("a + b || c"), BinaryOp::StringConcat);
    assert_eq!(binary_op::<Redshift>("a + b || c"), BinaryOp::StringConcat);
}

#[test]
fn bitwise_and_binds_tighter_than_bitwise_or_in_the_core() {
    assert_eq!(binary_op::<Core>("a | b & c"), BinaryOp::BitOr);
    assert_eq!(binary_op::<Core>("a & b | c"), BinaryOp::BitOr);
}

#[test]
fn bitwise_operators_bind_looser_than_addition() {
    assert_eq!(binary_op::<Core>("a + b | c"), BinaryOp::BitOr);
    assert_eq!(binary_op::<Core>("a | b + c"), BinaryOp::BitOr);
}

#[test]
fn bitwise_operators_bind_tighter_than_comparison() {
    assert_eq!(binary_op::<Core>("a = b | c"), BinaryOp::Eq);
}

#[test]
fn postgres_regex_operators_round_trip() {
    for op in ["~", "~*", "!~", "!~*", "~~", "~~*", "!~~", "!~~*"] {
        round_trips::<Postgres>(&format!("SELECT a {op} b"));
    }
}

#[test]
fn postgres_regex_operators_compare() {
    assert_eq!(binary_op::<Postgres>("a ~ b"), BinaryOp::Named("~"));
    assert_eq!(binary_op::<Postgres>("a + b ~ c"), BinaryOp::Named("~"));
}

#[test]
fn postgres_json_operators_round_trip() {
    for op in ["->", "->>", "#>", "#>>", "@>", "<@", "?", "?|", "?&", "&&"] {
        round_trips::<Postgres>(&format!("SELECT a {op} b"));
    }
}

#[test]
fn postgres_operators_are_not_core_syntax() {
    assert!(rejected::<Core>("SELECT a ~ b"));
    assert!(rejected::<MySql>("SELECT a ~ b"));
    assert!(rejected::<Core>("SELECT a -> b"));
    assert!(rejected::<Core>("SELECT a @> b"));
}

#[test]
fn mysql_has_null_safe_equals_and_json_arrows() {
    round_trips::<MySql>("SELECT a <=> b");
    round_trips::<MySql>("SELECT a -> '$.b'");
    round_trips::<MySql>("SELECT a ->> '$.b'");
    assert!(rejected::<Core>("SELECT a <=> b"));
    assert!(rejected::<Postgres>("SELECT a <=> b"));
}

#[test]
fn comments_still_win_over_arrows() {
    assert_eq!(renders::<Postgres>("SELECT 1 --> 2"), "SELECT 1");
}

#[test]
fn a_cast_is_not_a_placeholder() {
    assert!(rejected::<Core>("SELECT a::int"));
    assert!(rejected::<MySql>("SELECT a::int"));
}

// Placeholders

#[test]
fn numbered_and_named_placeholders_round_trip() {
    round_trips_everywhere("SELECT $1, $2");
    round_trips_everywhere("SELECT :name");
    round_trips_everywhere("SELECT * FROM t WHERE a = $1");
}

#[test]
fn placeholders_are_their_own_node() {
    assert_eq!(
        parse_expr::<Core>("$1").unwrap(),
        sqlparser::ast::ExprKind::<Builtin>::Placeholder("$1").into()
    );
    assert_eq!(
        parse_expr::<Core>(":foo").unwrap(),
        sqlparser::ast::ExprKind::<Builtin>::Placeholder(":foo").into()
    );
}

#[test]
fn a_colon_alone_is_not_a_placeholder() {
    assert!(rejected::<Postgres>("SELECT : a"));
}

#[test]
fn question_mark_is_a_placeholder_outside_postgres() {
    round_trips::<Core>("SELECT ?");
    round_trips::<MySql>("SELECT * FROM t WHERE a = ? LIMIT ?");
    round_trips::<Core>("SELECT ?1");
    assert!(rejected::<Postgres>("SELECT ?"));
    assert!(rejected::<Redshift>("SELECT ?"));
}

#[test]
fn mysql_user_variables_are_names() {
    round_trips::<MySql>("SELECT @a");
    round_trips::<MySql>("SELECT @@version");
    assert!(rejected::<Core>("SELECT @a"));
}

// Intervals and typed literals

#[test]
fn intervals_with_qualifiers_round_trip() {
    for sql in [
        "SELECT INTERVAL '1' DAY",
        "SELECT INTERVAL 1 DAY",
        "SELECT INTERVAL '1' DAY TO HOUR",
        "SELECT INTERVAL '1' DAY (2)",
        "SELECT INTERVAL '1' DAY (2) TO SECOND (3)",
        "SELECT INTERVAL '1' SECOND (1, 2)",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn interval_units_are_upper_cased() {
    assert_eq!(
        renders::<Core>("SELECT interval '1' day to hour"),
        "SELECT INTERVAL '1' DAY TO HOUR"
    );
}

#[test]
fn interval_values_are_whole_expressions_when_a_unit_is_required() {
    round_trips::<Core>("SELECT INTERVAL 1 + 1 DAY");
    round_trips::<MySql>("SELECT INTERVAL '1' + '2' DAY");
}

#[test]
fn the_core_requires_an_interval_unit() {
    assert!(rejected::<Core>("SELECT INTERVAL '1'"));
    assert!(rejected::<MySql>("SELECT INTERVAL '1'"));
}

#[test]
fn postgres_intervals_take_a_single_value_and_an_optional_unit() {
    round_trips::<Postgres>("SELECT INTERVAL '1 day'");
    round_trips::<Redshift>("SELECT INTERVAL '1 day'");
    assert_eq!(
        renders::<Postgres>("SELECT INTERVAL 1 + 1 DAY"),
        "SELECT INTERVAL 1 + 1 AS DAY"
    );
}

#[test]
fn deeply_nested_intervals_are_an_error() {
    let sql = format!("SELECT {}'1'", "INTERVAL ".repeat(5000));
    assert!(rejected::<Postgres>(&sql));
}

#[test]
fn typed_literals_round_trip() {
    round_trips_everywhere("SELECT DATE '2020-01-01'");
    round_trips_everywhere("SELECT TIME '10:00:00'");
    round_trips_everywhere("SELECT TIMESTAMP '2020-01-01 10:00:00'");
}

#[test]
fn type_names_can_still_be_columns() {
    round_trips_everywhere("SELECT date, time, timestamp, interval FROM t");
}

#[test]
fn an_arbitrary_name_does_not_start_a_typed_literal() {
    assert!(rejected::<Core>("SELECT foo 'bar'"));
}

// Special function forms

#[test]
fn extract_round_trips_and_upper_cases_the_field() {
    round_trips_everywhere("SELECT EXTRACT(YEAR FROM d)");
    assert_eq!(
        renders::<Core>("SELECT extract(year from d)"),
        "SELECT EXTRACT(YEAR FROM d)"
    );
}

#[test]
fn extract_needs_a_known_field() {
    assert!(rejected::<Core>("SELECT EXTRACT(nonsense FROM d)"));
}

#[test]
fn ceil_and_floor_take_a_unit() {
    round_trips_everywhere("SELECT CEIL(d TO DAY)");
    round_trips_everywhere("SELECT FLOOR(d TO HOUR) FROM t");
    round_trips_everywhere("SELECT CEIL(d)");
}

#[test]
fn position_round_trips() {
    round_trips_everywhere("SELECT POSITION('a' IN b)");
    round_trips_everywhere("SELECT POSITION('a' IN b || c)");
}

#[test]
fn substring_keeps_how_it_was_written() {
    round_trips_everywhere("SELECT SUBSTRING(a FROM 1 FOR 2)");
    round_trips_everywhere("SELECT SUBSTRING(a FROM 1)");
    round_trips_everywhere("SELECT SUBSTRING(a FOR 2)");
    round_trips_everywhere("SELECT SUBSTRING(a, 1, 2)");
    round_trips_everywhere("SELECT SUBSTR(a FROM 1)");
}

#[test]
fn trim_round_trips() {
    round_trips_everywhere("SELECT TRIM(BOTH 'x' FROM a)");
    round_trips_everywhere("SELECT TRIM(LEADING 'x' FROM a)");
    round_trips_everywhere("SELECT TRIM(TRAILING a)");
    round_trips_everywhere("SELECT TRIM('x' FROM a)");
    round_trips_everywhere("SELECT TRIM(a)");
}

#[test]
fn overlay_round_trips() {
    round_trips_everywhere("SELECT OVERLAY(a PLACING b FROM 1 FOR 2)");
    round_trips_everywhere("SELECT OVERLAY(a PLACING b FROM 1)");
}

#[test]
fn those_names_are_ordinary_functions_without_their_keywords() {
    for name in [
        "extract",
        "position",
        "substring",
        "trim",
        "overlay",
        "ceil",
    ] {
        let sql = format!("SELECT {name}(a, b)");
        round_trips_everywhere(&sql);
    }
}

#[test]
fn those_names_keep_call_options_without_their_keywords() {
    round_trips::<Postgres>("SELECT ceil(a) OVER (ORDER BY b)");
}

#[test]
fn those_names_can_still_be_columns() {
    round_trips_everywhere("SELECT extract, position, trim FROM t");
}

#[test]
fn a_keyword_inside_a_nested_call_does_not_count() {
    round_trips_everywhere("SELECT trim(f((SELECT a FROM t)), b)");
}

#[test]
fn try_cast_round_trips() {
    round_trips_everywhere("SELECT TRY_CAST(a AS BIGINT)");
}

#[test]
fn postgres_interval_types_name_their_fields() {
    for sql in [
        "SELECT CAST(a AS INTERVAL)",
        "SELECT CAST(a AS INTERVAL YEAR)",
        "SELECT CAST(a AS INTERVAL DAY TO HOUR)",
        "SELECT CAST(a AS INTERVAL(3))",
        "SELECT CAST(a AS INTERVAL SECOND(3))",
        "SELECT CAST(a AS INTERVAL DAY TO SECOND(3))",
    ] {
        round_trips::<Postgres>(sql);
    }
}

#[test]
fn interval_types_are_otherwise_plain() {
    assert!(rejected::<Core>("SELECT CAST(a AS INTERVAL(3))"));
    assert!(rejected::<Core>("SELECT CAST(a AS INTERVAL DAY TO HOUR)"));
}

#[test]
fn interval_type_ranges_must_run_from_larger_to_smaller_units() {
    round_trips::<Postgres>("SELECT CAST(a AS INTERVAL DAY TO HOUR(3))");
    assert!(rejected::<Postgres>(
        "SELECT CAST(a AS INTERVAL YEAR TO DAY)"
    ));
    assert!(rejected::<Postgres>(
        "SELECT CAST(a AS INTERVAL HOUR TO DAY)"
    ));
}

#[test]
fn casts_take_full_data_types() {
    round_trips_everywhere("SELECT CAST(a AS DECIMAL(10,2))");
    round_trips_everywhere("SELECT CAST(a AS DOUBLE PRECISION)");
    round_trips::<Postgres>("SELECT a::VARCHAR(10)");
    round_trips::<Postgres>("SELECT a::TIMESTAMP WITH TIME ZONE");
}

// Quantified comparisons, SIMILAR TO

#[test]
fn any_some_and_all_round_trip() {
    round_trips_everywhere("SELECT a = ANY(b)");
    round_trips_everywhere("SELECT a <> SOME(b)");
    round_trips_everywhere("SELECT a >= ALL(SELECT c FROM t)");
    round_trips_everywhere("SELECT a = ANY(SELECT c FROM t)");
}

#[test]
fn postgres_operators_take_quantifiers() {
    round_trips::<Postgres>("SELECT a ~ ANY(b)");
}

#[test]
fn any_can_still_be_a_column() {
    round_trips_everywhere("SELECT any FROM t");
    round_trips_everywhere("SELECT a = any FROM t");
}

#[test]
fn all_needs_parentheses() {
    assert!(rejected::<Core>("SELECT a = ALL b"));
}

#[test]
fn similar_to_round_trips() {
    round_trips_everywhere("SELECT a SIMILAR TO b");
    round_trips_everywhere("SELECT a NOT SIMILAR TO b ESCAPE 'x'");
}

// Arrays, tuples, named arguments

#[test]
fn array_literals_round_trip() {
    round_trips_everywhere("SELECT ARRAY[1, 2]");
    round_trips_everywhere("SELECT ARRAY[]");
    round_trips_everywhere("SELECT ARRAY[ARRAY[1], ARRAY[2]]");
}

#[test]
fn subscripts_round_trip() {
    round_trips_everywhere("SELECT a[1]");
    round_trips_everywhere("SELECT a[1][2]");
    round_trips_everywhere("SELECT t.a[b + 1]");
}

#[test]
fn tuples_round_trip() {
    round_trips_everywhere("SELECT (a, b)");
    round_trips_everywhere("SELECT * FROM t WHERE (a, b) IN (SELECT 1, 2)");
}

#[test]
fn a_single_parenthesized_expression_is_not_a_tuple() {
    assert_eq!(
        parse_expr::<Core>("(a)").unwrap(),
        parens::<Builtin>(col("a"))
    );
}

#[test]
fn named_arguments_round_trip() {
    round_trips_everywhere("SELECT f(a => 1, b => 'x')");
}

// VALUES

#[test]
fn values_is_a_query() {
    round_trips_everywhere("VALUES (1), (2)");
    round_trips_everywhere("VALUES (1, 'a') ORDER BY 1");
    round_trips_everywhere("VALUES (1) UNION VALUES (2)");
}

#[test]
fn values_works_in_from_with_column_aliases() {
    round_trips_everywhere("SELECT * FROM (VALUES (1), (2))");
    round_trips_everywhere("SELECT * FROM (VALUES (1, 2)) AS t (a, b)");
}

#[test]
fn values_can_still_be_a_column() {
    round_trips_everywhere("SELECT values FROM t");
    round_trips_everywhere("SELECT (values) FROM t");
}

#[test]
fn tables_can_alias_their_columns() {
    round_trips_everywhere("SELECT * FROM t AS a (x, y)");
    round_trips_everywhere("SELECT * FROM t a (x, y)");
}

// OFFSET, FETCH and locking

#[test]
fn offset_does_not_need_limit() {
    round_trips_everywhere("SELECT 1 OFFSET 2");
    round_trips_everywhere("SELECT 1 OFFSET 2 ROW");
    round_trips_everywhere("SELECT 1 OFFSET 2 ROWS");
    round_trips_everywhere("SELECT 1 LIMIT 3 OFFSET 2 ROWS");
}

#[test]
fn offset_can_come_before_limit() {
    assert_eq!(
        renders::<Core>("SELECT 1 OFFSET 2 LIMIT 3"),
        "SELECT 1 LIMIT 3 OFFSET 2"
    );
}

#[test]
fn fetch_has_one_canonical_form() {
    round_trips_everywhere("SELECT 1 FETCH FIRST 2 ROWS ONLY");
    round_trips_everywhere("SELECT 1 FETCH FIRST ROWS ONLY");
    round_trips_everywhere("SELECT 1 FETCH FIRST 10 PERCENT ROWS WITH TIES");
    assert_eq!(
        renders::<Core>("SELECT 1 FETCH NEXT 2 ROW ONLY"),
        "SELECT 1 FETCH FIRST 2 ROWS ONLY"
    );
}

#[test]
fn offset_and_fetch_combine() {
    round_trips_everywhere("SELECT 1 ORDER BY 1 OFFSET 2 ROWS FETCH FIRST 3 ROWS ONLY");
}

#[test]
fn locking_clauses_round_trip() {
    round_trips_everywhere("SELECT 1 FOR UPDATE");
    round_trips_everywhere("SELECT 1 FOR SHARE");
    round_trips_everywhere("SELECT 1 FOR UPDATE OF t NOWAIT");
    round_trips_everywhere("SELECT 1 FOR SHARE OF t SKIP LOCKED");
    round_trips_everywhere("SELECT 1 FOR SHARE OF a FOR UPDATE OF b");
}

#[test]
fn for_alone_is_still_an_error() {
    assert!(rejected::<Core>("SELECT 1 FOR"));
}

// Tables

#[test]
fn parenthesized_joins_round_trip() {
    round_trips_everywhere("SELECT * FROM (a JOIN b ON x)");
    round_trips_everywhere("SELECT * FROM ((a NATURAL JOIN b))");
    round_trips_everywhere("SELECT * FROM (a JOIN b ON x) AS c");
    round_trips_everywhere("SELECT * FROM (a AS alias1 NATURAL JOIN b AS c)");
}

#[test]
fn subqueries_can_lead_a_parenthesized_join() {
    round_trips_everywhere("SELECT * FROM ((SELECT 1) AS x JOIN y ON z)");
    round_trips_everywhere("SELECT * FROM (((SELECT 1) UNION (SELECT 2)) AS t1 NATURAL JOIN t2)");
}

#[test]
fn parenthesized_queries_stay_subqueries() {
    let statements = parse_sql::<Core>("SELECT * FROM ((SELECT 1))").unwrap();
    let StatementKind::Query(query) = &statements[0].kind else {
        panic!("expected a query");
    };
    assert!(matches!(
        body(query).from[0].relation.factor,
        TableFactor::Derived(_)
    ));
    round_trips_everywhere("SELECT * FROM ((SELECT 1) UNION (SELECT 2))");
    round_trips_everywhere("SELECT * FROM ((SELECT 1) LIMIT 1)");
}

#[test]
fn deeply_parenthesized_tables_are_an_error() {
    let depth = 5000;
    let sql = format!("SELECT * FROM {}a{}", "(".repeat(depth), ")".repeat(depth));
    assert!(rejected::<Core>(&sql));
}

#[test]
fn parentheses_must_group_a_join() {
    assert!(rejected::<Core>("SELECT * FROM (a)"));
    assert!(rejected::<Core>("SELECT * FROM (a b) c"));
    assert!(rejected::<Core>("SELECT * FROM ((a))"));
    assert!(rejected::<Core>("SELECT * FROM (a NATURAL JOIN (b))"));
    assert!(rejected::<Core>("SELECT * FROM ((SELECT 1) AS t)"));
}

#[test]
fn table_functions_round_trip() {
    round_trips_everywhere("SELECT * FROM generate_series(1, 2)");
    round_trips_everywhere("SELECT * FROM generate_series(1, 2) AS s");
}

#[test]
fn common_nodes_stay_small() {
    use std::mem::size_of;
    assert!(
        size_of::<Expr<'_, Builtin>>() <= 72,
        "{}",
        size_of::<Expr<'_, Builtin>>()
    );
    assert!(
        size_of::<Query<'_, Builtin>>() <= 160,
        "{}",
        size_of::<Query<'_, Builtin>>()
    );
}
