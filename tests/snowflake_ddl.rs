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

//! Snowflake's table options, scripting and remaining statements.

mod common;

use sqlparser::ast::{
    AlterTableOp, AngleArguments, Expr, ExprKind, FunctionArg, SelectItem, SelectItemKind,
    StatementKind,
};
use sqlparser::dialect::builtin::{BuiltinExpr, BuiltinStatement};
use sqlparser::dialect::snowflake::{SnowflakeExpr, SnowflakeStatement};
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Snowflake};
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

fn is_snowflake_statement(sql: &str) -> bool {
    matches!(
        parse_sql::<Snowflake>(sql).unwrap().remove(0).kind,
        StatementKind::Ext(BuiltinStatement::Snowflake(_))
    )
}

// Parenthesized tables

#[test]
fn parentheses_around_a_table_are_dropped() {
    assert_eq!(renders("SELECT * FROM ((a.b.c))"), "SELECT * FROM a.b.c");
    assert_eq!(renders("SELECT * FROM (a b)"), "SELECT * FROM a b");
    assert_eq!(renders("SELECT * FROM (a) c"), "SELECT * FROM a c");
    assert_eq!(
        renders("SELECT * FROM ((SELECT 1) AS t)"),
        "SELECT * FROM (SELECT 1) AS t"
    );
    assert_eq!(
        renders("SELECT * FROM (((SELECT 1) AS t))"),
        "SELECT * FROM (SELECT 1) AS t"
    );
    assert_eq!(
        renders("SELECT * FROM (a NATURAL JOIN (b))"),
        "SELECT * FROM (a NATURAL JOIN b)"
    );
}

#[test]
fn a_parenthesized_table_has_one_alias() {
    assert!(rejected("SELECT * FROM (a b) c"));
}

#[test]
fn parentheses_around_a_join_are_kept() {
    round_trips("SELECT * FROM (a JOIN b ON c)");
    round_trips("SELECT * FROM (((a NATURAL JOIN b)))");
}

#[test]
fn parentheses_around_a_table_stay_errors_elsewhere() {
    assert!(rejected_in::<Postgres>("SELECT * FROM (a)"));
}

// Outer joins

#[test]
fn oracle_style_outer_joins_round_trip() {
    round_trips("SELECT 1 FROM t WHERE a = b (+)");
    round_trips("SELECT 1 FROM t WHERE t1.c1 = t2.c2 (+)");
    assert_eq!(
        renders("SELECT 1 FROM t WHERE t1.c1 = t2.c2( + )"),
        "SELECT 1 FROM t WHERE t1.c1 = t2.c2 (+)"
    );
}

#[test]
fn outer_join_marks_follow_columns() {
    assert!(rejected("SELECT 1 FROM t WHERE 1 = 2 (+)"));
}

#[test]
fn a_call_with_a_plus_is_still_a_call() {
    round_trips("SELECT f(+a)");
}

#[test]
fn outer_join_marks_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("SELECT a = b (+)"));
}

// CREATE TABLE

#[test]
fn a_plain_create_table_is_the_core_statement() {
    assert!(!is_snowflake_statement("CREATE TABLE t (a INT)"));
    assert!(!is_snowflake_statement(
        "CREATE OR REPLACE TEMPORARY TABLE t (a INT)"
    ));
    assert!(!is_snowflake_statement("CREATE TABLE t AS SELECT 1"));
    round_trips("CREATE TABLE t (a INT, PRIMARY KEY (a))");
}

#[test]
fn table_options_round_trip() {
    for sql in [
        "CREATE TABLE t (a INT) CLUSTER BY (a, f(b))",
        "CREATE OR REPLACE TABLE t (a INT) COPY GRANTS",
        "CREATE OR REPLACE TABLE t COPY GRANTS AS SELECT 1 AS a",
        "CREATE TABLE t (a INT) CHANGE_TRACKING=TRUE",
        "CREATE TABLE t (a INT) DATA_RETENTION_TIME_IN_DAYS=5",
        "CREATE TABLE t (a INT) DEFAULT_DDL_COLLATION='de'",
        "CREATE TABLE t (a INT) COMMENT = 'c'",
        "CREATE TABLE t (a INT) WITH TAG (A='x', b.c='y')",
        "CREATE TABLE t (a INT) WITH AGGREGATION POLICY p",
        "CREATE TABLE t (a INT, b INT) WITH ROW ACCESS POLICY p ON (a)",
        "CREATE TABLE t (a INT) WITH STORAGE LIFECYCLE POLICY s.p ON (a, b)",
        "CREATE TRANSIENT TABLE t (a INT)",
        "CREATE VOLATILE TABLE t (a INT)",
        "CREATE GLOBAL TEMPORARY TABLE t (a INT)",
        "CREATE LOCAL TEMPORARY TABLE t (a INT)",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql), "{sql}");
    }
}

#[test]
fn if_not_exists_can_come_before_the_name() {
    round_trips("CREATE TRANSIENT TABLE IF NOT EXISTS t (a INT)");
}

