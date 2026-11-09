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

//! Snowflake's table syntax and its statements for loading data.

use sqlparser::ast::StatementKind;
use sqlparser::dialect::builtin::BuiltinStatement;
use sqlparser::dialect::{Core, Dialect, Postgres, Snowflake};
use sqlparser::parse_sql;

fn renders(sql: &str) -> String {
    let statements = parse_sql::<Snowflake>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips(sql: &str) {
    assert_eq!(renders(sql), sql);
}

fn rejected(sql: &str) -> bool {
    parse_sql::<Snowflake>(sql).is_err()
}

fn rejected_in<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

// PIVOT and UNPIVOT

#[test]
fn pivot_round_trips() {
    for sql in [
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x', 'y'))",
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x', 'y')) AS p",
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x', 'y')) AS p (c, d) ORDER BY c",
        "SELECT * FROM (SELECT 1) PIVOT(MAX(a) FOR b IN (0, 1))",
        "SELECT * FROM t PIVOT(SUM(a) AS s, AVG(b) AS c FOR (d, e) IN (('x', 1) AS f))",
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN (ANY ORDER BY b))",
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN (ANY))",
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN (SELECT c FROM d))",
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x') DEFAULT ON NULL (0))",
    ] {
        round_trips(sql);
    }
}

#[test]
fn unpivot_round_trips() {
    for sql in [
        "SELECT * FROM t UNPIVOT(a FOR b IN (c, d))",
        "SELECT * FROM t AS s UNPIVOT(a FOR b IN (c, d)) AS u (e, f)",
        "SELECT * FROM t UNPIVOT INCLUDE NULLS (a FOR b IN (c AS x, d AS y))",
        "SELECT * FROM t UNPIVOT EXCLUDE NULLS (a FOR b IN (c, d))",
        "SELECT * FROM t UNPIVOT((a, b) FOR c IN ((d, e) AS f, (g, h) AS i))",
    ] {
        round_trips(sql);
    }
}

#[test]
fn pivots_can_be_stacked() {
    round_trips(
        "SELECT * FROM t AS c UNPIVOT(a FOR b IN (d)) AS u PIVOT(SUM(a) FOR b IN (1)) AS p",
    );
}

#[test]
fn pivot_alone_is_an_alias() {
    round_trips("SELECT * FROM t PIVOT");
    round_trips("SELECT * FROM t UNPIVOT");
}

#[test]
fn pivot_needs_for_and_in() {
    assert!(rejected("SELECT * FROM t PIVOT(SUM(a))"));
    assert!(rejected("SELECT * FROM t PIVOT(SUM(a) FOR b)"));
    assert!(rejected("SELECT * FROM t UNPIVOT(a FOR b)"));
}

#[test]
fn pivot_is_an_alias_elsewhere() {
    assert!(!rejected_in::<Postgres>("SELECT * FROM t pivot"));
    assert!(rejected_in::<Postgres>(
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN (1))"
    ));
}

// Sampling

#[test]
fn sampling_round_trips() {
    for sql in [
        "SELECT * FROM t SAMPLE (10)",
        "SELECT * FROM t TABLESAMPLE (10)",
        "SELECT * FROM t SAMPLE (10 ROWS)",
        "SELECT * FROM t SAMPLE (50 PERCENT)",
        "SELECT * FROM t AS s SAMPLE (50)",
        "SELECT * FROM (SELECT 1) SAMPLE (10) SEED (42)",
        "SELECT * FROM t TABLESAMPLE BERNOULLI (50)",
        "SELECT * FROM t TABLESAMPLE SYSTEM (50) REPEATABLE (10)",
        "SELECT * FROM t SAMPLE 1 / 10",
        "SELECT * FROM t SAMPLE 1 / 10 OFFSET 1 / 2",
    ] {
        round_trips(sql);
    }
}

#[test]
fn a_sample_takes_no_alias() {
    assert!(rejected("SELECT * FROM t SAMPLE (10) AS a"));
}

#[test]
fn sampling_is_not_in_other_dialects() {
    assert!(rejected_in::<Core>("SELECT * FROM t TABLESAMPLE (10)"));
}

// Time travel

#[test]
fn time_travel_round_trips() {
    for sql in [
        "SELECT * FROM t AT(TIMESTAMP => '2024-12-15')",
        "SELECT * FROM t BEFORE(STATEMENT => 'x')",
        "SELECT * FROM t AT(OFFSET => -60) AS a",
        "SELECT a FROM t CHANGES(INFORMATION => DEFAULT) AT(OFFSET => -60)",
        "SELECT a FROM t CHANGES(INFORMATION => APPEND_ONLY) BEFORE(OFFSET => 1) END(OFFSET => 2)",
    ] {
        round_trips(sql);
    }
}

