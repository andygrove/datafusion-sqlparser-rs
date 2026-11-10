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

//! Syntax that several dialects share, each opted into through a hook. Every
//! construct is tested in the dialects that have it, and rejected in some that
//! do not.

use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, DialectKind, parse_sql_as};
use sqlparser::parse_sql;

use DialectKind::{
    BigQuery, ClickHouse, Core, Databricks, DuckDb, Hive, MsSql, MySql, Oracle, Postgres, Redshift,
    Snowflake, Spark, Sqlite, Teradata,
};

/// `sql` parses and renders back as written in each of `kinds`.
fn accepts(kinds: &[DialectKind], sql: &str) {
    renders(kinds, sql, sql);
}

/// `sql` parses and renders as `expected` in each of `kinds`.
fn renders(kinds: &[DialectKind], sql: &str, expected: &str) {
    for &kind in kinds {
        let statements =
            parse_sql_as(kind, sql).unwrap_or_else(|e| panic!("{kind:?} rejected {sql}: {e}"));
        let rendered: Vec<String> = statements.iter().map(ToString::to_string).collect();
        assert_eq!(rendered.join("; "), expected, "rendering in {kind:?}");
    }
}

fn rejects(kinds: &[DialectKind], sql: &str) {
    for &kind in kinds {
        assert!(
            parse_sql_as(kind, sql).is_err(),
            "{kind:?} accepted {sql}: {:?}",
            parse_sql_as(kind, sql).map(|s| s[0].to_string()),
        );
    }
}

fn statement<D: sqlparser::dialect::Dialect<Ext = Builtin>>(sql: &str) -> Statement<'_, Builtin> {
    parse_sql::<D>(sql).unwrap().remove(0)
}

fn select_of<'a>(statement: &'a Statement<'a, Builtin>) -> &'a Select<'a, Builtin> {
    match &statement.kind {
        StatementKind::Query(query) => match &query.body {
            SetExpr::Select(select) => select,
            other => panic!("not a select: {other:?}"),
        },
        other => panic!("not a query: {other:?}"),
    }
}

const NO_UPDATE_FROM: [DialectKind; 6] = [Core, Spark, Databricks, Hive, ClickHouse, MySql];

#[test]
fn update_from_after_set() {
    let sql = "UPDATE t SET a = b FROM u, (SELECT foo FROM v) AS w WHERE 1 = 1";
    accepts(
        &[
            Postgres, Redshift, MsSql, DuckDb, Snowflake, BigQuery, Oracle, Sqlite,
        ],
        sql,
    );
    rejects(&NO_UPDATE_FROM, sql);
    rejects(&[Teradata], sql);
    let StatementKind::Update(update) =
        statement::<sqlparser::dialect::Postgres>("UPDATE t SET a = 1 FROM u WHERE a = 2").kind
    else {
        panic!("not an update");
    };
    assert_eq!(update.from.len(), 1);
    assert!(!update.from_before_set);
}

#[test]
fn update_from_before_set_is_teradata() {
    let sql = "UPDATE t1 FROM u, (SELECT id FROM v) AS w SET a = b WHERE 1 = 1";
    accepts(&[Teradata], sql);
    rejects(&[Postgres, Snowflake, BigQuery, MsSql, Core], sql);
    let StatementKind::Update(update) =
        statement::<sqlparser::dialect::Teradata>("UPDATE t FROM u SET a = 1").kind
    else {
        panic!("not an update");
    };
    assert_eq!(update.from.len(), 1);
    assert!(update.from_before_set);
}

#[test]
fn delete_using() {
    let sql = "DELETE FROM basket AS a USING basket AS b WHERE a.id < b.id";
    accepts(&[Postgres, Redshift, DuckDb, Snowflake, MySql], sql);
    rejects(
        &[Core, Oracle, MsSql, Sqlite, BigQuery, Spark, Teradata],
        sql,
    );
    let StatementKind::Delete(delete) =
        statement::<sqlparser::dialect::Postgres>("DELETE FROM a USING b WHERE a.i = b.i").kind
    else {
        panic!("not a delete");
    };
    assert_eq!(delete.using.len(), 1);
    assert!(delete.multi.is_none());
}

#[test]
fn multi_table_delete_is_mysql() {
    let using = "DELETE FROM s.t1, s.t2 USING s.t1 JOIN s.t2 ON s.t2.pk = s.t1.pk WHERE s.t1.a = 1";
    let from = "DELETE s.t1, s.t2 FROM s.t1 JOIN s.t2 ON s.t2.pk = s.t1.pk WHERE s.t2.a = 1";
    accepts(&[MySql], using);
    accepts(&[MySql], from);
    rejects(&[Postgres, Core, Snowflake, Oracle], using);
    rejects(&[Postgres, Core, MsSql, Oracle], from);
    let StatementKind::Delete(delete) = statement::<sqlparser::dialect::MySql>(from).kind else {
        panic!("not a delete");
    };
    let multi = delete.multi.expect("multi-table form");
    assert_eq!(multi.targets.len(), 2);
    assert!(multi.from_keyword);
    assert_eq!(delete.using.len(), 1);
}

