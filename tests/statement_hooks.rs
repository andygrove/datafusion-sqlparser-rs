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

//! Every statement and clause can be replaced by a dialect.

use sqlparser::ast::*;
use sqlparser::dialect::{Builtin, Core, Dialect, MySql, Postgres, Redshift, Snowflake};
use sqlparser::lexer::TokenKind;
use sqlparser::parser::Parser;
use sqlparser::{PResult, parse_sql};

/// Fails from each statement hook, naming the hook in the error.
#[derive(Debug, Clone, PartialEq)]
struct Statements;

impl Dialect for Statements {
    type Ext = Builtin;

    fn parse_show<'a>(p: &mut Parser<'a, Self>) -> PResult<Show<'a, Builtin>> {
        Err(p.expected("hook show"))
    }
    fn parse_transaction<'a>(p: &mut Parser<'a, Self>) -> PResult<Transaction<'a>> {
        Err(p.expected("hook transaction"))
    }
    fn parse_use<'a>(p: &mut Parser<'a, Self>) -> PResult<ObjectName<'a>> {
        Err(p.expected("hook use"))
    }
    fn parse_explain<'a>(p: &mut Parser<'a, Self>) -> PResult<Explain<'a, Builtin>> {
        Err(p.expected("hook explain"))
    }
    fn parse_call<'a>(p: &mut Parser<'a, Self>) -> PResult<Expr<'a, Builtin>> {
        Err(p.expected("hook call"))
    }
    fn parse_grant<'a>(p: &mut Parser<'a, Self>) -> PResult<Grant<'a>> {
        Err(p.expected("hook grant"))
    }
    fn parse_revoke<'a>(p: &mut Parser<'a, Self>) -> PResult<Revoke<'a>> {
        Err(p.expected("hook revoke"))
    }
    fn parse_merge<'a>(p: &mut Parser<'a, Self>) -> PResult<Merge<'a, Builtin>> {
        Err(p.expected("hook merge"))
    }
    fn parse_drop_routine<'a>(p: &mut Parser<'a, Self>) -> PResult<DropRoutine<'a, Builtin>> {
        Err(p.expected("hook drop routine"))
    }
}

#[test]
fn statements_go_through_their_hooks() {
    for (sql, hook) in [
        ("SHOW TABLES", "hook show"),
        ("BEGIN", "hook transaction"),
        ("COMMIT", "hook transaction"),
        ("USE db", "hook use"),
        ("EXPLAIN SELECT 1", "hook explain"),
        ("DESCRIBE t", "hook explain"),
        ("CALL f()", "hook call"),
        ("GRANT SELECT ON t TO u", "hook grant"),
        ("REVOKE SELECT ON t FROM u", "hook revoke"),
        (
            "MERGE INTO t USING u ON a WHEN MATCHED THEN DELETE",
            "hook merge",
        ),
        ("DROP FUNCTION f", "hook drop routine"),
    ] {
        let error = parse_sql::<Statements>(sql).unwrap_err();
        assert!(error.message.contains(hook), "{sql}: {}", error.message);
    }
}

/// Replaces rules inside statements.
#[derive(Debug, Clone, PartialEq)]
struct Clauses;

impl Dialect for Clauses {
    type Ext = Builtin;

    fn parse_explain_target<'a>(p: &mut Parser<'a, Self>) -> PResult<ExplainTarget<'a, Builtin>> {
        Err(p.expected("hook explain target"))
    }
    fn parse_name_part<'a>(p: &mut Parser<'a, Self>) -> PResult<Ident<'a>> {
        Err(p.expected("hook name part"))
    }
}

#[test]
fn explain_targets_go_through_their_hook() {
    let error = parse_sql::<Clauses>("EXPLAIN SELECT 1").unwrap_err();
    assert!(
        error.message.contains("hook explain target"),
        "{}",
        error.message
    );
}

#[test]
fn name_parts_go_through_their_hook() {
    let error = parse_sql::<Clauses>("SELECT a.b FROM t").unwrap_err();
    assert!(
        error.message.contains("hook name part"),
        "{}",
        error.message
    );
}