#[test]
fn changes_needs_a_start_point() {
    assert!(rejected("SELECT a FROM t CHANGES(INFORMATION => DEFAULT)"));
    assert!(rejected(
        "SELECT a FROM t CHANGES(INFORMATION => DEFAULT) FOO(x)"
    ));
}

#[test]
fn at_and_before_can_still_be_aliases() {
    round_trips("SELECT * FROM t at");
    round_trips("SELECT * FROM t before");
}

#[test]
fn time_travel_is_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>(
        "SELECT * FROM t AT(TIMESTAMP => 'x')"
    ));
}

// Joins and names

#[test]
fn asof_join_round_trips() {
    round_trips("SELECT * FROM a ASOF JOIN b MATCH_CONDITION (a.x >= b.x)");
    round_trips("SELECT * FROM a AS c ASOF JOIN b AS d MATCH_CONDITION (c.x >= d.x) ON c.y = d.y");
}

#[test]
fn asof_join_needs_its_condition() {
    assert!(rejected("SELECT * FROM a ASOF JOIN b ON c"));
}

#[test]
fn asof_is_an_alias_elsewhere() {
    assert!(!rejected_in::<Postgres>("SELECT * FROM a asof"));
}

#[test]
fn double_dot_names_round_trip() {
    round_trips("SELECT * FROM db..t");
    round_trips("SELECT * FROM db..t AS a");
    assert!(rejected_in::<Postgres>("SELECT * FROM db..t"));
}

#[test]
fn a_name_cannot_end_with_a_dot() {
    assert!(rejected("SELECT * FROM db.."));
}

#[test]
fn named_arguments_can_use_reserved_words() {
    round_trips("SELECT * FROM TABLE(FLATTEN(input => a, outer => true)) AS f");
    round_trips("SELECT f(offset => 1)");
}

// Multi-table INSERT

fn is_snowflake_statement(sql: &str) -> bool {
    matches!(
        parse_sql::<Snowflake>(sql).unwrap().remove(0).kind,
        StatementKind::Ext(BuiltinStatement::Snowflake(_))
    )
}

#[test]
fn unconditional_multi_table_insert_round_trips() {
    for sql in [
        "INSERT ALL INTO t SELECT 1",
        "INSERT ALL INTO t (a, b) SELECT 1, 2",
        "INSERT ALL INTO t VALUES ($1, DEFAULT) SELECT 1",
        "INSERT ALL INTO t (a) VALUES (NULL) INTO u SELECT 1 FROM src",
        "INSERT OVERWRITE ALL INTO t INTO u SELECT 1",
        "INSERT ALL INTO a INTO b INTO c SELECT 1",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql));
    }
}

#[test]
fn conditional_multi_table_insert_round_trips() {
    for sql in [
        "INSERT ALL WHEN a > 1 THEN INTO t SELECT a FROM s",
        "INSERT FIRST WHEN a > 1 THEN INTO t WHEN a > 0 THEN INTO t INTO u ELSE INTO v SELECT a FROM s",
        "INSERT OVERWRITE FIRST WHEN a THEN INTO t ELSE INTO u SELECT a",
    ] {
        round_trips(sql);
    }
}

#[test]
fn multi_table_insert_needs_a_target_and_a_source() {
    assert!(rejected("INSERT ALL SELECT 1"));
    assert!(rejected("INSERT ALL INTO t"));
    assert!(rejected("INSERT FIRST WHEN a INTO t SELECT 1"));
}

#[test]
fn ordinary_insert_is_unchanged() {
    round_trips("INSERT INTO t VALUES (1)");
    assert!(!is_snowflake_statement("INSERT INTO t VALUES (1)"));
}

// Stage commands

#[test]
fn stage_listing_commands_have_one_spelling() {
    round_trips("LIST @~");
    round_trips("LIST @db.s.stage/dir/");
    round_trips("REMOVE @~");
    round_trips("REMOVE @a/b/ PATTERN='.*data.*'");
    assert_eq!(renders("LS @~"), "LIST @~");
    assert_eq!(renders("RM @~"), "REMOVE @~");
    assert!(is_snowflake_statement("LIST @~"));
}

#[test]
fn put_round_trips() {
    round_trips("PUT 'file:///tmp/a.csv' @s");
    round_trips("PUT 'file:///tmp/a.csv' @\"my stage\" PARALLEL=4");
    round_trips("PUT 'file:///tmp/a.csv' @s PARALLEL=8 AUTO_COMPRESS=true SOURCE_COMPRESSION=GZIP");
}

#[test]
fn stage_commands_need_a_stage() {
    assert!(rejected("LIST"));
    assert!(rejected("LIST t"));
    assert!(rejected("PUT 'a' t"));
    assert!(rejected("PUT @s"));
}

#[test]
fn stage_commands_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("LIST @~"));
    assert!(rejected_in::<Core>("REMOVE a"));
}