#[test]
fn select_into_table() {
    accepts(
        &[Postgres, Redshift, MsSql],
        "SELECT * INTO table0 FROM table1",
    );
    accepts(
        &[Postgres, Redshift],
        "SELECT a INTO TEMPORARY TABLE t FROM s",
    );
    accepts(&[Postgres], "SELECT * INTO UNLOGGED t FROM s");
    rejects(
        &[Postgres],
        "SELECT * INTO TEMPORARY UNLOGGED TABLE t FROM s",
    );
    rejects(
        &[Core, DuckDb, Spark, BigQuery, Sqlite, ClickHouse, Hive],
        "SELECT * INTO table0 FROM table1",
    );
    let tree = statement::<sqlparser::dialect::Postgres>("SELECT a INTO TEMP TABLE t FROM s");
    let into = select_of(&tree).into.as_deref().expect("an INTO clause");
    assert_eq!(into.modifier.as_deref(), Some("TEMP"));
    assert!(into.table_keyword);
    assert_eq!(into.targets.len(), 1);
}

#[test]
fn select_into_variables() {
    accepts(
        &[Oracle, MySql, Snowflake, Teradata],
        "SELECT a, b INTO p, q FROM t",
    );
    accepts(
        &[Oracle, MySql, Snowflake, Teradata],
        "SELECT a, b INTO foo.bar, bar.baz FROM t",
    );
    accepts(
        &[Snowflake, Teradata, Oracle],
        "SELECT a, b INTO :h1, :h2 FROM t",
    );
    rejects(
        &[Core, DuckDb, Spark, BigQuery],
        "SELECT a, b INTO p, q FROM t",
    );
    // A single table is all that SELECT INTO can create in these engines.
    rejects(&[Postgres, MsSql, Redshift], "SELECT a, b INTO p, q FROM t");
}

#[test]
fn is_json_predicate() {
    for sql in [
        "SELECT a IS JSON",
        "SELECT a IS NOT JSON",
        "SELECT a IS JSON VALUE",
        "SELECT a IS JSON ARRAY",
        "SELECT a IS JSON OBJECT",
        "SELECT a IS JSON SCALAR",
        "SELECT a IS JSON WITH UNIQUE",
        "SELECT a IS JSON WITH UNIQUE KEYS",
        "SELECT a IS JSON WITHOUT UNIQUE",
        "SELECT a IS NOT JSON OBJECT WITHOUT UNIQUE KEYS",
    ] {
        accepts(&[Postgres, Oracle, MsSql], sql);
        rejects(
            &[
                Core, MySql, DuckDb, Snowflake, Spark, Hive, BigQuery, ClickHouse, Teradata,
                Redshift,
            ],
            sql,
        );
    }
    let tree = statement::<sqlparser::dialect::Postgres>("SELECT a IS NOT JSON ARRAY WITH UNIQUE");
    let SelectItemKind::Expr {
        expr: Expr {
            kind: ExprKind::Is { test, negated, .. },
            ..
        },
        ..
    } = &select_of(&tree).projection[0].kind
    else {
        panic!("not IS");
    };
    assert!(*negated);
    assert_eq!(
        test,
        &IsTest::Json {
            kind: Some(JsonPredicateType::Array),
            unique_keys: Some(JsonUniqueKeys {
                with: true,
                keys: false,
            }),
        }
    );
}

#[test]
fn is_normalized_predicate() {
    for sql in [
        "SELECT a IS NORMALIZED",
        "SELECT a IS NOT NORMALIZED",
        "SELECT a IS NFC NORMALIZED",
        "SELECT a IS NOT NFD NORMALIZED",
        "SELECT a IS NFKC NORMALIZED",
        "SELECT a IS NOT NFKD NORMALIZED",
    ] {
        accepts(&[Postgres], sql);
        rejects(&[Core, Oracle, MsSql, MySql, DuckDb, Snowflake], sql);
    }
    let tree = statement::<sqlparser::dialect::Postgres>("SELECT a IS NFKD NORMALIZED");
    let SelectItemKind::Expr {
        expr: Expr {
            kind: ExprKind::Is { test, .. },
            ..
        },
        ..
    } = &select_of(&tree).projection[0].kind
    else {
        panic!("not IS");
    };
    assert_eq!(test, &IsTest::Normalized(Some(NormalForm::Nfkd)));
}