#[test]
fn table_clauses_have_a_canonical_form() {
    assert_eq!(
        renders("CREATE OR REPLACE TABLE t COPY GRANTS (a INT)"),
        "CREATE OR REPLACE TABLE t (a INT) COPY GRANTS"
    );
    assert_eq!(
        renders("CREATE TABLE \"A\".\"B\" IF NOT EXISTS (v VARIANT)"),
        "CREATE TABLE IF NOT EXISTS \"A\".\"B\" (v VARIANT)"
    );
    assert_eq!(
        renders("CREATE TABLE t (a INT) TAG (A='x')"),
        "CREATE TABLE t (a INT) WITH TAG (A='x')"
    );
    assert_eq!(
        renders("CREATE TABLE t (a INT) AGGREGATION POLICY p"),
        "CREATE TABLE t (a INT) WITH AGGREGATION POLICY p"
    );
    assert_eq!(
        renders("CREATE TABLE t (a INT) ROW ACCESS POLICY p ON (a)"),
        "CREATE TABLE t (a INT) WITH ROW ACCESS POLICY p ON (a)"
    );
    assert_eq!(
        renders("CREATE TABLE t (a STRING COMMENT $$c$$) COMMENT = $$d$$"),
        "CREATE TABLE t (a STRING COMMENT 'c') COMMENT = 'd'"
    );
}

#[test]
fn on_commit_clone_and_like_are_ordered() {
    assert_eq!(
        renders("CREATE TEMPORARY TABLE d AS (SELECT 1) ON COMMIT PRESERVE ROWS"),
        "CREATE TEMPORARY TABLE d ON COMMIT PRESERVE ROWS AS (SELECT 1)"
    );
    assert_eq!(
        renders("CREATE TEMPORARY TABLE d ON COMMIT DELETE ROWS CLONE s"),
        "CREATE TEMPORARY TABLE d CLONE s ON COMMIT DELETE ROWS"
    );
    assert_eq!(
        renders("CREATE TEMPORARY TABLE d ON COMMIT DROP LIKE s"),
        "CREATE TEMPORARY TABLE d LIKE s ON COMMIT DROP"
    );
    assert_eq!(
        renders("CREATE TEMPORARY TABLE d ON COMMIT DROP CLONE s AT(x => 1)"),
        "CREATE TEMPORARY TABLE d CLONE s AT(x => 1) ON COMMIT DROP"
    );
}

#[test]
fn a_table_without_columns_renders_empty_parentheses() {
    assert_eq!(renders("CREATE TABLE t"), "CREATE TABLE t ()");
    round_trips("CREATE TABLE t CLONE s");
}

#[test]
fn a_table_of_only_constraints_keeps_them() {
    round_trips("CREATE TABLE t (CONSTRAINT c CHECK (2 > 1))");
    round_trips("CREATE TABLE t (CHECK (a > 1)) COPY GRANTS");
}

#[test]
fn constraints_are_kept_when_a_query_supplies_the_columns() {
    round_trips("CREATE TABLE t (CHECK (a > 1)) AS SELECT 1 AS a");
}

#[test]
fn a_table_comment_keeps_its_equals_sign_or_lack_of_one() {
    round_trips("CREATE TABLE t (a INT) COMMENT 'c'");
    round_trips("CREATE TABLE t (a INT) COMMENT = 'c'");
}

#[test]
fn empty_parentheses_parse_back() {
    round_trips("CREATE TABLE t ()");
}

#[test]
fn dynamic_and_iceberg_tables_round_trip() {
    for sql in [
        "CREATE DYNAMIC TABLE t TARGET_LAG='1 minute' WAREHOUSE=w AS SELECT 1",
        "CREATE OR REPLACE DYNAMIC TABLE t (a INT) CLUSTER BY (a) TARGET_LAG='x' WAREHOUSE=w AS SELECT 1",
        "CREATE DYNAMIC TABLE t TARGET_LAG='x' WAREHOUSE=w REFRESH_MODE=AUTO INITIALIZE=ON_SCHEDULE REQUIRE USER AS SELECT 1",
        "CREATE DYNAMIC TABLE t CLONE s AT(TIMESTAMP => f('x'))",
        "CREATE DYNAMIC TABLE t CLONE s BEFORE(OFFSET => f('x'))",
        "CREATE ICEBERG TABLE t (a INT) BASE_LOCATION='p'",
        "CREATE DYNAMIC ICEBERG TABLE t (a INT) BASE_LOCATION='p' TARGET_LAG='x' WAREHOUSE=w AS SELECT 1",
    ] {
        round_trips(sql);
    }
}

#[test]
fn a_dynamic_table_with_only_columns_stays_dynamic() {
    round_trips("CREATE DYNAMIC TABLE t (a INT)");
}

#[test]
fn iceberg_tables_need_a_base_location() {
    assert!(rejected("CREATE ICEBERG TABLE t (a INT)"));
}

#[test]
fn identity_columns_round_trip() {
    for sql in [
        "CREATE TABLE t (a INT IDENTITY)",
        "CREATE TABLE t (a INT IDENTITY NOT NULL)",
        "CREATE TABLE t (a INT IDENTITY(1, 1) NOT NULL)",
        "CREATE TABLE t (a INT AUTOINCREMENT ORDER)",
        "CREATE TABLE t (a INT AUTOINCREMENT(100, 1) NOORDER)",
        "CREATE TABLE t (a INT IDENTITY START 100 INCREMENT 1 ORDER)",
    ] {
        round_trips(sql);
    }
}

#[test]
fn column_policies_and_tags_round_trip() {
    for sql in [
        "CREATE TABLE t (a INT MASKING POLICY p)",
        "CREATE TABLE t (a INT WITH MASKING POLICY p USING (a, b))",
        "CREATE TABLE t (a INT PROJECTION POLICY p)",
        "CREATE TABLE t (a INT WITH PROJECTION POLICY p)",
        "CREATE TABLE t (a INT TAG (A='x', B='y'))",
        "CREATE TABLE t (a INT WITH TAG (A='x'))",
        "CREATE TABLE t (a TEXT COLLATE 'de_DE' COMMENT 'c')",
    ] {
        round_trips(sql);
    }
}

