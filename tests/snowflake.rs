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

//! Snowflake's own syntax.

use sqlparser::ast::{BinaryOp, Expr, ExprKind, SetExpr, SetOperator, StatementKind};
use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement};
use sqlparser::dialect::snowflake::{SnowflakeExpr, SnowflakeStatement};
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Snowflake};
use sqlparser::{parse_expr, parse_sql};

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

// Lexing

#[test]
fn identifiers_can_contain_dollar_signs() {
    round_trips("SELECT a$b, _c$ FROM t$1");
    assert!(rejected_in::<Core>("SELECT a$b$c d e"));
}

#[test]
fn identifiers_are_ascii() {
    assert!(rejected("SELECT 説明 FROM t"));
    assert!(rejected("SELECT a説明 FROM t"));
    round_trips("SELECT \"説明\" FROM t");
}

#[test]
fn hex_strings_still_lex() {
    round_trips("SELECT X'AB'");
}

#[test]
fn line_comments_can_start_with_hash_or_slashes() {
    assert_eq!(renders("SELECT 1 # c\n"), "SELECT 1");
    assert_eq!(renders("SELECT 1 // c\n"), "SELECT 1");
    assert_eq!(renders("SELECT 1 -- c\n"), "SELECT 1");
}

#[test]
fn division_is_not_a_comment() {
    round_trips("SELECT a / b");
}

#[test]
fn block_comments_do_not_nest() {
    assert!(rejected("SELECT /* a /* b */ c */ 1"));
    assert_eq!(renders("SELECT /* a /* b */ 1"), "SELECT 1");
}

#[test]
fn backslashes_escape_in_strings() {
    round_trips(r"SELECT 'it\'s'");
}

#[test]
fn dollar_quoted_strings_round_trip() {
    round_trips("SELECT $$a 'b'$$");
    round_trips("SELECT $t$a$t$");
}

#[test]
fn placeholders_still_work() {
    round_trips("SELECT $1, :name, :1");
}

// Expressions

#[test]
fn intervals_take_a_single_value_and_an_optional_unit() {
    round_trips("SELECT INTERVAL '1 day'");
    assert_eq!(
        renders("SELECT INTERVAL 1 + 1 DAY"),
        "SELECT INTERVAL 1 + 1 AS DAY"
    );
}

#[test]
fn connect_by_root_is_not_read_as_a_column() {
    round_trips("SELECT CONNECT_BY_ROOT name FROM t");
    assert!(!rejected_in::<Postgres>(
        "SELECT CONNECT_BY_ROOT name FROM t"
    ));
}

#[test]
fn casts_ilike_and_regexp_round_trip() {
    round_trips("SELECT a::VARCHAR(10)");
    round_trips("SELECT a ILIKE b, c NOT ILIKE d");
    round_trips("SELECT a REGEXP b, c NOT RLIKE d");
}

#[test]
fn paths_round_trip() {
    for sql in [
        "SELECT a:b",
        "SELECT a:b.c",
        "SELECT a:\"B\"",
        "SELECT a:b.\"C\".d",
        "SELECT a:b[0].c",
        "SELECT a:b:c",
        "SELECT $1:b",
        "SELECT t.a:b FROM t",
        "SELECT a[0]:b",
    ] {
        round_trips(sql);
    }
}

#[test]
fn paths_bind_tighter_than_casts_and_arithmetic() {
    round_trips("SELECT a:b::INT");
    round_trips("SELECT a:b + 1");
    round_trips("SELECT 1 + a:b");
}

#[test]
fn a_path_is_a_dialect_node() {
    assert!(matches!(
        parse_expr::<Snowflake>("a:b").unwrap().kind,
        sqlparser::ast::ExprKind::<Builtin>::Ext(_)
    ));
}

fn path_len(expr: &Expr<'_, Builtin>) -> Option<usize> {
    match &expr.kind {
        ExprKind::Ext(BuiltinExpr::Snowflake(SnowflakeExpr::Path { path, .. })) => Some(path.len()),
        _ => None,
    }
}