#[test]
fn xml_table() {
    for sql in [
        "SELECT * FROM XMLTABLE('/root' PASSING data COLUMNS element TEXT)",
        "SELECT * FROM XMLTABLE('//ROWS/ROW' PASSING data COLUMNS row_num FOR ORDINALITY, id INT PATH '@id', name TEXT PATH 'NAME' DEFAULT 'unnamed')",
        "SELECT y FROM XMLTABLE('/X' PASSING '<X></X>' COLUMNS y TEXT NOT NULL)",
        "SELECT y FROM XMLTABLE((SELECT '/X') PASSING (SELECT CAST('<X><y>z</y></X>' AS xml)) COLUMNS y TEXT PATH (SELECT 'y'))",
        "SELECT xmltable.* FROM XMLTABLE(XMLNAMESPACES('http://example.com/myns' AS x, 'http://example.com/b' AS \"B\"), '/x:example' PASSING data COLUMNS a INT PATH '@a')",
    ] {
        accepts(&[Postgres, Oracle, Teradata], sql);
        rejects(
            &[Core, MySql, MsSql, DuckDb, Snowflake, BigQuery, Spark],
            sql,
        );
    }
    let without_passing =
        "SELECT X.* FROM T1, XMLTABLE('$C/customers' COLUMNS \"Cid\" BIGINT PATH '@Cid')";
    accepts(&[Oracle, Teradata], without_passing);
    rejects(&[Postgres], without_passing);
    let tree = statement::<sqlparser::dialect::Postgres>(
        "SELECT * FROM XMLTABLE('/r' PASSING BY VALUE d AS x COLUMNS a FOR ORDINALITY, b INT NOT NULL) AS t",
    );
    let select = select_of(&tree);
    let TableFactor::XmlTable(table) = &select.from[0].relation.factor else {
        panic!("not XMLTABLE");
    };
    assert_eq!(table.passing.len(), 1);
    assert!(table.passing[0].by_value);
    assert_eq!(table.columns.len(), 2);
    assert_eq!(table.columns[0].kind, XmlColumnKind::ForOrdinality);
}

#[test]
fn array_slices() {
    let slices = [
        "SELECT arr[1:2]",
        "SELECT arr[:]",
        "SELECT arr[:][:]",
        "SELECT arr[1:array_length(arr)]",
        "SELECT arr[array_length(arr) - 1:array_length(arr)]",
    ];
    for sql in slices {
        accepts(&[Postgres, DuckDb], sql);
        rejects(&[Core, BigQuery, Spark, ClickHouse, MySql, Oracle], sql);
    }
    // Only DuckDB has a step.
    accepts(&[DuckDb], "SELECT arr[1:2:4]");
    rejects(&[Postgres], "SELECT arr[1:2:4]");
    let tree = statement::<sqlparser::dialect::Postgres>("SELECT arr[1:2]");
    let SelectItemKind::Expr { expr, .. } = &select_of(&tree).projection[0].kind else {
        panic!("not an expression");
    };
    let ExprKind::Special(special) = &expr.kind else {
        panic!("not special: {expr:?}");
    };
    assert!(matches!(**special, SpecialExpr::Slice { step: None, .. }));
}

#[test]
fn listagg_on_overflow() {
    for sql in [
        "SELECT LISTAGG(dateid ON OVERFLOW ERROR)",
        "SELECT LISTAGG(dateid ON OVERFLOW TRUNCATE N'...' WITH COUNT)",
        "SELECT LISTAGG(dateid ON OVERFLOW TRUNCATE X'deadbeef' WITH COUNT)",
        "SELECT LISTAGG(DISTINCT dateid, ', ' ON OVERFLOW TRUNCATE '%' WITHOUT COUNT) WITHIN GROUP (ORDER BY id, username)",
    ] {
        accepts(&[Oracle], sql);
        rejects(&[Core, Postgres, MySql, MsSql, Snowflake, Redshift], sql);
    }
    let tree = statement::<sqlparser::dialect::Oracle>("SELECT LISTAGG(x ON OVERFLOW ERROR)");
    let SelectItemKind::Expr {
        expr: Expr {
            kind: ExprKind::Function { options, .. },
            ..
        },
        ..
    } = &select_of(&tree).projection[0].kind
    else {
        panic!("not a function");
    };
    let overflow = options.as_ref().unwrap().on_overflow.as_deref().unwrap();
    assert!(matches!(overflow, ListAggOnOverflow::Error));
}

#[test]
fn national_strings() {
    accepts(&[Oracle, Postgres], "SELECT N'national string'");
    renders(
        &[Oracle, Postgres],
        "SELECT n'national string'",
        "SELECT N'national string'",
    );
    let tree = statement::<sqlparser::dialect::Oracle>("SELECT N'x'");
    let SelectItemKind::Expr {
        expr:
            Expr {
                kind: ExprKind::Literal(Literal::String(literal)),
                ..
            },
        ..
    } = &select_of(&tree).projection[0].kind
    else {
        panic!("not a string");
    };
    assert!(literal.national);
    // The letter N before a string means nothing elsewhere.
    rejects(&[Core, Snowflake], "SELECT N'x'");
}

