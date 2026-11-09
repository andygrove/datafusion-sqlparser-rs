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

//! SQL that must be rejected, or read differently, in specific dialects.

mod common;

use sqlparser::ast::ExprKind;
use std::borrow::Cow;

use common::*;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift, Snowflake, Spark};
use sqlparser::{parse_expr, parse_query};

fn accepted<D: Dialect>(sql: &str) -> bool {
    parse_query::<D>(sql).is_ok()
}

fn rejected_everywhere(sql: &str) {
    assert!(!accepted::<Core>(sql), "ansi: {sql}");
    assert!(!accepted::<MySql>(sql), "mysql: {sql}");
    assert!(!accepted::<Postgres>(sql), "postgres: {sql}");
    assert!(!accepted::<Redshift>(sql), "redshift: {sql}");
    assert!(!accepted::<Snowflake>(sql), "snowflake: {sql}");
    assert!(!accepted::<Spark>(sql), "spark: {sql}");
}

fn string<D: Dialect<Ext = Builtin>>(sql: &str) -> StringLiteral<'_> {
    match parse_expr::<D>(sql) {
        Ok(Expr {
            kind: ExprKind::Literal(Literal::String(string)),
            ..
        }) => string,
        other => panic!("expected a string from {sql}, found {other:?}"),
    }
}

// Keywords that clauses begin with cannot be implicit aliases

/// Snowflake reads some clause keywords as aliases when no clause follows.
fn rejected_outside_snowflake(sql: &str) {
    assert!(!accepted::<Core>(sql), "ansi: {sql}");
    assert!(!accepted::<MySql>(sql), "mysql: {sql}");
    assert!(!accepted::<Postgres>(sql), "postgres: {sql}");
    assert!(!accepted::<Redshift>(sql), "redshift: {sql}");
    assert!(!accepted::<Spark>(sql), "spark: {sql}");
    assert!(accepted::<Snowflake>(sql), "snowflake: {sql}");
}

#[test]
fn clause_keywords_are_not_table_aliases() {
    for word in ["FOR", "INTO", "ARRAY"] {
        rejected_everywhere(&format!("SELECT 1 FROM t {word}"));
    }
    for word in ["FETCH", "WINDOW"] {
        rejected_outside_snowflake(&format!("SELECT 1 FROM t {word}"));
    }
}

#[test]
fn clause_keywords_are_not_select_aliases() {
    for word in ["FOR", "WINDOW", "INTO", "ARRAY"] {
        rejected_everywhere(&format!("SELECT 1 {word}"));
    }
    rejected_outside_snowflake("SELECT 1 FETCH");
}

#[test]
fn array_join_is_not_read_as_an_alias_and_a_join() {
    rejected_everywhere("SELECT x FROM t ARRAY JOIN arr AS x");
}

#[test]
fn clause_keywords_can_follow_as() {
    assert!(accepted::<Core>("SELECT 1 AS fetch FROM t AS window"));
}

#[test]
fn quoted_clause_keywords_can_be_aliases() {
    assert!(accepted::<Core>("SELECT 1 \"for\" FROM t \"into\""));
}

#[test]
fn clause_keywords_can_still_name_columns() {
    assert!(accepted::<Core>("SELECT fetch, window FROM t"));
}

#[test]
fn words_only_some_dialects_reserve_stay_valid_aliases_in_postgres() {
    for word in ["sort", "anti", "semi", "top", "view", "settings"] {
        let sql = format!("SELECT 1 FROM t {word}");
        assert!(accepted::<Postgres>(&sql), "{sql}");
    }
}

// Words MySQL reserves

#[test]
fn mysql_reserved_words_are_not_aliases() {
    for word in [
        "USE",
        "FORCE",
        "IGNORE",
        "PARTITION",
        "ANALYZE",
        "EXPLAIN",
        "MATCH",
        "REGEXP",
        "RLIKE",
    ] {
        let sql = format!("SELECT 1 FROM t {word}");
        assert!(!accepted::<MySql>(&sql), "{sql}");
    }
}

#[test]
fn mysql_reserved_words_are_ordinary_elsewhere() {
    assert!(accepted::<Postgres>("SELECT 1 FROM t use"));
    assert!(accepted::<Core>("SELECT 1 FROM t regexp"));
}

#[test]
fn insert_ignore_still_works_with_ignore_reserved() {
    assert!(sqlparser::parse_sql::<MySql>("INSERT IGNORE INTO t VALUES (1)").is_ok());
}

// Backslash escapes

#[test]
fn mysql_backslash_escapes_a_quote() {
    let literal = string::<MySql>(r"'it\'s'");
    assert_eq!(literal.text, r"it\'s");
    assert_eq!(literal.value(), "it's");
}

#[test]
fn mysql_escapes_apply_in_double_quoted_strings() {
    assert_eq!(string::<MySql>(r#""a\"b""#).value(), "a\"b");
}

#[test]
fn mysql_escape_sequences_are_resolved() {
    let cases = [
        (r"'\\'", "\\"),
        (r"'a\nb'", "a\nb"),
        (r"'\t\r\b'", "\t\r\u{8}"),
        (r"'\0'", "\0"),
        (r"'\Z'", "\u{1a}"),
        (r"'\q'", "q"),
    ];
    for (sql, value) in cases {
        assert_eq!(string::<MySql>(sql).value(), value, "{sql}");
    }
}

#[test]
fn mysql_keeps_the_backslash_in_like_wildcards() {
    assert_eq!(string::<MySql>(r"'\%\_'").value(), r"\%\_");
}

#[test]
fn mysql_doubled_quotes_still_work() {
    assert_eq!(string::<MySql>("'it''s'").value(), "it's");
}

#[test]
fn string_without_escapes_does_not_allocate() {
    assert!(matches!(
        string::<MySql>("'plain'").value(),
        Cow::Borrowed("plain")
    ));
}

#[test]
fn trailing_backslash_escapes_the_closing_quote() {
    assert!(parse_expr::<MySql>(r"'\'").is_err());
    assert!(parse_expr::<Redshift>(r"'\'").is_err());
}

#[test]
fn redshift_backslash_escapes_a_quote() {
    assert_eq!(string::<Redshift>(r"'it\'s'").value(), "it's");
}

#[test]
fn backslash_is_ordinary_text_elsewhere() {
    assert_eq!(string::<Core>(r"'\'").value(), "\\");
    assert_eq!(string::<Postgres>(r"'a\'").text, r"a\");
}

// FILTER

#[test]
fn filter_is_not_available_in_mysql_or_redshift() {
    let sql = "SELECT COUNT(*) FILTER (WHERE a) FROM t";
    assert!(!accepted::<MySql>(sql));
    assert!(!accepted::<Redshift>(sql));
}

#[test]
fn filter_is_available_in_the_core_and_postgres() {
    let sql = "SELECT COUNT(*) FILTER (WHERE a) FROM t";
    assert!(accepted::<Core>(sql));
    assert!(accepted::<Postgres>(sql));
}

#[test]
fn filter_override_leaves_ordinary_calls_alone() {
    assert_eq!(
        parse_expr::<MySql>("f(a)").unwrap(),
        call(&["f"], vec![FunctionArg::Expr(col("a"))])
    );
}

#[test]
fn mysql_resolves_doubled_quotes_alongside_backslashes() {
    assert_eq!(string::<MySql>(r"'a\n''b'").value(), "a\n'b");
}
