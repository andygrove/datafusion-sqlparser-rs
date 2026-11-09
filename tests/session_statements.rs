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

//! `SHOW`, transactions, `USE`, `EXPLAIN`, `SET`, `COMMENT` and `CALL`.

use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift, Snowflake};
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
    round_trips::<Snowflake>(sql);
}

fn renders_everywhere(sql: &str, expected: &str) {
    assert_eq!(renders::<Core>(sql), expected);
    assert_eq!(renders::<MySql>(sql), expected);
    assert_eq!(renders::<Postgres>(sql), expected);
    assert_eq!(renders::<Redshift>(sql), expected);
    assert_eq!(renders::<Snowflake>(sql), expected);
}

fn rejected<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

fn first(sql: &str) -> Statement<'_, Builtin> {
    parse_sql::<Core>(sql).unwrap().remove(0)
}

// SHOW

#[test]
fn show_round_trips() {
    for sql in [
        "SHOW TABLES",
        "SHOW search_path",
        "SHOW a a",
        "SHOW ALL ALL",
        "SHOW TERSE TABLES",
        "SHOW EXTENDED FULL TABLES",
        "SHOW FULL PROCESSLIST",
        "SHOW MATERIALIZED VIEWS",
        "SHOW EXTERNAL TABLES IN ACCOUNT",
        "SHOW CHARACTER SET LIKE 'utf8%'",
        "SHOW CHARSET WHERE charset = 'x'",
        "SHOW COLLATION WHERE 1 = 2",
        "SHOW COLUMNS FROM a.b",
        "SHOW COLUMNS IN TABLE abc",
        "SHOW COLUMNS IN TABLE",
        "SHOW CATALOGS 'pay*'",
        "SHOW CATALOGS HISTORY STARTS WITH 'pay'",
        "SHOW DATABASES IN ACCOUNT abc",
        "SHOW SCHEMAS IN DATABASE STARTS WITH 'a' LIMIT 20 FROM 'b'",
        "SHOW TABLES LIMIT 10",
        "SHOW VIEWS FROM db1",
        "SHOW OBJECTS IN abc",
        "SHOW TABLES IN db1 'abc'",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn show_create_names_an_object() {
    round_trips_everywhere("SHOW CREATE TABLE a.b");
    round_trips_everywhere("SHOW CREATE VIEW v");
    let StatementKind::Show(show) = first("SHOW CREATE TABLE a.b").kind else {
        panic!("expected SHOW");
    };
    assert_eq!(show.object, ["CREATE", "TABLE"]);
    assert!(show.name.is_some());
}

#[test]
fn show_keeps_what_it_was_given() {
    let StatementKind::Show(show) = first("SHOW TERSE TABLES LIKE 'a' IN SCHEMA s").kind else {
        panic!("expected SHOW");
    };
    assert_eq!(show.modifiers, ["TERSE"]);
    assert_eq!(show.object, ["TABLES"]);
    assert_eq!(show.clauses.len(), 2);
}

#[test]
fn show_does_not_lose_words() {
    round_trips_everywhere("SHOW GLOBAL VARIABLES LIKE 'a'");
    round_trips_everywhere("SHOW VARIABLES");
}

#[test]
fn fields_is_a_spelling_of_columns() {
    renders_everywhere("SHOW FIELDS FROM t", "SHOW COLUMNS FROM t");
}

#[test]
fn extended_and_full_are_for_tables_and_columns() {
    round_trips::<Core>("SHOW EXTENDED COLUMNS FROM t");
    assert!(rejected::<Core>("SHOW FULL VARIABLES"));
    assert!(rejected::<Core>("SHOW EXTENDED COLLATION"));
    assert!(rejected::<Core>("SHOW EXTENDED FULL CREATE TABLE t"));
}

#[test]
fn show_needs_something_to_show() {
    assert!(rejected::<Core>("SHOW"));
    assert!(rejected::<Core>("SHOW )"));
}

#[test]
fn a_modifier_word_can_be_what_is_shown() {
    round_trips::<Core>("SHOW FULL");
}

#[test]
fn from_a_string_is_a_position_and_not_a_location() {
    let StatementKind::Show(show) = first("SHOW TABLES LIMIT 1 FROM 'b'").kind else {
        panic!("expected SHOW");
    };
    assert!(matches!(show.clauses[1], ShowClause::FromValue(_)));
}

// Transactions

#[test]
fn transactions_round_trip() {
    for sql in [
        "START TRANSACTION",
        "START TRANSACTION ISOLATION LEVEL READ COMMITTED",
        "START TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
        "START TRANSACTION ISOLATION LEVEL REPEATABLE READ",
        "START TRANSACTION ISOLATION LEVEL SERIALIZABLE",
        "START TRANSACTION READ ONLY, READ WRITE, ISOLATION LEVEL SERIALIZABLE",
        "BEGIN TRANSACTION",
        "BEGIN WORK",
        "BEGIN",
        "COMMIT",
        "COMMIT AND CHAIN",
        "END",
        "END AND CHAIN",
        "ROLLBACK",
        "ROLLBACK AND CHAIN",
        "ROLLBACK TO SAVEPOINT a",
        "ROLLBACK AND CHAIN TO SAVEPOINT a",
        "SAVEPOINT a",
        "RELEASE SAVEPOINT a",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn transaction_words_are_normalized() {
    renders_everywhere("COMMIT WORK", "COMMIT");
    renders_everywhere("COMMIT TRANSACTION AND CHAIN", "COMMIT AND CHAIN");
    renders_everywhere("COMMIT AND NO CHAIN", "COMMIT");
    renders_everywhere("END WORK AND NO CHAIN", "END");
    renders_everywhere("ROLLBACK TRAN", "ROLLBACK");
    renders_everywhere("ROLLBACK TO a", "ROLLBACK TO SAVEPOINT a");
    renders_everywhere("RELEASE a", "RELEASE SAVEPOINT a");
    renders_everywhere(
        "START TRANSACTION READ ONLY READ WRITE",
        "START TRANSACTION READ ONLY, READ WRITE",
    );
}

#[test]
fn begin_tran_is_kept_outside_snowflake() {
    round_trips::<Core>("BEGIN TRAN");
}

#[test]
fn transactions_can_be_followed_by_statements() {
    let statements = parse_sql::<Core>("BEGIN; DROP TABLE IF EXISTS a; COMMIT").unwrap();
    assert_eq!(statements.len(), 3);
}

#[test]
fn transaction_modes_have_no_trailing_comma() {
    assert!(rejected::<Core>("START TRANSACTION READ ONLY,"));
}

#[test]
fn only_rollback_can_go_to_a_savepoint() {
    assert!(rejected::<Core>("COMMIT TO a"));
}

#[test]
fn isolation_levels_must_exist() {
    assert!(rejected::<Core>(
        "START TRANSACTION ISOLATION LEVEL NONSENSE"
    ));
    assert!(rejected::<Core>("START TRANSACTION READ"));
}

// USE

#[test]
fn use_round_trips() {
    for sql in [
        "USE mydb",
        "USE a.b",
        "USE 'mydb'",
        "USE 'a'.'b'",
        "USE CATALOG",
        "USE DEFAULT",
    ] {
        round_trips_everywhere(sql);
    }
    round_trips::<Core>("USE \"mydb\"");
    assert!(rejected::<Core>("USE"));
}

#[test]
fn use_keeps_the_quote_it_was_written_with() {
    round_trips::<MySql>("USE \"mydb\"");
    round_trips::<MySql>("USE 'mydb'");
}

// EXPLAIN and DESCRIBE

#[test]
fn explain_round_trips() {
    for sql in [
        "EXPLAIN SELECT 1",
        "EXPLAIN ANALYZE SELECT 1",
        "EXPLAIN VERBOSE SELECT 1",
        "EXPLAIN ANALYZE VERBOSE SELECT 1",
        "EXPLAIN ANALYZE VERBOSE FORMAT JSON SELECT 1",
        "EXPLAIN ANALYZE VERBOSE FORMAT=JSON SELECT 1",
        "EXPLAIN FORMAT=TREE SELECT 1",
        "EXPLAIN ESTIMATE SELECT 1",
        "EXPLAIN QUERY PLAN SELECT 1",
        "EXPLAIN INSERT INTO t VALUES (1)",
        "EXPLAIN t",
        "EXPLAIN a.b",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn explain_holds_a_statement_or_a_table() {
    let StatementKind::Explain(explain) = first("EXPLAIN SELECT 1").kind else {
        panic!("expected EXPLAIN");
    };
    assert!(matches!(explain.target, ExplainTarget::Statement(_)));
    let StatementKind::Explain(explain) = first("EXPLAIN t").kind else {
        panic!("expected EXPLAIN");
    };
    assert!(matches!(explain.target, ExplainTarget::Table { .. }));
}

#[test]
fn describe_round_trips() {
    for sql in [
        "DESCRIBE a.b",
        "DESC a.b",
        "DESCRIBE EXTENDED a.b",
        "DESCRIBE FORMATTED a.b",
        "DESCRIBE SELECT 1",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn describe_table_is_a_snowflake_keyword() {
    round_trips::<Snowflake>("DESCRIBE TABLE a.b");
    round_trips::<Snowflake>("DESC TABLE a.b");
    round_trips::<Snowflake>("EXPLAIN TABLE a");
    round_trips::<Core>("DESCRIBE table");
    assert!(rejected::<Core>("DESCRIBE TABLE a.b"));
}

#[test]
fn postgres_explain_takes_options() {
    for sql in [
        "EXPLAIN (ANALYZE ON, VERBOSE OFF) SELECT 1",
        "EXPLAIN (ANALYZE false, VERBOSE true) SELECT 1",
        "EXPLAIN (ANALYZE, WAL OFF, FORMAT YAML, N -100.1) SELECT 1",
        "EXPLAIN (A TEXT, B 'JSON', C \"XML\") SELECT 1",
        "EXPLAIN (A 10, B +10.1) SELECT 1",
    ] {
        round_trips::<Postgres>(sql);
    }
    assert!(rejected::<Core>("EXPLAIN (ANALYZE ON) SELECT 1"));
}

#[test]
fn explain_modifier_words_can_be_table_names() {
    round_trips::<Core>("EXPLAIN verbose");
    round_trips::<Core>("EXPLAIN analyze");
    round_trips::<Core>("EXPLAIN format.t");
    round_trips::<Snowflake>("DESC TABLE create");
}

#[test]
fn a_parenthesized_query_is_not_a_list_of_options() {
    round_trips::<Postgres>("EXPLAIN (SELECT 1)");
}

#[test]
fn explain_needs_a_target() {
    assert!(rejected::<Core>("EXPLAIN"));
    assert!(rejected::<Core>("DESCRIBE"));
}

// SET

#[test]
fn set_round_trips() {
    for sql in [
        "SET a = 'b'",
        "SET a = 0",
        "SET a = DEFAULT",
        "SET a = b",
        "SET a.b.c = d",
        "SET LOCAL a = b",
        "SET GLOBAL a = 'b'",
        "SET role = 'x'",
        "SET ROLE 'x'",
        "SET LOCAL ROLE 'x'",
        "SET SESSION ROLE NONE",
        "SET SESSION AUTHORIZATION 'u'",
        "SET SESSION AUTHORIZATION DEFAULT",
        "SET TIME ZONE 'UTC'",
        "SET TIMEZONE = 'UTC'",
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
        "SET TRANSACTION READ ONLY, READ WRITE, ISOLATION LEVEL SERIALIZABLE",
        "SET TRANSACTION SNAPSHOT 'a'",
        "SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn time_can_be_a_variable() {
    round_trips::<Core>("SET time = 1");
}

#[test]
fn set_names_keeps_collate_separate() {
    let statement = parse_sql::<MySql>("SET NAMES a COLLATE b")
        .unwrap()
        .remove(0);
    let StatementKind::Set(set) = statement.kind else {
        panic!("expected SET");
    };
    assert!(matches!(
        *set,
        SetStatement::Names {
            collate: Some(_),
            ..
        }
    ));
}

#[test]
fn set_normalizes_to() {
    renders_everywhere("SET a TO b", "SET a = b");
    renders_everywhere("SET a=b", "SET a = b");
    renders_everywhere("SET TIME ZONE TO 'UTC'", "SET TIMEZONE = 'UTC'");
}

#[test]
fn set_can_take_a_list_of_values_outside_mysql() {
    round_trips::<Core>("SET a = 1, 2");
}

#[test]
fn mysql_set_takes_several_assignments() {
    round_trips::<MySql>("SET @a = 1, b = 2");
    round_trips::<MySql>("SET GLOBAL @a = 1, SESSION b = 2, LOCAL c = 3, d = 4");
    round_trips::<MySql>("SET a = 1");
    round_trips::<MySql>("SET sql_mode = CONCAT(@@sql_mode, ',x')");
}

#[test]
fn snowflake_variables_do_not_start_with_an_at_sign() {
    assert!(rejected::<Snowflake>("SET @a = 1"));
}

#[test]
fn set_names_is_in_mysql_and_postgres() {
    for sql in [
        "SET NAMES 'utf8'",
        "SET NAMES utf8 COLLATE bogus",
        "SET NAMES DEFAULT",
    ] {
        round_trips::<MySql>(sql);
        round_trips::<Postgres>(sql);
        assert!(rejected::<Core>(sql), "{sql}");
    }
}

#[test]
fn set_needs_a_value() {
    assert!(rejected::<Core>("SET a"));
    assert!(rejected::<Core>("SET a ="));
}

// COMMENT and CALL

#[test]
fn comment_on_is_in_postgres_and_snowflake() {
    for sql in [
        "COMMENT ON TABLE a.b IS 'c'",
        "COMMENT ON COLUMN t.a IS 'c'",
        "COMMENT IF EXISTS ON MATERIALIZED VIEW v IS 'c'",
        "COMMENT ON TABLE t IS NULL",
    ] {
        round_trips::<Postgres>(sql);
        round_trips::<Snowflake>(sql);
        assert!(rejected::<Core>(sql), "{sql}");
        assert!(rejected::<MySql>(sql), "{sql}");
    }
    assert!(rejected::<Postgres>("COMMENT ON UNKNOWN t IS 'c'"));
    assert!(rejected::<Postgres>("COMMENT ON TABLE t"));
    assert!(rejected::<Postgres>("COMMENT ON TABLE t IS 1"));
}

#[test]
fn call_round_trips() {
    for sql in ["CALL p", "CALL p()", "CALL p(1, 'a')", "CALL a.b(1)"] {
        round_trips_everywhere(sql);
    }
    assert!(rejected::<Core>("CALL"));
}