#[test]
fn set_operator_minus() {
    let sql = "SELECT 1 MINUS SELECT 2";
    accepts(
        &[
            Oracle, Snowflake, Redshift, Hive, Spark, Databricks, Teradata,
        ],
        sql,
    );
    rejects(
        &[
            Core, Postgres, MySql, MsSql, DuckDb, Sqlite, ClickHouse, BigQuery,
        ],
        sql,
    );
    accepts(
        &[Hive, Spark, Databricks, Teradata],
        "SELECT 1 MINUS ALL SELECT 2",
    );
    accepts(
        &[Hive, Spark, Databricks, Teradata],
        "SELECT 1 MINUS DISTINCT SELECT 2",
    );
}

#[test]
fn limit_all() {
    accepts(
        &[Postgres, Redshift, DuckDb, Spark, Databricks],
        "SELECT a FROM t LIMIT ALL",
    );
    accepts(
        &[Postgres, Redshift, DuckDb, Spark, Databricks],
        "SELECT a FROM t LIMIT ALL OFFSET 1",
    );
    rejects(
        &[Core, MySql, MsSql, Oracle, Snowflake, BigQuery, Hive],
        "SELECT a FROM t LIMIT ALL",
    );
    let tree = statement::<sqlparser::dialect::Postgres>("SELECT a FROM t LIMIT ALL");
    let StatementKind::Query(query) = &tree.kind else {
        panic!("not a query");
    };
    let limit = query.limit.as_deref().unwrap();
    assert!(limit.all);
    assert!(limit.count.is_none());
}

#[test]
fn all_as_a_name() {
    let sql = "SELECT id, ALL FROM customer WHERE id < 5 ORDER BY ALL";
    accepts(&[Spark, Databricks, ClickHouse], sql);
    rejects(
        &[
            Core, Postgres, MySql, MsSql, Oracle, Snowflake, Hive, BigQuery,
        ],
        sql,
    );
}

#[test]
fn order_by_all() {
    let sql = "SELECT a FROM t ORDER BY ALL DESC NULLS LAST";
    accepts(&[Spark, Databricks, Snowflake, DuckDb, ClickHouse], sql);
    rejects(&[Core, Postgres, MySql, Oracle, Hive, BigQuery], sql);
}

#[test]
fn distinct_parenthesized_wildcard() {
    renders(
        &[Spark, Databricks],
        "SELECT DISTINCT (*) FROM t",
        "SELECT DISTINCT * FROM t",
    );
    renders(
        &[Spark, Databricks],
        "SELECT DISTINCT(*) FROM t",
        "SELECT DISTINCT * FROM t",
    );
    rejects(
        &[Core, Postgres, MySql, Snowflake, DuckDb, Hive],
        "SELECT DISTINCT (*) FROM t",
    );
}

#[test]
fn cte_column_lists() {
    let sql = "WITH cte (col1, col2) AS (SELECT foo, bar FROM baz) SELECT * FROM cte";
    accepts(
        &[
            Postgres, MySql, Snowflake, Oracle, MsSql, Sqlite, DuckDb, Spark, Databricks, Teradata,
            Redshift, ClickHouse, Core,
        ],
        sql,
    );
    rejects(&[BigQuery, Hive], sql);
    let StatementKind::Query(query) = statement::<sqlparser::dialect::Postgres>(sql).kind else {
        panic!("not a query");
    };
    assert_eq!(query.ctes()[0].columns.len(), 2);
}

#[test]
fn with_before_dml() {
    let update = "WITH x AS (SELECT 1) UPDATE t SET bar = (SELECT * FROM x)";
    let delete = "WITH x AS (SELECT 1) DELETE FROM q WHERE id IN (SELECT x FROM x)";
    let insert = "WITH x AS (SELECT 42) INSERT INTO t SELECT foo FROM x";
    accepts(&[Postgres, DuckDb, Sqlite, MsSql], update);
    accepts(&[Postgres, DuckDb, Sqlite, MsSql], delete);
    accepts(&[Postgres, DuckDb, Sqlite, MsSql], insert);
    accepts(&[MySql, Redshift], update);
    accepts(&[MySql, Redshift], delete);
    rejects(&[MySql, Redshift], insert);
    accepts(&[Spark, Databricks, Hive], insert);
    rejects(&[Spark, Databricks, Hive], update);
    rejects(&[Core, Snowflake, Oracle, BigQuery, Teradata], update);
    rejects(&[Core, Snowflake, Oracle, BigQuery, Teradata], insert);
}

#[test]
fn insert_default_values() {
    let sql = "INSERT INTO t DEFAULT VALUES";
    accepts(&[Postgres, Redshift, MsSql, DuckDb, Sqlite], sql);
    rejects(&[Core, MySql, Oracle, Snowflake, BigQuery, Spark], sql);
    accepts(
        &[Postgres],
        "INSERT INTO t DEFAULT VALUES ON CONFLICT DO NOTHING",
    );
    accepts(&[Postgres], "INSERT INTO t DEFAULT VALUES RETURNING a");
}