#[test]
fn a_chain_of_keys_is_one_path() {
    assert_eq!(
        path_len(&parse_expr::<Snowflake>("a:b:c.d").unwrap()),
        Some(3)
    );
}

#[test]
fn a_path_binds_tighter_than_arithmetic() {
    let ExprKind::Binary { op, right, .. } = parse_expr::<Snowflake>("1 + a:b").unwrap().kind
    else {
        panic!("expected a binary expression");
    };
    assert_eq!(op, BinaryOp::Add);
    assert_eq!(path_len(&right), Some(1));
}

#[test]
fn a_cast_applies_to_the_whole_path() {
    let ExprKind::Cast { expr, .. } = parse_expr::<Snowflake>("a:b::INT").unwrap().kind else {
        panic!("expected a cast");
    };
    assert_eq!(path_len(&expr), Some(1));
}

#[test]
fn a_number_after_a_value_is_not_a_key() {
    assert!(rejected("SELECT a :1"));
}

#[test]
fn a_colon_needs_a_key() {
    assert!(rejected("SELECT a:"));
    assert!(rejected("SELECT a: 'b'"));
}

#[test]
fn paths_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("SELECT a:b"));
    assert!(rejected_in::<Core>("SELECT a:b"));
    assert!(rejected_in::<MySql>("SELECT a:b"));
}

// Queries

#[test]
fn stages_can_be_queried() {
    round_trips("SELECT $1, $2 FROM @mystage1");
    round_trips("SELECT * FROM @stage/day=18/23.parquet");
    round_trips("SELECT * FROM @stage/0:18:23/23.parquet");
    round_trips("SELECT * FROM @~/path");
    round_trips("SELECT * FROM @ns.%t/path");
    round_trips("SELECT $1 FROM @mystage1(file_format => 'myformat')");
    round_trips("SELECT * FROM @a AS b, @c");
    round_trips("SELECT * FROM @\"my stage\"/a");
}

#[test]
fn stages_are_only_valid_as_tables() {
    assert!(rejected("SELECT @a"));
    assert!(rejected("SELECT 1 @a"));
    assert!(rejected("SELECT * FROM t @a"));
    assert!(rejected("SELECT a & @b"));
}

#[test]
fn stages_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("SELECT 1 FROM @a"));
}

#[test]
fn trailing_commas_are_allowed_in_lists() {
    assert_eq!(renders("SELECT 1, 2, FROM t"), "SELECT 1, 2 FROM t");
    assert_eq!(renders("SELECT a FROM b, c,"), "SELECT a FROM b, c");
    assert_eq!(
        renders("SELECT a FROM (SELECT b, FROM c,) WHERE d"),
        "SELECT a FROM (SELECT b FROM c) WHERE d"
    );
    assert_eq!(renders("SELECT a, ORDER BY a"), "SELECT a ORDER BY a");
    assert_eq!(renders("SELECT a, ;"), "SELECT a");
    assert_eq!(renders("SELECT 1, FETCH"), "SELECT 1, FETCH");
    assert_eq!(
        renders("SELECT a FROM b, HAVING COUNT(*) > 1"),
        "SELECT a FROM b HAVING COUNT(*) > 1"
    );
}

#[test]
fn trailing_commas_stay_errors_elsewhere() {
    assert!(rejected_in::<Core>("SELECT 1, FROM t"));
    assert!(rejected_in::<Postgres>("SELECT a FROM b,"));
}

#[test]
fn a_list_still_needs_an_item_after_a_comma_in_the_middle() {
    assert!(rejected("SELECT 1,, 2"));
}

#[test]
fn wildcard_options_round_trip() {
    for sql in [
        "SELECT * EXCLUDE (a, b) FROM t",
        "SELECT * EXCLUDE a FROM t",
        "SELECT t.* EXCLUDE a FROM t",
        "SELECT * RENAME a AS b FROM t",
        "SELECT * RENAME (a AS b, c AS d) FROM t",
        "SELECT * ILIKE '%a%' FROM t",
        "SELECT * EXCLUDE a RENAME b AS c FROM t",
        "SELECT * REPLACE (a + 1 AS a) FROM t",
    ] {
        round_trips(sql);
    }
}