/// Accepts a marker word and adds a setting, as a dialect with its own index
/// syntax would.
#[derive(Debug, Clone, PartialEq)]
struct Indexes;

impl Dialect for Indexes {
    type Ext = Builtin;

    fn parse_index_flags<'a>(p: &mut Parser<'a, Self>) -> IndexFlags {
        IndexFlags {
            concurrently: p.eat_keyword("QUICKLY"),
            asynchronous: false,
        }
    }
    fn parse_index_settings<'a>(p: &mut Parser<'a, Self>) -> PResult<Vec<(&'static str, &'a str)>> {
        if !p.eat_keyword("MODE") {
            return Ok(Vec::new());
        }
        let token = p.peek();
        if token.kind != TokenKind::Word {
            return Err(p.expected("a mode"));
        }
        p.advance();
        Ok(vec![("MODE", token.text)])
    }
}

#[test]
fn dialects_add_their_own_index_syntax() {
    let statements = parse_sql::<Indexes>("CREATE INDEX QUICKLY i ON t (a) MODE fast").unwrap();
    assert_eq!(
        statements[0].to_string(),
        "CREATE INDEX CONCURRENTLY i ON t (a) MODE = fast"
    );
}

fn accepted<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_ok()
}

#[test]
fn concurrently_and_async_are_postgres_syntax() {
    for sql in [
        "CREATE INDEX CONCURRENTLY i ON t (a)",
        "CREATE INDEX ASYNC i ON t (a)",
    ] {
        assert!(accepted::<Postgres>(sql), "{sql}");
        assert!(!accepted::<Core>(sql), "{sql}");
        assert!(!accepted::<MySql>(sql), "{sql}");
        assert!(!accepted::<Redshift>(sql), "{sql}");
        assert!(!accepted::<Snowflake>(sql), "{sql}");
    }
}

#[test]
fn those_words_stay_ordinary_index_names_elsewhere() {
    let statements = parse_sql::<Core>("CREATE INDEX concurrently ON t (a)").unwrap();
    assert_eq!(
        statements[0].to_string(),
        "CREATE INDEX concurrently ON t (a)"
    );
}

#[test]
fn async_can_still_name_a_postgres_index() {
    let statements = parse_sql::<Postgres>("CREATE INDEX async ON t (a)").unwrap();
    assert_eq!(statements[0].to_string(), "CREATE INDEX async ON t (a)");
}

#[test]
fn algorithm_and_lock_are_mysql_syntax() {
    let sql = "CREATE INDEX i ON t (a) ALGORITHM = INPLACE LOCK = NONE";
    assert!(accepted::<MySql>(sql));
    assert!(!accepted::<Core>(sql));
    assert!(!accepted::<Postgres>(sql));
    assert!(!accepted::<Redshift>(sql));
    assert!(!accepted::<Snowflake>(sql));
}

/// Reuses Snowflake's statements in a dialect of its own, with its own queries.
#[derive(Debug, Clone, PartialEq)]
struct Warehouse;

impl Dialect for Warehouse {
    type Ext = Builtin;

    fn parse_statement<'a>(p: &mut Parser<'a, Self>) -> PResult<Statement<'a, Builtin>> {
        sqlparser::dialect::snowflake::parse_statement(p)
    }
    fn parse_query<'a>(p: &mut Parser<'a, Self>) -> PResult<Query<'a, Builtin>> {
        Err(p.expected("hook query"))
    }
}

#[test]
fn another_dialect_can_reuse_snowflake_rules() {
    let statements = parse_sql::<Warehouse>("CREATE STAGE s").unwrap();
    assert_eq!(statements[0].to_string(), "CREATE STAGE s");
}

#[test]
fn reused_snowflake_rules_call_the_host_dialects_hooks() {
    let error = parse_sql::<Warehouse>("INSERT ALL INTO t SELECT 1").unwrap_err();
    assert!(error.message.contains("hook query"), "{}", error.message);
}