#[test]
fn insert_without_into() {
    let sql = "INSERT customer VALUES (1, 2, 3)";
    accepts(&[MySql, MsSql, Teradata], sql);
    rejects(&[Core, Postgres, Oracle, Snowflake, Spark], sql);
}

#[test]
fn parenthesized_query_after_in() {
    let sql = "SELECT * FROM t WHERE a IN ((SELECT b FROM u) UNION (SELECT c FROM v))";
    accepts(&[Core, Postgres, MySql, MsSql, Snowflake, Spark], sql);
    let sql = "SELECT ((SELECT 1) UNION SELECT 2)";
    accepts(&[Core, Postgres], sql);
}

#[test]
fn group_by_empty_set() {
    let sql = "SELECT count(1) FROM t GROUP BY ()";
    accepts(
        &[
            Postgres, DuckDb, Oracle, Spark, Databricks, Teradata, BigQuery, Snowflake,
        ],
        sql,
    );
    accepts(
        &[Postgres, Oracle, Spark],
        "SELECT name, count(1) FROM t GROUP BY name, ()",
    );
    rejects(&[Core, MySql, Hive, Sqlite], sql);
}

#[test]
fn group_by_list_then_grouping_sets() {
    let sql = "SELECT a, b, SUM(c) FROM t GROUP BY a, b GROUPING SETS ((a, b), (a), (b), ())";
    accepts(&[Spark, Databricks, Hive], sql);
    rejects(&[Core, Postgres, MySql, Snowflake, Oracle], sql);
    let tree = statement::<sqlparser::dialect::Spark>(sql);
    let select = select_of(&tree);
    assert_eq!(select.group_by.len(), 2);
    assert!(select.grouping_sets.is_some());
}

#[test]
fn create_schema_authorization() {
    accepts(
        &[Postgres, Redshift, MsSql],
        "CREATE SCHEMA X AUTHORIZATION Y",
    );
    accepts(
        &[Postgres, Redshift, MsSql, Oracle],
        "CREATE SCHEMA AUTHORIZATION Y",
    );
    rejects(
        &[Core, MySql, Snowflake, BigQuery, Teradata],
        "CREATE SCHEMA X AUTHORIZATION Y",
    );
    rejects(&[Oracle], "CREATE SCHEMA X AUTHORIZATION Y");
    let StatementKind::CreateSchema(schema) =
        statement::<sqlparser::dialect::Postgres>("CREATE SCHEMA AUTHORIZATION Y").kind
    else {
        panic!("not CREATE SCHEMA");
    };
    assert!(schema.name.is_none());
    assert_eq!(schema.authorization.unwrap().text, "Y");
}

#[test]
fn create_or_replace_schema() {
    accepts(&[Snowflake, DuckDb], "CREATE OR REPLACE SCHEMA X");
    accepts(
        &[Snowflake, DuckDb],
        "CREATE OR REPLACE SCHEMA IF NOT EXISTS X",
    );
    rejects(
        &[Core, Postgres, MySql, Oracle, MsSql, Teradata],
        "CREATE OR REPLACE SCHEMA X",
    );
    accepts(&[Snowflake], "CREATE SCHEMA a CLONE b");
    rejects(&[Core, Postgres, DuckDb], "CREATE SCHEMA a CLONE b");
}

#[test]
fn alter_view() {
    accepts(
        &[MsSql, MySql, Spark, Databricks, Hive],
        "ALTER VIEW s.v AS SELECT foo FROM bar",
    );
    accepts(&[MsSql, MySql], "ALTER VIEW v (a, b) AS SELECT 1, 2");
    rejects(
        &[Spark, Databricks, Hive],
        "ALTER VIEW v (a, b) AS SELECT 1, 2",
    );
    rejects(
        &[Core, Postgres, Oracle, Snowflake, BigQuery],
        "ALTER VIEW v AS SELECT 1",
    );
    let StatementKind::AlterView(alter) =
        statement::<sqlparser::dialect::MySql>("ALTER VIEW v (a) AS SELECT 1").kind
    else {
        panic!("not ALTER VIEW");
    };
    assert_eq!(alter.columns.len(), 1);
}

#[test]
fn create_table_as_table() {
    let sql = "CREATE TABLE new_table AS TABLE old_table";
    accepts(&[Postgres, DuckDb, MySql], sql);
    accepts(&[Postgres, DuckDb, MySql], "CREATE TABLE n AS TABLE s.old");
    rejects(
        &[Core, Oracle, MsSql, Snowflake, Spark, Sqlite, Redshift],
        sql,
    );
    let StatementKind::CreateTable(create) = statement::<sqlparser::dialect::Postgres>(sql).kind
    else {
        panic!("not CREATE TABLE");
    };
    assert!(matches!(
        create.query.as_deref().unwrap().body,
        SetExpr::Table(_)
    ));
}