#[test]
fn unknown_table_options_are_an_error() {
    assert!(rejected("CREATE TABLE t (a INT) ENGINE=InnoDB"));
    assert!(rejected("CREATE DATABASE d ENGINE = InnoDB"));
}

#[test]
fn table_options_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>(
        "CREATE TABLE t (a INT) CLUSTER BY (a)"
    ));
    assert!(rejected_in::<Core>("CREATE TRANSIENT TABLE t (a INT)"));
    assert!(rejected_in::<Postgres>("CREATE TABLE t (a INT IDENTITY)"));
}

// CREATE DATABASE

#[test]
fn create_database_round_trips() {
    for sql in [
        "CREATE OR REPLACE DATABASE d",
        "CREATE OR REPLACE TRANSIENT DATABASE IF NOT EXISTS d CLONE s DATA_RETENTION_TIME_IN_DAYS = 1",
        "CREATE DATABASE d EXTERNAL_VOLUME = 'v' CATALOG = 'c' REPLACE_INVALID_CHARACTERS = TRUE",
        "CREATE DATABASE d COMMENT = 'c' WITH TAG (A='x') WITH CONTACT (p = q)",
    ] {
        round_trips(sql);
    }
    assert!(!is_snowflake_statement("CREATE DATABASE d"));
}

// ALTER

#[test]
fn alter_dynamic_and_external_tables_round_trip() {
    for sql in [
        "ALTER DYNAMIC TABLE a.b REFRESH",
        "ALTER DYNAMIC TABLE t SUSPEND",
        "ALTER DYNAMIC TABLE t RESUME",
        "ALTER EXTERNAL TABLE t REFRESH",
        "ALTER EXTERNAL TABLE IF EXISTS t REFRESH",
        "ALTER EXTERNAL TABLE t REFRESH 'year=2025/'",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql));
    }
    assert!(rejected("ALTER DYNAMIC TABLE t"));
    assert!(rejected("ALTER DYNAMIC TABLE t DROP"));
    assert!(rejected("ALTER EXTERNAL TABLE t SUSPEND"));
}

#[test]
fn alter_session_unset_takes_spaces_or_commas() {
    assert_eq!(
        renders("ALTER SESSION UNSET a B"),
        "ALTER SESSION UNSET a, B"
    );
}

// Scripting

#[test]
fn blocks_round_trip() {
    for sql in [
        "BEGIN SELECT 1; END",
        "BEGIN SELECT 1; SELECT 2; END",
        "BEGIN INSERT INTO t VALUES (1); UPDATE t SET x = 2; END",
        "BEGIN SELECT 1; EXCEPTION WHEN OTHER THEN SELECT 2; RAISE; END",
        "BEGIN SELECT 1; EXCEPTION WHEN FOO THEN SELECT 2; WHEN OTHER THEN SELECT 3; RAISE; END",
        "BEGIN SELECT 1; EXCEPTION WHEN A OR B THEN SELECT 2; END",
        "BEGIN SELECT 1; EXCEPTION WHEN OTHER THEN RAISE EX_1; END",
        "BEGIN SELECT 1; EXCEPTION WHEN E THEN RAISE USING MESSAGE = FORMAT('x', 'y'); END",
        "BEGIN EXCEPTION WHEN ERROR THEN END",
        "BEGIN EXCEPTION WHEN ERROR THEN SELECT 2; END",
        "BEGIN BEGIN SELECT 1; EXCEPTION WHEN OTHER THEN SELECT 2; RAISE; END; END",
    ] {
        round_trips(sql);
    }
}

#[test]
fn blocks_need_end() {
    assert!(rejected("BEGIN SELECT 1;"));
    assert!(rejected("BEGIN SELECT 1; EXCEPTION WHEN E SELECT 2; END"));
}

#[test]
fn deeply_nested_blocks_are_an_error() {
    let depth = 5000;
    let sql = format!(
        "{}SELECT 1;{}",
        "BEGIN ".repeat(depth),
        " END;".repeat(depth)
    );
    assert!(rejected(&sql));
}

#[test]
fn blocks_are_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("BEGIN SELECT 1; END"));
}

#[test]
fn declarations_round_trip() {
    for sql in [
        "DECLARE x",
        "DECLARE x DEFAULT 42",
        "DECLARE x INT64",
        "DECLARE x TEXT DEFAULT 42",
        "DECLARE c CURSOR FOR res",
        "DECLARE c CURSOR FOR SELECT 1",
        "DECLARE e EXCEPTION",
        "DECLARE e EXCEPTION (42, 'ERROR')",
        "DECLARE r RESULTSET",
        "DECLARE r RESULTSET := 42",
        "DECLARE r RESULTSET DEFAULT (SELECT 1)",
        "DECLARE a DEFAULT 42; r RESULTSET DEFAULT (SELECT 1); c CURSOR FOR r; e EXCEPTION (-1, 'x')",
    ] {
        round_trips(sql);
    }
    assert_eq!(renders("DECLARE x;"), "DECLARE x");
}

#[test]
fn a_statement_after_a_declaration_is_separate() {
    let statements = parse_sql::<Snowflake>("DECLARE x; SELECT 1").unwrap();
    assert_eq!(statements.len(), 2);
    let statements = parse_sql::<Snowflake>("DECLARE x; BEGIN SELECT 1; END").unwrap();
    assert_eq!(statements.len(), 2);
}

