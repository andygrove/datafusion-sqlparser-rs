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

//! Less common `SELECT` syntax, including what belongs to specific dialects.

use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift};
use sqlparser::parse_sql;

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

#[test]
fn field_access_round_trips() {
    round_trips_everywhere("SELECT (a.b).c FROM t");
    round_trips_everywhere("SELECT (a).b.c FROM t");
    round_trips_everywhere("SELECT a[1].b FROM t");
}

#[test]
fn field_access_needs_a_composite_on_the_left() {
    assert!(rejected::<Core>("SELECT 1 .a"));
    assert!(rejected::<Core>("SELECT 'x'.a"));
}

#[test]
fn hex_literals_have_one_rendering() {
    round_trips_everywhere("SELECT X'deadBEEF'");
    assert_eq!(renders::<Core>("SELECT 0xdeadBEEF"), "SELECT X'deadBEEF'");
    assert_eq!(renders::<Core>("SELECT x'AB'"), "SELECT X'AB'");
}

#[test]
fn x_can_still_be_a_name() {
    round_trips_everywhere("SELECT x FROM t");
    assert!(rejected::<Core>("SELECT 0xZZ"));
}

#[test]
fn lateral_round_trips() {
    round_trips_everywhere("SELECT * FROM a, LATERAL (SELECT 1) AS b");
    round_trips_everywhere("SELECT * FROM a LEFT JOIN LATERAL f(a.x) ON true");
    round_trips_everywhere("SELECT * FROM lateral");
    assert!(rejected::<Core>(
        "SELECT * FROM a LEFT JOIN LATERAL (b CROSS JOIN c)"
    ));
}

#[test]
fn with_ordinality_round_trips() {
    round_trips_everywhere("SELECT * FROM f(1) WITH ORDINALITY AS t");
}

#[test]
fn null_treatment_and_within_group_round_trip() {
    round_trips_everywhere("SELECT LAG(a) IGNORE NULLS OVER (ORDER BY b) FROM t");
    round_trips_everywhere("SELECT LAG(a) RESPECT NULLS OVER (ORDER BY b) FROM t");
    round_trips_everywhere("SELECT f(a) WITHIN GROUP (ORDER BY b ASC) FROM t");
}

#[test]
fn respect_can_still_be_an_alias() {
    round_trips_everywhere("SELECT f(a) AS respect FROM t");
    assert_eq!(
        renders::<Core>("SELECT f(a) respect FROM t"),
        "SELECT f(a) AS respect FROM t"
    );
}

#[test]
fn at_time_zone_round_trips() {
    round_trips_everywhere("SELECT a AT TIME ZONE 'UTC' FROM t");
    round_trips::<Postgres>("SELECT a::TIMESTAMP AT TIME ZONE 'UTC' FROM t");
}

#[test]
fn at_time_needs_zone() {
    assert!(rejected::<Core>("SELECT a AT TIME b c"));
}

#[test]
fn collate_round_trips() {
    round_trips_everywhere("SELECT a COLLATE de_DE FROM t");
    round_trips::<Postgres>("SELECT a COLLATE \"de_DE\" FROM t");
    round_trips_everywhere("CREATE TABLE t (a TEXT COLLATE de_DE)");
}

#[test]
fn overlaps_round_trips() {
    round_trips_everywhere("SELECT (a, b) OVERLAPS (c, d)");
}

#[test]
fn named_windows_round_trip() {
    round_trips_everywhere("SELECT f() OVER w FROM t WINDOW w AS (PARTITION BY a)");
    round_trips_everywhere("SELECT 1 FROM t WINDOW a AS (ORDER BY b), c AS (ORDER BY d)");
    assert!(rejected::<Core>("SELECT 1 FROM t WINDOW a AS b"));
    round_trips_everywhere("SELECT f() OVER (w ROWS UNBOUNDED PRECEDING) FROM t");
}

#[test]
fn a_window_extending_another_keeps_its_spacing() {
    round_trips_everywhere("SELECT f() OVER (w PARTITION BY a) FROM t");
    round_trips_everywhere("SELECT f() OVER (w ORDER BY a) FROM t");
}

#[test]
fn a_nested_join_can_start_and_end_with_derived_tables() {
    round_trips_everywhere("SELECT * FROM (((SELECT 1) AS a CROSS JOIN (SELECT 2)))");
}

#[test]
fn endless_table_parentheses_are_rejected() {
    let sql = format!("SELECT * FROM {}", "(".repeat(200_000));
    assert!(rejected::<Core>(&sql));
}

#[test]
fn window_clause_needs_a_definition() {
    assert!(rejected::<Core>("SELECT 1 WINDOW"));
}

#[test]
fn array_types_round_trip() {
    for ty in ["INT ARRAY", "INT ARRAY[3]"] {
        round_trips_everywhere(&format!("SELECT CAST(a AS {ty})"));
    }
    for ty in ["INT[]", "INT[3]", "INT[][]"] {
        round_trips::<Postgres>(&format!("SELECT CAST(a AS {ty})"));
        assert!(rejected::<MySql>(&format!("SELECT CAST(a AS {ty})")));
    }
    round_trips::<Postgres>("SELECT a::INT[]");
    round_trips::<Postgres>("CREATE TABLE t (a INT[])");
}

// PostgreSQL