#[test]
fn create_table_like() {
    let sql = "CREATE TABLE new LIKE old";
    accepts(&[MySql, Spark, Databricks, Hive, Snowflake, BigQuery], sql);
    rejects(&[Core, Postgres, MsSql, Oracle, DuckDb, Teradata], sql);
}

#[test]
fn drop_type() {
    accepts(
        &[DuckDb, MsSql, Oracle, Teradata, Postgres],
        "DROP TYPE abc",
    );
    accepts(&[DuckDb, Postgres], "DROP TYPE IF EXISTS my_type CASCADE");
    rejects(
        &[Core, MySql, Snowflake, BigQuery, Spark, Hive],
        "DROP TYPE abc",
    );
}

#[test]
fn create_type_enum() {
    accepts(
        &[Postgres, DuckDb],
        "CREATE TYPE mood AS ENUM ('happy', 'sad')",
    );
    rejects(
        &[Core, MySql, Snowflake, Oracle, BigQuery],
        "CREATE TYPE mood AS ENUM ('happy', 'sad')",
    );
}

#[test]
fn collation_strings() {
    accepts(
        &[MySql],
        "CREATE TABLE t (b TEXT DEFAULT 'foo' COLLATE 'en_US')",
    );
    accepts(&[MySql], "SELECT name COLLATE \"de_DE\" FROM customer");
    accepts(
        &[MySql],
        "CREATE TABLE t (b TEXT CHARACTER SET utf8mb4 COLLATE \"de_DE\")",
    );
    rejects(
        &[Core, Postgres, Oracle],
        "CREATE TABLE t (b TEXT COLLATE 'x')",
    );
}

#[test]
fn invisible_columns() {
    let sql = "CREATE TABLE t (foo INT, bar INT INVISIBLE)";
    accepts(&[MySql, Oracle], sql);
    accepts(
        &[MySql, Oracle],
        "ALTER TABLE t ADD COLUMN bar INT INVISIBLE",
    );
    rejects(&[Core, Postgres, Snowflake, BigQuery], sql);
}

#[test]
fn grant_of_sets_in_group_by_is_not_core() {
    // A guard that opting a few dialects in has not leaked into the core.
    rejects(&[Core], "SELECT a FROM t GROUP BY a GROUPING SETS ((a))");
}

#[test]
fn qualify_clause() {
    let sql = "SELECT a FROM t QUALIFY ROW_NUMBER() OVER (PARTITION BY a ORDER BY b) = 1";
    accepts(
        &[
            Snowflake, BigQuery, DuckDb, Teradata, Databricks, ClickHouse,
        ],
        sql,
    );
    rejects(&[Core, Postgres, MySql, MsSql, Oracle, Hive, Spark], sql);
}

#[test]
fn like_over_a_list() {
    let sql = "SELECT * FROM x WHERE a LIKE ANY ('%x%', 'T%e')";
    accepts(&[Teradata, Snowflake, Spark, Databricks, Hive], sql);
    accepts(
        &[Spark],
        "SELECT * FROM x WHERE a NOT LIKE ALL ('%x%', 'T%e')",
    );
    let tree = statement::<sqlparser::dialect::Snowflake>("SELECT a LIKE ANY ('x')");
    let SelectItemKind::Expr {
        expr: Expr {
            kind: ExprKind::Like { pattern, .. },
            ..
        },
        ..
    } = &select_of(&tree).projection[0].kind
    else {
        panic!("not LIKE");
    };
    let ExprKind::Special(special) = &pattern.kind else {
        panic!("not a quantified list: {pattern:?}");
    };
    assert!(matches!(
        **special,
        SpecialExpr::QuantifiedList {
            quantifier: Quantifier::Any,
            ..
        }
    ));
}

#[test]
fn databricks_colon_paths() {
    accepts(&[Databricks, Snowflake], "SELECT a:b FROM t");
    accepts(&[Databricks], "SELECT a:b.c FROM t");
    rejects(&[Core, Spark, Hive, BigQuery], "SELECT a:b FROM t");
}

#[test]
fn execute_immediate() {
    for sql in [
        "EXECUTE IMMEDIATE 'SELECT 1'",
        "EXECUTE IMMEDIATE 'SELECT 1' INTO a USING 1 AS b",
    ] {
        accepts(&[Spark, Databricks, Snowflake, Oracle, BigQuery], sql);
        rejects(&[Core, Postgres, MySql, DuckDb, Hive], sql);
    }
}

#[test]
fn odbc_escapes() {
    for sql in [
        "SELECT {d '2025-07-17'}, category_name FROM categories",
        "SELECT {t '14:12:01'}",
        "SELECT {ts '2025-07-17 14:12:01'}",
        "SELECT {fn my_func(1, 2)}",
        "SELECT {fn fna()} AS foo, fnb(1)",
    ] {
        accepts(&[MsSql, MySql], sql);
        rejects(&[Core, Postgres, Snowflake, Oracle, BigQuery], sql);
    }
}