#[test]
fn declare_is_not_in_other_dialects() {
    assert!(rejected_in::<Core>("DECLARE x"));
}

// SET

#[test]
fn set_with_a_tuple_round_trips() {
    round_trips("SET (a, b, c) = (1, 2, 3)");
    round_trips("SET (a) = ((SELECT 22 FROM t))");
    assert_eq!(
        renders("SET (a) = (SELECT 22 FROM t)"),
        "SET (a) = ((SELECT 22 FROM t))"
    );
    assert_eq!(
        renders("SET (a, b) = ((SELECT 1), SELECT 2)"),
        "SET (a, b) = ((SELECT 1), (SELECT 2))"
    );
}

#[test]
fn set_needs_values() {
    assert!(rejected("SET (a) ="));
    assert!(rejected("SET (a) = 1"));
}

// Views

#[test]
fn secure_views_round_trip() {
    for sql in [
        "CREATE SECURE VIEW v AS SELECT 1",
        "CREATE SECURE MATERIALIZED VIEW v AS SELECT 1",
        "CREATE OR REPLACE SECURE VIEW v AS SELECT 1",
        "CREATE OR REPLACE SECURE MATERIALIZED VIEW v AS SELECT 1",
        "CREATE SECURE TEMPORARY VIEW v AS SELECT 1",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql), "{sql}");
    }
    assert_eq!(
        renders("CREATE TEMPORARY SECURE VIEW v AS SELECT 1"),
        "CREATE SECURE TEMPORARY VIEW v AS SELECT 1"
    );
}

#[test]
fn secure_is_only_a_snowflake_view_modifier() {
    assert!(rejected_in::<Core>("CREATE SECURE VIEW v AS SELECT 1"));
    assert!(rejected_in::<Postgres>("CREATE SECURE VIEW v AS SELECT 1"));
}

#[test]
fn plain_views_stay_core_nodes() {
    for sql in [
        "CREATE VIEW v AS SELECT 1",
        "CREATE OR REPLACE TEMPORARY VIEW v (a, b) AS SELECT 1, 2",
        "CREATE MATERIALIZED VIEW IF NOT EXISTS v AS SELECT 1",
    ] {
        round_trips(sql);
        assert!(
            matches!(
                parse_sql::<Snowflake>(sql).unwrap().remove(0).kind,
                StatementKind::CreateView(_)
            ),
            "{sql}"
        );
    }
}

#[test]
fn view_clauses_round_trip() {
    for sql in [
        "CREATE OR REPLACE VIEW bla COPY GRANTS AS (SELECT * FROM source)",
        "CREATE OR REPLACE SECURE VIEW bla COPY GRANTS AS (SELECT * FROM source)",
        "CREATE OR REPLACE VIEW bla COPY GRANTS (a, b) AS (SELECT a, b FROM source)",
        "CREATE OR REPLACE VIEW v COMMENT = 'hello, world' AS SELECT 1",
        "CREATE OR REPLACE VIEW v (a) COMMENT = 'Comment' AS SELECT a FROM t",
        "CREATE MATERIALIZED VIEW v CLUSTER BY (a, b) AS SELECT a, b FROM t",
        "CREATE VIEW v IF NOT EXISTS AS SELECT 1",
    ] {
        round_trips(sql);
    }
    assert_eq!(
        renders("CREATE OR REPLACE VIEW v (a, b) COPY GRANTS AS SELECT a, b FROM t"),
        "CREATE OR REPLACE VIEW v COPY GRANTS (a, b) AS SELECT a, b FROM t"
    );
    assert_eq!(
        renders("CREATE MATERIALIZED VIEW v (a) COPY GRANTS AS SELECT a FROM t"),
        "CREATE MATERIALIZED VIEW v COPY GRANTS (a) AS SELECT a FROM t"
    );
    assert!(rejected("CREATE OR REPLACE VIEW v COMMENT 'x' AS SELECT 1"));
    assert!(rejected("CREATE VIEW v COPY AS SELECT 1"));
    assert!(rejected("CREATE VIEW AS SELECT 1"));
}

#[test]
fn view_columns_take_options() {
    for sql in [
        "CREATE OR REPLACE VIEW v (a COMMENT 'Comment', b) AS SELECT a, b FROM table1",
        "CREATE OR REPLACE VIEW v (a COMMENT 'a comment', b, c COMMENT 'c comment') COMMENT = 'Comment' AS SELECT a FROM t",
        "CREATE VIEW X (COL WITH TAG (pii='email') COMMENT 'foobar') AS SELECT * FROM Y",
        "CREATE VIEW X (COL WITH TAG (foo.bar.baz.pii='email')) AS SELECT * FROM Y",
        "CREATE VIEW X (COL WITH MASKING POLICY foo.bar.baz) AS SELECT * FROM Y",
        "CREATE VIEW v (a WITH MASKING POLICY p USING (a, b), c PROJECTION POLICY q, d TAG (t='v')) AS SELECT 1",
    ] {
        round_trips(sql);
    }
    assert_eq!(
        renders(
            "CREATE OR REPLACE TEMPORARY VIEW foo.bar.baz (\"COL_1\" COMMENT $$comment 1$$) \
             COMMENT = $$view comment$$ AS (SELECT 1)"
        ),
        "CREATE OR REPLACE TEMPORARY VIEW foo.bar.baz (\"COL_1\" COMMENT 'comment 1') \
         COMMENT = 'view comment' AS (SELECT 1)"
    );
    assert!(rejected("CREATE VIEW v (a NOT NULL) AS SELECT 1"));
    assert!(rejected("CREATE VIEW v (a INT) AS SELECT 1"));
}