#[test]
fn distinct_on_round_trips_in_postgres() {
    round_trips::<Postgres>("SELECT DISTINCT ON (a, b) c FROM t");
    round_trips::<Postgres>("SELECT DISTINCT a FROM t");
    assert!(rejected::<MySql>("SELECT DISTINCT ON (a) b FROM t"));
    assert!(rejected::<Core>("SELECT DISTINCT ON (a) b FROM t"));
}

#[test]
fn dollar_quoted_strings_round_trip_in_the_postgres_family() {
    for sql in ["SELECT $$a 'b' $c$$", "SELECT $t$a $$ b$t$", "SELECT $$$$"] {
        round_trips::<Postgres>(sql);
        round_trips::<Redshift>(sql);
    }
    assert!(rejected::<Postgres>("SELECT $$a"));
    assert!(rejected::<Core>("SELECT $$a$$"));
}

#[test]
fn dollar_numbers_are_still_placeholders_in_postgres() {
    round_trips::<Postgres>("SELECT $1, $2");
    assert!(rejected::<Postgres>("SELECT $1$x$1$"));
}

#[test]
fn grouping_extensions_round_trip_in_postgres() {
    round_trips::<Postgres>("SELECT 1 FROM t GROUP BY ROLLUP (a, (b, c))");
    round_trips::<Postgres>("SELECT 1 FROM t GROUP BY CUBE (a, b)");
    round_trips::<Postgres>("SELECT 1 FROM t GROUP BY GROUPING SETS ((a, b), (c), ())");
    assert_eq!(
        renders::<Postgres>("SELECT 1 FROM t GROUP BY GROUPING SETS ((a), b)"),
        "SELECT 1 FROM t GROUP BY GROUPING SETS ((a), (b))"
    );
    assert!(rejected::<MySql>(
        "SELECT 1 FROM t GROUP BY GROUPING SETS ((a))"
    ));
}

#[test]
fn notnull_and_isnull_are_postfix_in_postgres() {
    assert_eq!(
        renders::<Postgres>("SELECT a NOTNULL"),
        "SELECT a IS NOT NULL"
    );
    assert_eq!(renders::<Postgres>("SELECT a ISNULL"), "SELECT a IS NULL");
    assert_eq!(renders::<MySql>("SELECT a NOTNULL"), "SELECT a AS NOTNULL");
}

// MySQL

#[test]
fn mysql_select_modifiers_follow_the_quantifier() {
    round_trips::<MySql>("SELECT HIGH_PRIORITY a FROM t");
    round_trips::<MySql>("SELECT DISTINCT HIGH_PRIORITY STRAIGHT_JOIN a FROM t");
    round_trips::<MySql>("SELECT SQL_SMALL_RESULT SQL_BIG_RESULT SQL_BUFFER_RESULT a");
    round_trips::<MySql>("SELECT SQL_NO_CACHE SQL_CALC_FOUND_ROWS a");
    assert_eq!(
        renders::<MySql>("SELECT SQL_NO_CACHE DISTINCT HIGH_PRIORITY a"),
        "SELECT DISTINCT HIGH_PRIORITY SQL_NO_CACHE a"
    );
}

#[test]
fn distinctrow_means_distinct() {
    assert_eq!(
        renders::<MySql>("SELECT DISTINCTROW a"),
        "SELECT DISTINCT a"
    );
}

#[test]
fn mysql_rejects_two_quantifiers() {
    assert!(rejected::<MySql>("SELECT DISTINCT DISTINCTROW a"));
    assert!(rejected::<MySql>("SELECT ALL DISTINCT a"));
}

#[test]
fn mysql_modifiers_are_not_elsewhere() {
    assert_eq!(
        renders::<Postgres>("SELECT HIGH_PRIORITY a FROM t"),
        "SELECT HIGH_PRIORITY AS a FROM t"
    );
}

#[test]
fn match_against_round_trips_in_mysql() {
    for modifier in [
        "",
        " IN BOOLEAN MODE",
        " IN NATURAL LANGUAGE MODE",
        " IN NATURAL LANGUAGE MODE WITH QUERY EXPANSION",
        " WITH QUERY EXPANSION",
    ] {
        round_trips::<MySql>(&format!("SELECT MATCH (a, t.b) AGAINST ('x'{modifier})"));
    }
    assert!(rejected::<Postgres>("SELECT MATCH (a) AGAINST ('x')"));
}

#[test]
fn match_without_against_is_an_error() {
    assert!(rejected::<MySql>("SELECT MATCH (a)"));
}

#[test]
fn regexp_and_rlike_round_trip_in_mysql() {
    round_trips::<MySql>("SELECT a REGEXP 'b'");
    round_trips::<MySql>("SELECT a NOT REGEXP 'b'");
    round_trips::<MySql>("SELECT a RLIKE 'b'");
    round_trips::<MySql>("SELECT a NOT RLIKE 'b'");
    assert_eq!(renders::<Postgres>("SELECT a REGEXP"), "SELECT a AS REGEXP");
}

#[test]
fn xor_binds_between_and_or_in_mysql() {
    use sqlparser::ast::{BinaryOp, ExprKind};
    let top = |sql| match sqlparser::parse_expr::<MySql>(sql).unwrap().kind {
        ExprKind::Binary { op, .. } => op,
        other => panic!("{other:?}"),
    };
    assert_eq!(top("a XOR b AND c"), BinaryOp::Named("XOR"));
    assert_eq!(top("a OR b XOR c"), BinaryOp::Or);
    round_trips::<MySql>("SELECT a XOR b");
    assert_eq!(renders::<Postgres>("SELECT a XOR"), "SELECT a AS XOR");
}