#[test]
fn a_plain_wildcard_stays_plain() {
    round_trips("SELECT *, a FROM t");
}

#[test]
fn wildcard_options_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("SELECT * EXCLUDE (a) FROM t"));
}

#[test]
fn qualify_round_trips() {
    round_trips("SELECT a FROM t QUALIFY ROW_NUMBER() OVER (ORDER BY b) = 1");
    round_trips("SELECT a FROM t HAVING b QUALIFY c");
    round_trips("SELECT a FROM t QUALIFY b WINDOW w AS (ORDER BY c)");
    assert!(rejected_in::<Postgres>("SELECT a FROM t QUALIFY b c"));
}

#[test]
fn minus_is_a_set_operator() {
    round_trips("SELECT 1 MINUS SELECT 2");
    round_trips("SELECT 1 MINUS ALL SELECT 2");
    assert!(rejected("SELECT * FROM a minus JOIN b ON c"));
    round_trips("SELECT * FROM a AS minus JOIN b ON c");
}

#[test]
fn minus_binds_like_except() {
    let statement = parse_sql::<Snowflake>("SELECT 1 MINUS SELECT 2 INTERSECT SELECT 3")
        .unwrap()
        .remove(0);
    let StatementKind::Query(query) = statement.kind else {
        panic!("expected a query");
    };
    assert!(matches!(
        query.body,
        SetExpr::SetOperation {
            op: SetOperator::Minus,
            ..
        }
    ));
}

#[test]
fn minus_is_an_alias_elsewhere() {
    assert_eq!(
        parse_sql::<Postgres>("SELECT * FROM a minus").unwrap()[0].to_string(),
        "SELECT * FROM a minus"
    );
}

// Statements

fn is_snowflake_statement(sql: &str) -> bool {
    matches!(
        parse_sql::<Snowflake>(sql).unwrap().remove(0).kind,
        StatementKind::Ext(BuiltinStatement::Snowflake(_))
    )
}

#[test]
fn use_round_trips() {
    for sql in [
        "USE mydb",
        "USE \"mydb\"",
        "USE 'mydb'",
        "USE a.b",
        "USE DATABASE a",
        "USE SCHEMA a.b",
        "USE ROLE \"r\"",
        "USE WAREHOUSE w",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql));
    }
}

#[test]
fn use_keeps_its_object_kind() {
    let statement = parse_sql::<Snowflake>("USE ROLE r").unwrap().remove(0);
    let StatementKind::Ext(BuiltinStatement::Snowflake(SnowflakeStatement::Use { kind, .. })) =
        statement.kind
    else {
        panic!("expected USE");
    };
    assert_eq!(kind.as_deref(), Some("ROLE"));
}

#[test]
fn use_secondary_roles_round_trips() {
    round_trips("USE SECONDARY ROLES ALL");
    round_trips("USE SECONDARY ROLES NONE");
    round_trips("USE SECONDARY ROLES a, b");
    assert_eq!(renders("USE SECONDARY ROLE ALL"), "USE SECONDARY ROLES ALL");
}

#[test]
fn use_needs_a_name() {
    assert!(rejected("USE"));
    assert!(rejected("USE DATABASE"));
}

#[test]
fn alter_session_has_one_rendering() {
    round_trips("ALTER SESSION SET A=false B='tag'");
    assert_eq!(
        renders("ALTER SESSION SET A=false, B='tag'"),
        "ALTER SESSION SET A=false B='tag'"
    );
    round_trips("ALTER SESSION UNSET A, B");
    assert!(is_snowflake_statement("ALTER SESSION SET A=1"));
}

#[test]
fn alter_session_needs_options() {
    assert!(rejected("ALTER SESSION SET"));
    assert!(rejected("ALTER SESSION UNSET"));
    assert!(rejected("ALTER SESSION"));
}

#[test]
fn alter_table_still_works() {
    round_trips("ALTER TABLE t ADD COLUMN a INT");
}

#[test]
fn snowflake_statements_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("ALTER SESSION SET A=1"));
    assert!(rejected_in::<Core>("USE ROLE r"));
}