#[test]
fn view_ast_holds_each_clause() {
    let sql =
        "CREATE OR REPLACE SECURE VIEW v (a COMMENT 'x') COPY GRANTS COMMENT = 'y' AS SELECT 1";
    let StatementKind::Ext(BuiltinStatement::Snowflake(SnowflakeStatement::CreateView(view))) =
        parse_sql::<Snowflake>(sql).unwrap().remove(0).kind
    else {
        panic!("not a Snowflake view");
    };
    assert!(view.or_replace && view.secure && view.copy_grants);
    assert!(!view.materialized && !view.temporary);
    assert_eq!(view.columns.len(), 1);
    assert_eq!(view.columns[0].name.text, "a");
    assert_eq!(view.columns[0].options.len(), 1);
    assert_eq!(view.comment.as_ref().map(|c| c.text.as_ref()), Some("y"));
}

// Names

fn parts_of(sql: &str) -> Vec<String> {
    let statement = parse_sql::<Snowflake>(sql).unwrap().remove(0);
    let name = match statement.kind {
        StatementKind::CreateTable(create) => create.name,
        StatementKind::CreateDatabase(create) => create.name,
        StatementKind::CreateSchema(create) => create.name.unwrap(),
        other => panic!("unexpected statement: {other:?}"),
    };
    name.parts
        .iter()
        .map(|part| part.text.to_string())
        .collect()
}

#[test]
fn identifier_calls_are_object_names() {
    for sql in [
        "CREATE DATABASE IDENTIFIER('tbl')",
        "CREATE SCHEMA IDENTIFIER('db1.sc1')",
        "CREATE TABLE IDENTIFIER('tbl') (id INT)",
        "CREATE TABLE db.IDENTIFIER('tbl') (id INT)",
        "CREATE TABLE IDENTIFIER($t).x (id INT)",
        "DROP TABLE IDENTIFIER('t')",
        "ALTER TABLE IDENTIFIER('t') DROP COLUMN a",
        "INSERT INTO IDENTIFIER('t') VALUES (1)",
        "TRUNCATE TABLE IDENTIFIER('t')",
        "USE DATABASE IDENTIFIER('d')",
        "SELECT * FROM IDENTIFIER('t') AS x",
    ] {
        round_trips(sql);
    }
    assert_eq!(
        renders("CREATE TEMP FILE FORMAT identifier(?) TYPE=PARQUET COMPRESSION=auto"),
        "CREATE TEMPORARY FILE FORMAT identifier(?) TYPE=PARQUET COMPRESSION=auto"
    );
    assert_eq!(
        parts_of("CREATE TABLE db.IDENTIFIER('tbl') (id INT)"),
        ["db", "IDENTIFIER('tbl')"]
    );
    assert_eq!(
        parts_of("CREATE DATABASE IDENTIFIER('d')"),
        ["IDENTIFIER('d')"]
    );
}

#[test]
fn only_one_part_of_a_name_is_an_identifier_call() {
    assert!(rejected(
        "CREATE TABLE IDENTIFIER('db1').IDENTIFIER('sc1').IDENTIFIER('tbl') (id INT)"
    ));
    assert!(rejected(
        "CREATE TABLE IDENTIFIER('db1')..IDENTIFIER('tbl') (id INT)"
    ));
}

#[test]
fn identifier_is_otherwise_an_ordinary_word() {
    round_trips("CREATE TABLE identifier (id INT)");
    round_trips("SELECT identifier('email') FROM customers");
    assert!(rejected_in::<Core>(
        "CREATE TABLE IDENTIFIER('tbl') (id INT)"
    ));
    assert!(rejected_in::<Postgres>("CREATE DATABASE IDENTIFIER('tbl')"));
}

#[test]
fn identifier_wildcards_are_select_items() {
    round_trips("SELECT identifier('alias1').* FROM tbl AS alias1");
    round_trips("SELECT IDENTIFIER($a).* EXCLUDE (b) FROM t");
    let statement = parse_sql::<Snowflake>("SELECT identifier('t').* FROM t")
        .unwrap()
        .remove(0);
    let StatementKind::Query(query) = statement.kind else {
        panic!("not a query");
    };
    assert!(matches!(
        common::body(&query).projection.as_slice(),
        [SelectItem { kind: SelectItemKind::QualifiedWildcard(name), .. }] if name.parts[0].text == "identifier('t')"
    ));
    assert!(rejected("SELECT identifier('alias1').*.* FROM t"));
}

#[test]
fn double_dots_work_in_any_object_name() {
    round_trips("CREATE TABLE db..t (x INT)");
    round_trips("SELECT * FROM db..t");
    assert!(rejected("SELECT * FROM X..Y..Z"));
    assert!(rejected("SELECT * FROM .X.Y"));
    assert!(rejected_in::<Core>("CREATE TABLE db..t (x INT)"));
}

// Function arguments

#[test]
fn bare_subquery_arguments_round_trip() {
    for sql in [
        "SELECT parse_json(SELECT '{}')",
        "SELECT parse_json(WITH q AS (SELECT '{}' AS foo) SELECT foo FROM q)",
        "SELECT func(SELECT 1, 2)",
        "SELECT func(SELECT 1 UNION SELECT 2)",
    ] {
        round_trips(sql);
    }
    round_trips("SELECT func((SELECT 1))");
}