#[test]
fn left_and_right_as_functions() {
    accepts(
        &[Postgres, MySql, MsSql, Snowflake, BigQuery],
        "SELECT LEFT(x, 1), RIGHT(x, 2) FROM t",
    );
}

#[test]
fn clickhouse_right_semi_and_anti_joins() {
    accepts(
        &[ClickHouse],
        "SELECT * FROM t1 RIGHT SEMI JOIN t2 ON c1 = c2",
    );
    accepts(
        &[ClickHouse],
        "SELECT * FROM t1 RIGHT ANTI JOIN t2 USING(c1)",
    );
    rejects(
        &[Spark, Postgres, Core],
        "SELECT * FROM t1 RIGHT SEMI JOIN t2 ON c1 = c2",
    );
}

#[test]
fn abort_statement() {
    for sql in ["ABORT", "ABORT WORK", "ABORT TRANSACTION"] {
        renders(&[Postgres, Redshift, DuckDb], sql, "ABORT");
    }
    rejects(&[Core, MySql, Snowflake, Teradata], "ABORT");
}

#[test]
fn postgres_session_statements() {
    accepts(&[Postgres], "DISCARD ALL");
    accepts(&[Postgres], "DISCARD TEMP");
    accepts(
        &[Postgres, Redshift, DuckDb],
        "RESET some_extension.some_parameter",
    );
    accepts(&[Postgres, Redshift, DuckDb], "RESET ALL");
    accepts(&[Postgres], "RESET SESSION AUTHORIZATION");
    accepts(&[Postgres, Redshift], "CLOSE my_cursor");
    accepts(&[Postgres, Redshift], "CLOSE ALL");
    accepts(&[Postgres], "LOAD 'filename'");
    rejects(&[Core, MySql, Snowflake], "DISCARD ALL");
    rejects(&[Core, MySql, Oracle], "CLOSE my_cursor");
}

#[test]
fn postgres_row_security_policies() {
    accepts(
        &[Postgres],
        "CREATE POLICY p ON t AS PERMISSIVE FOR SELECT TO my_role, CURRENT_USER USING (c0 = 1) WITH CHECK (1 = 1)",
    );
    accepts(&[Postgres], "CREATE POLICY p ON t");
    accepts(&[Postgres], "ALTER POLICY p ON t RENAME TO q");
    accepts(
        &[Postgres],
        "ALTER POLICY p ON t TO CURRENT_USER USING ((SELECT c0)) WITH CHECK (c0 > 0)",
    );
    rejects(&[Core, MySql, Snowflake], "CREATE POLICY p ON t");
}

#[test]
fn alter_index_rename() {
    accepts(&[Postgres, Oracle], "ALTER INDEX idx RENAME TO new_idx");
    rejects(
        &[Core, MySql, Snowflake, BigQuery],
        "ALTER INDEX idx RENAME TO new_idx",
    );
}

#[test]
fn typed_table_alias_columns() {
    let sql = "SELECT * FROM jsonb_to_record('{}'::JSONB) AS x (a TEXT, b INT)";
    accepts(&[Postgres], sql);
    rejects(&[Core, Oracle, MySql, Snowflake], sql);
}

#[test]
fn order_by_using() {
    accepts(&[Postgres], "SELECT a FROM t ORDER BY a USING <");
    accepts(
        &[Postgres],
        "SELECT a FROM t ORDER BY a USING OPERATOR(pg_catalog.<) NULLS LAST",
    );
    rejects(
        &[Core, MySql, Snowflake],
        "SELECT a FROM t ORDER BY a USING <",
    );
}

#[test]
fn reserved_word_alias_after_as() {
    accepts(&[Postgres, Redshift], "SELECT 1 AS RETURNING");
    accepts(&[Postgres, Redshift], "SELECT NULL AS NOTNULL");
    rejects(&[Core, MySql], "SELECT 1 AS select");
    accepts(&[Postgres], "SELECT 1 AS \"select\"");
}

#[test]
fn create_role() {
    accepts(
        &[
            Postgres, Snowflake, Redshift, MySql, Oracle, ClickHouse, Hive, Teradata,
        ],
        "CREATE ROLE consultant",
    );
    accepts(
        &[MySql, ClickHouse],
        "CREATE ROLE IF NOT EXISTS mysql_a, mysql_b",
    );
    rejects(
        &[Postgres, Snowflake, Oracle],
        "CREATE ROLE IF NOT EXISTS mysql_a, mysql_b",
    );
    rejects(
        &[Core, BigQuery, Spark, DuckDb, Sqlite],
        "CREATE ROLE consultant",
    );
    accepts(&[Snowflake], "CREATE OR REPLACE ROLE consultant");
    let StatementKind::CreateRole(create) =
        statement::<sqlparser::dialect::MySql>("CREATE ROLE a, b").kind
    else {
        panic!("not CREATE ROLE");
    };
    assert_eq!(create.names.len(), 2);
}