#[test]
fn bare_subquery_is_the_only_argument() {
    assert!(rejected("SELECT func(a, SELECT 1)"));
    assert!(rejected("SELECT func(DISTINCT SELECT 1)"));
    assert!(rejected_in::<Core>("SELECT func(SELECT 1)"));
}

#[test]
fn bare_subquery_argument_is_a_snowflake_expression() {
    let statement = parse_sql::<Snowflake>("SELECT func(SELECT 1)")
        .unwrap()
        .remove(0);
    let StatementKind::Query(query) = statement.kind else {
        panic!("not a query");
    };
    let [
        SelectItem {
            kind: SelectItemKind::Expr { expr, .. },
            ..
        },
    ] = common::body(&query).projection.as_slice()
    else {
        panic!("one item");
    };
    let ExprKind::Function { args, .. } = &expr.kind else {
        panic!("not a call");
    };
    assert!(matches!(
        args.as_slice(),
        [FunctionArg::Expr(Expr {
            kind: ExprKind::Ext(BuiltinExpr::Snowflake(SnowflakeExpr::BareSubquery(_))),
            ..
        })]
    ));
}

#[test]
fn null_treatment_can_be_written_inside_the_call() {
    round_trips("SELECT FIRST_VALUE(column2 IGNORE NULLS) OVER (ORDER BY a) FROM t");
    round_trips("SELECT LAST_VALUE(a RESPECT NULLS) OVER (ORDER BY a) FROM t");
    round_trips("SELECT FIRST_VALUE(column2 IGNORE NULLS)");
}

#[test]
fn extract_takes_any_part() {
    round_trips("SELECT EXTRACT(eod FROM d)");
    round_trips("SELECT EXTRACT('hour' FROM d)");
    round_trips("SELECT EXTRACT(HOUR, d)");
    assert!(rejected_in::<MySql>("SELECT EXTRACT(eod FROM d)"));
}

#[test]
fn lambdas_round_trip() {
    round_trips("SELECT FILTER(a, x -> x > 1) FROM t");
    round_trips("SELECT TRANSFORM(a, (x, y) -> x + y) FROM t");
    round_trips("SELECT FILTER(a, x INT -> x > 1) FROM t");
    assert!(rejected_in::<Core>("SELECT FILTER(a, x -> x > 1) FROM t"));
}

#[test]
fn object_constants_round_trip() {
    round_trips("SELECT {'a': 1, 'b': 2}");
    round_trips("SELECT {'a': {'b': 1}}");
    assert_eq!(renders("SELECT {'a':1, 'b':c}"), "SELECT {'a': 1, 'b': c}");
    round_trips("SELECT {}");
    assert!(rejected("SELECT {'a'}"));
    assert!(rejected_in::<Core>("SELECT {'a': 1}"));
}

// TOP, joins and table factors

#[test]
fn top_round_trips() {
    round_trips("SELECT TOP 4 c1 FROM testtable");
    round_trips("SELECT DISTINCT TOP 4 c1 FROM t");
    assert!(rejected("SELECT TOP 4 DISTINCT c1 FROM t"));
    round_trips("SELECT top FROM t");
}

#[test]
fn order_by_all_and_group_by_all_round_trip() {
    round_trips("SELECT * FROM t ORDER BY ALL");
    round_trips("SELECT * FROM t ORDER BY ALL DESC NULLS LAST");
    round_trips("SELECT a FROM t GROUP BY ALL");
    assert!(rejected_in::<Core>("SELECT a FROM t GROUP BY ALL"));
}

#[test]
fn values_as_a_table_round_trip() {
    round_trips("SELECT * FROM VALUES (1, 2), (3, 4)");
    round_trips("SELECT * FROM VALUES (1), (2) AS t (a), b");
}

#[test]
fn execute_immediate_round_trips() {
    round_trips("EXECUTE IMMEDIATE 'SELECT 1'");
    round_trips("EXECUTE IMMEDIATE foo");
    round_trips("EXECUTE IMMEDIATE $$SELECT 1$$");
    assert!(rejected_in::<Core>("EXECUTE IMMEDIATE 'SELECT 1'"));
}

#[test]
fn nested_joins_may_omit_parentheses() {
    assert_eq!(
        renders("SELECT * FROM a JOIN b JOIN c ON b.x = c.x ON a.x = b.x"),
        "SELECT * FROM a JOIN (b JOIN c ON b.x = c.x) ON a.x = b.x"
    );
    assert_eq!(
        renders("SELECT * FROM a JOIN b JOIN c ON x"),
        "SELECT * FROM a JOIN (b JOIN c ON x)"
    );
    round_trips("SELECT * FROM a JOIN b ON x JOIN c ON y");
    round_trips("SELECT * FROM a CROSS JOIN b JOIN c ON x");
    round_trips("SELECT * FROM a NATURAL JOIN b JOIN c ON x");
}

#[test]
fn semantic_views_round_trip() {
    for sql in [
        "SELECT * FROM SEMANTIC_VIEW(model)",
        "SELECT * FROM SEMANTIC_VIEW(a.b METRICS c.d, c.e)",
        "SELECT * FROM SEMANTIC_VIEW(model FACTS fact1, fact2)",
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS DATE_PART('year', col))",
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1 METRICS met1)",
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1 WHERE x > 0)",
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1) AS sv",
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim.* METRICS orders.*)",
    ] {
        round_trips(sql);
    }
    for (sql, canonical) in [
        (
            "SELECT * FROM SEMANTIC_VIEW(model WHERE x > 0 DIMENSIONS dim1)",
            "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1 WHERE x > 0)",
        ),
        (
            "SELECT * FROM SEMANTIC_VIEW(model METRICS met1 DIMENSIONS dim1)",
            "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1 METRICS met1)",
        ),
        (
            "SELECT * FROM SEMANTIC_VIEW(model FACTS fact1 DIMENSIONS dim1)",
            "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1 FACTS fact1)",
        ),
    ] {
        assert_eq!(renders(sql), canonical);
    }
    assert!(rejected(
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS dim1 INVALID inv1)"
    ));
    assert!(rejected(
        "SELECT * FROM SEMANTIC_VIEW(model DIMENSIONS a DIMENSIONS b)"
    ));
    round_trips("SELECT * FROM semantic_view");
    assert!(rejected_in::<Core>(
        "SELECT * FROM SEMANTIC_VIEW(m DIMENSIONS a)"
    ));
}

// ALTER

#[test]
fn alter_iceberg_round_trips() {
    for sql in [
        "ALTER ICEBERG TABLE tbl DROP CLUSTERING KEY",
        "ALTER ICEBERG TABLE tbl RESUME RECLUSTER",
        "ALTER ICEBERG TABLE tbl SUSPEND RECLUSTER",
        "ALTER ICEBERG TABLE IF EXISTS tbl CLUSTER BY (a)",
        "ALTER ICEBERG TABLE tbl ADD COLUMN a INT",
    ] {
        round_trips(sql);
    }
    let sql = "ALTER ICEBERG TABLE tbl DROP CLUSTERING KEY";
    let StatementKind::Ext(BuiltinStatement::Snowflake(SnowflakeStatement::AlterIcebergTable(
        alter,
    ))) = parse_sql::<Snowflake>(sql).unwrap().remove(0).kind
    else {
        panic!("not an ALTER ICEBERG TABLE");
    };
    assert_eq!(alter.name.parts[0].text, "tbl");
    assert!(matches!(
        alter.operations.as_slice(),
        [AlterTableOp::Phrase(std::borrow::Cow::Borrowed(
            "DROP CLUSTERING KEY"
        ))]
    ));
    assert!(rejected("ALTER ICEBERG TABLE ONLY tbl DROP CLUSTERING KEY"));
    assert!(rejected("ALTER ICEBERG tbl DROP CLUSTERING KEY"));
    assert!(rejected_in::<Core>(
        "ALTER ICEBERG TABLE t DROP CLUSTERING KEY"
    ));
}

#[test]
fn drop_lists_several_columns() {
    round_trips("ALTER TABLE t DROP COLUMN a, b");
    round_trips("ALTER TABLE t DROP COLUMN IF EXISTS a, b CASCADE");
    round_trips("ALTER TABLE t DROP a, b");
    round_trips("ALTER TABLE t DROP COLUMN a, ADD COLUMN b INT");
    round_trips("ALTER TABLE t DROP COLUMN a, DROP COLUMN b");
    let sql = "ALTER TABLE t DROP COLUMN a, b";
    let StatementKind::AlterTable(alter) = parse_sql::<Snowflake>(sql).unwrap().remove(0).kind
    else {
        panic!("not an ALTER TABLE");
    };
    assert!(matches!(
        alter.operations.as_slice(),
        [AlterTableOp::DropColumns { names, .. }] if names.len() == 2
    ));
    assert!(rejected_in::<Core>("ALTER TABLE t DROP COLUMN a, b"));
}

#[test]
fn drop_primary_and_foreign_keys() {
    round_trips("ALTER TABLE tbl DROP PRIMARY KEY");
    round_trips("ALTER TABLE tbl DROP PRIMARY KEY CASCADE");
    round_trips("ALTER TABLE tbl DROP FOREIGN KEY k1");
    round_trips("ALTER TABLE tbl DROP FOREIGN KEY k1 RESTRICT");
    round_trips("ALTER TABLE t ADD COLUMN IF NOT EXISTS a INT");
    assert!(rejected_in::<Core>("ALTER TABLE tbl DROP PRIMARY KEY"));
    assert!(rejected_in::<Core>("ALTER TABLE tbl DROP FOREIGN KEY k1"));
}

// Keywords as names

#[test]
fn clause_keywords_are_names_when_nothing_starts_the_clause() {
    for kw in ["CLUSTER", "FETCH", "RETURNING", "LIMIT", "EXCEPT", "SORT"] {
        round_trips(&format!("SELECT 1, {kw}"));
        assert_eq!(
            renders(&format!("SELECT 1 {kw}")),
            format!("SELECT 1 AS {kw}")
        );
    }
    for kw in [
        "LIMIT",
        "OFFSET",
        "EXCEPT",
        "OUTER",
        "WINDOW",
        "PARTITION",
        "FETCH",
    ] {
        round_trips(&format!("SELECT * FROM tbl AS {kw}"));
        round_trips(&format!("SELECT * FROM tbl {kw}"));
    }
    round_trips("SELECT * FROM tbl, LIMIT");
    assert_eq!(renders("SELECT 1 OFFSET"), "SELECT 1 AS OFFSET");
    round_trips("SELECT * FROM tbl LIMIT WHERE a = 1");
}