#[test]
fn analyze() {
    accepts(&[DuckDb, Sqlite, Redshift, Postgres], "ANALYZE");
    accepts(&[DuckDb, Sqlite, Redshift, Postgres], "ANALYZE test_table");
    accepts(&[MySql], "ANALYZE TABLE test_table");
    rejects(&[Core, Oracle, Snowflake, BigQuery], "ANALYZE test_table");
}

#[test]
fn rename_table_is_mysql() {
    accepts(&[MySql], "RENAME TABLE a TO b, c.d TO e.f");
    rejects(&[Core, Postgres, Oracle, Snowflake], "RENAME TABLE a TO b");
}

#[test]
fn truncate_on_cluster_is_clickhouse() {
    accepts(&[ClickHouse], "TRUNCATE TABLE t ON CLUSTER cluster_name");
    rejects(
        &[Postgres, Core, MySql],
        "TRUNCATE TABLE t ON CLUSTER cluster_name",
    );
}

#[test]
fn hive_connectors() {
    accepts(&[Hive], "CREATE CONNECTOR my_connector");
    accepts(&[Hive], "DROP CONNECTOR IF EXISTS my_connector");
    accepts(
        &[Hive],
        "ALTER CONNECTOR my_connector SET URL 'jdbc:mysql://localhost:3306/mydb'",
    );
    accepts(&[Hive], "ALTER CONNECTOR c SET OWNER USER 'root'");
    accepts(
        &[Hive],
        "ALTER CONNECTOR c SET DCPROPERTIES('user' = 'root', 'password' = 'password')",
    );
    rejects(&[Spark, Core, Postgres], "CREATE CONNECTOR my_connector");
}

#[test]
fn spark_family_set_tblproperties() {
    accepts(
        &[Spark, Databricks, Hive],
        "ALTER TABLE tab SET TBLPROPERTIES('classification' = 'parquet')",
    );
    rejects(
        &[Core, Postgres, MySql],
        "ALTER TABLE tab SET TBLPROPERTIES ('a' = 'b')",
    );
}

#[test]
fn redshift_copy_iam_role_default() {
    accepts(
        &[Redshift],
        "COPY dst (c1) FROM 's3://b/x' IAM_ROLE DEFAULT CSV IGNOREHEADER 1",
    );
    rejects(
        &[Postgres],
        "COPY dst (c1) FROM 's3://b/x' IAM_ROLE DEFAULT CSV IGNOREHEADER 1",
    );
}

#[test]
fn snowflake_users() {
    for sql in [
        "CREATE USER u1",
        "CREATE OR REPLACE USER IF NOT EXISTS u1 PASSWORD='secret' MUST_CHANGE_PASSWORD=true",
        "CREATE USER u1 option1='value1' option2='value2'",
        "CREATE OR REPLACE USER IF NOT EXISTS u1 PASSWORD='secret' MUST_CHANGE_PASSWORD=false TYPE=SERVICE WITH TAG (t1='v1', t2='v2')",
        "ALTER USER IF EXISTS u1 RENAME TO u2",
        "ALTER USER u1 RESET PASSWORD",
        "ALTER USER u1 ABORT ALL QUERIES",
        "ALTER USER IF EXISTS u1 ADD DELEGATED AUTHORIZATION OF ROLE r1 TO SECURITY INTEGRATION i1",
        "ALTER USER u1 REMOVE DELEGATED AUTHORIZATION OF ROLE r1 FROM SECURITY INTEGRATION i1",
        "ALTER USER u1 REMOVE DELEGATED AUTHORIZATIONS FROM SECURITY INTEGRATION i1",
        "ALTER USER u1 ENROLL MFA",
        "ALTER USER u1 ADD MFA METHOD OTP COUNT = 8",
        "ALTER USER u1 MODIFY MFA METHOD PASSKEY SET COMMENT 'abc'",
        "ALTER USER u1 REMOVE MFA METHOD TOTP",
        "ALTER USER u1 SET AUTHENTICATION POLICY p1",
        "ALTER USER u1 SET DEFAULT_MFA_METHOD DUO",
        "ALTER USER u1 SET DEFAULT_SECONDARY_ROLES=('ALL')",
        "ALTER USER u1 SET PASSWORD='secret', WORKLOAD_IDENTITY=(TYPE=AWS, ARN='x')",
        "ALTER USER u1 SET TAG k1='v1', k2='v2'",
        "ALTER USER u1 UNSET TAG k1, k2",
        "ALTER USER u1 UNSET PASSWORD POLICY",
        "ALTER USER u1 UNSET PASSWORD, MUST_CHANGE_PASSWORD",
    ] {
        accepts(&[Snowflake], sql);
    }
    rejects(&[MySql, Core, Oracle], "ALTER USER u1 RESET PASSWORD");
    rejects(&[MySql, Core, BigQuery], "CREATE OR REPLACE USER u1");
    rejects(&[Snowflake], "ALTER USER u1");
}