#[test]
fn clause_keywords_still_start_clauses() {
    round_trips("SELECT * FROM tbl LIMIT 1");
    round_trips("SELECT * FROM tbl LIMIT $1");
    round_trips("SELECT * FROM tbl LIMIT ''");
    round_trips("SELECT * FROM tbl LIMIT NULL");
    round_trips("SELECT * FROM tbl LIMIT $$$$");
    round_trips("SELECT 1 LIMIT 5");
    round_trips("SELECT 1 OFFSET 5 ROWS");
    round_trips("SELECT * FROM tbl OFFSET 5");
    round_trips("SELECT 1 FETCH FIRST 10 ROWS ONLY");
    assert_eq!(
        renders("SELECT 1 FETCH 10"),
        "SELECT 1 FETCH FIRST 10 ROWS ONLY"
    );
    round_trips("SELECT 1 EXCEPT SELECT 2");
    round_trips("SELECT * FROM t EXCEPT SELECT 2 FROM u");
    assert_eq!(
        renders("SELECT * FROM tbl, LIMIT 1"),
        "SELECT * FROM tbl LIMIT 1"
    );
    for kw in [
        "FROM", "GROUP", "HAVING", "ORDER", "SELECT", "UNION", "WHERE", "WITH",
    ] {
        assert!(rejected(&format!("SELECT * FROM tbl {kw}")), "{kw}");
    }
}

// Structured types

#[test]
fn structured_types_round_trip() {
    for sql in [
        "CREATE TABLE t (a ARRAY(VARCHAR))",
        "CREATE TABLE t (a ARRAY(VARCHAR NOT NULL))",
        "SELECT CAST(a AS ARRAY(NUMBER(10, 2))) FROM t",
        "SELECT CAST(a AS ARRAY(VARCHAR NOT NULL)) FROM t",
        "SELECT payload::OBJECT(address OBJECT(city VARCHAR NOT NULL), zip NUMBER) FROM t",
        "SELECT payload::OBJECT(tags ARRAY, labels MAP(VARCHAR, VARCHAR)) FROM t",
        "SELECT payload::OBJECT(items ARRAY(NUMBER NOT NULL), meta MAP(VARCHAR, OBJECT(k NUMBER) NOT NULL)) FROM t",
        "SELECT payload::ARRAY(NUMBER) FROM t",
        "SELECT payload::MAP(VARCHAR, OBJECT(k NUMBER)) FROM t",
        "SELECT payload::ARRAY(NUMBER NOT NULL) FROM t",
        "SELECT payload::MAP(VARCHAR, NUMBER NOT NULL) FROM t",
        "SELECT CAST(payload AS OBJECT(city VARCHAR, zip NUMBER NOT NULL)) FROM t",
        "CREATE TABLE t (o OBJECT())",
        "CREATE TABLE t (o OBJECT)",
        "CREATE TABLE t (a ARRAY)",
    ] {
        round_trips(sql);
    }
    assert!(rejected("CREATE TABLE t (o OBJECT(city VARCHAR)"));
    assert!(rejected("SELECT a::MAP(VARCHAR) FROM t"));
    assert!(rejected("SELECT a::ARRAY() FROM t"));
    assert!(rejected_in::<Core>(
        "SELECT a::ARRAY(VARCHAR NOT NULL) FROM t"
    ));
}

#[test]
fn object_fields_keep_their_names() {
    let sql = "SELECT CAST(p AS OBJECT(city VARCHAR, zip NUMBER NOT NULL)) FROM t";
    let statement = parse_sql::<Snowflake>(sql).unwrap().remove(0);
    let StatementKind::Query(query) = statement.kind else {
        panic!("not a query");
    };
    let [
        SelectItem {
            kind:
                SelectItemKind::Expr {
                    expr:
                        Expr {
                            kind: ExprKind::Cast { ty, .. },
                            ..
                        },
                    ..
                },
            ..
        },
    ] = common::body(&query).projection.as_slice()
    else {
        panic!("one cast");
    };
    let Some(AngleArguments::ParenFields(fields)) = ty.angle.as_deref() else {
        panic!("not a structured type");
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name.as_ref().unwrap().text, "city");
    assert!(!fields[0].not_null);
    assert_eq!(fields[1].name.as_ref().unwrap().text, "zip");
    assert!(fields[1].not_null);
}

// Remaining statements

#[test]
fn insert_overwrite_round_trips() {
    round_trips("INSERT OVERWRITE INTO schema.table SELECT a FROM b");
    round_trips("INSERT OVERWRITE INTO t VALUES (1)");
    assert!(rejected("INSERT OVERWRITE t SELECT 1"));
    assert!(rejected_in::<Core>("INSERT OVERWRITE INTO t SELECT 1"));
}

#[test]
fn create_warehouse_round_trips() {
    round_trips("CREATE WAREHOUSE my_wh");
    round_trips("CREATE WAREHOUSE my_wh WAREHOUSE_SIZE='XSMALL' AUTO_SUSPEND=60");
    round_trips("CREATE OR REPLACE WAREHOUSE IF NOT EXISTS w WAREHOUSE_SIZE='XSMALL'");
    assert_eq!(
        renders("CREATE WAREHOUSE my_wh WITH WAREHOUSE_SIZE = 'XSMALL' AUTO_SUSPEND = 60"),
        "CREATE WAREHOUSE my_wh WAREHOUSE_SIZE='XSMALL' AUTO_SUSPEND=60"
    );
    assert!(rejected("CREATE TEMPORARY WAREHOUSE w"));
}

#[test]
fn database_contacts_can_be_strings() {
    assert_eq!(
        renders("CREATE DATABASE d WITH CONTACT (owner = 'admin', dpo = compliance)"),
        "CREATE DATABASE d WITH CONTACT (owner = admin, dpo = compliance)"
    );
}
