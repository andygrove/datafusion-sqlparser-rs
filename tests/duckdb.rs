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

//! DuckDB: what it adds to the core grammar, and what stays out of it.

use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::dialect::{Core, DuckDb, Postgres};
use sqlparser::parse_sql;

fn round_trips(sql: &str) {
    let statements = parse_sql::<DuckDb>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<DuckDb>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements[0].to_string(), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<DuckDb>(sql).is_err(), "accepted: {sql}");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("DuckDb").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
}

#[test]
fn struct_types_use_parentheses() {
    round_trips("CREATE TABLE t1 (s STRUCT(v VARCHAR, i INTEGER))");
    round_trips("CREATE TABLE t1 (s STRUCT(v VARCHAR, i INTEGER)[])");
    round_trips("CREATE TABLE t1 (s STRUCT(v VARCHAR, s STRUCT(a1 INTEGER, a2 VARCHAR))[])");
    rejected("CREATE TABLE t1 (s STRUCT(v VARCHAR, i INTEGER)))");
    rejected("CREATE TABLE t1 (s STRUCT(v VARCHAR, i INTEGER>)");
    rejected("CREATE TABLE t1 (s STRUCT<v VARCHAR, i INTEGER>)");
    rejected("CREATE TABLE t1 (s STRUCT v VARCHAR, i INTEGER )");
    rejected("CREATE TABLE t1 (s STRUCT VARCHAR, i INTEGER )");
    rejected("CREATE TABLE t1 (s STRUCT (VARCHAR, INTEGER))");
    rejected("CREATE TABLE t1 (s STRUCT)");
}

#[test]
fn union_types() {
    round_trips(
        "CREATE TABLE tbl1 (one UNION(a INT), two UNION(a INT, b INT), nested UNION(a UNION(b INT)))",
    );
    round_trips("SELECT x::UNION(a INT)");
    rejected("CREATE TABLE t (a UNION)");
}

#[test]
fn wildcard_exclude_and_replace() {
    round_trips("SELECT * EXCLUDE (col_a) FROM data");
    round_trips("SELECT name.* EXCLUDE department_id FROM employee_table");
    round_trips("SELECT * EXCLUDE (department_id, employee_id) FROM employee_table");
    round_trips("SELECT * REPLACE (a + 1 AS a) FROM t");
    round_trips("SELECT * EXCLUDE (b) REPLACE (a + 1 AS a) FROM t");
    rejected("SELECT * ILIKE 'a%' FROM t");
    rejected("SELECT * RENAME (a AS b) FROM t");
}

#[test]
fn integer_division() {
    round_trips("SELECT 5 // 2");
    assert!(parse_sql::<Postgres>("SELECT 5 // 2").is_err());
}

#[test]
fn macros() {
    round_trips("CREATE MACRO schema.add(a, b) AS a + b");
    round_trips("CREATE MACRO add_default(a, b := 5) AS a + b");
    round_trips(
        "CREATE OR REPLACE TEMPORARY MACRO dynamic_table(col1_value, col2_value) AS TABLE SELECT col1_value AS column1, col2_value AS column2 UNION ALL SELECT 'Hello' AS col1_value, 456 AS col2_value",
    );
    round_trips("CREATE MACRO four() AS 4");
    round_trips("CREATE OR REPLACE TEMPORARY MACRO nothing() AS TABLE SELECT 1");
    parses_to("CREATE FUNCTION add() AS 1", "CREATE MACRO add() AS 1");
    parses_to(
        "CREATE MACRO add_default(a, b => 5) AS a + b",
        "CREATE MACRO add_default(a, b := 5) AS a + b",
    );
    parses_to(
        "CREATE TEMP MACRO f(a) AS a",
        "CREATE TEMPORARY MACRO f(a) AS a",
    );
    rejected("CREATE MACRO f AS 1");
    rejected("CREATE MACRO f(a)");
    rejected("CREATE PERSISTENT MACRO f(a) AS a");
}

#[test]
fn set_operations_by_name() {
    round_trips("SELECT * FROM capitals UNION BY NAME SELECT * FROM weather");
    round_trips("SELECT * FROM capitals UNION ALL BY NAME SELECT * FROM weather");
    round_trips("SELECT * FROM capitals UNION DISTINCT BY NAME SELECT * FROM weather");
    round_trips("SELECT 1 EXCEPT BY NAME SELECT 2");
    round_trips("SELECT 1 INTERSECT ALL BY NAME SELECT 2");
    rejected("SELECT 1 UNION BY SELECT 2");
    assert!(parse_sql::<Postgres>("SELECT 1 UNION BY NAME SELECT 2").is_err());
}

#[test]
fn extensions() {
    round_trips("INSTALL tpch");
    round_trips("LOAD my_extension");
    round_trips("LOAD 'my_extension'");
    rejected("INSTALL");
}

#[test]
fn integer_types() {
    for name in ["UTINYINT", "USMALLINT", "UBIGINT", "UHUGEINT", "HUGEINT"] {
        round_trips(&format!("SELECT 123::{name}"));
    }
}

#[test]
fn struct_and_list_literals() {
    round_trips(
        "SELECT {'a': 1, 'b': 2, 'c': 3}, [{'a': 'abc'}], {'a': 1, 'b': [t.str_col]}, {'a': 1, 'b': 'abc'}, {'abc': str_col}, {'a': {'aa': 1}}",
    );
    round_trips("SELECT {a: 1, \"b\": 2}");
    round_trips("SELECT []");
    round_trips("SELECT ['a', 'b', 'c'][3] AS three");
    parses_to("SELECT {'a':b}", "SELECT {'a': b}");
    parses_to("SELECT {'a':1}", "SELECT {'a': 1}");
    rejected("SELECT {'a' 1}");
    rejected("SELECT {1: 2}");
    rejected("SELECT {'a': 1");
    rejected("SELECT ['a'");
}

#[test]
fn map_literals() {
    round_trips("SELECT MAP {'a': 1, 'b': 2}");
    round_trips("SELECT MAP {1 + 1: [2], 3: []}");
    round_trips("SELECT MAP {}");
    parses_to("SELECT map {k:v}", "SELECT MAP {k: v}");
    round_trips("SELECT map(['a'], [1])");
    rejected("SELECT MAP {'a' 1}");
}

#[test]
fn secrets() {
    round_trips(
        "CREATE OR REPLACE PERSISTENT SECRET IF NOT EXISTS name IN storage ( TYPE type, key1 value1, key2 value2 )",
    );
    round_trips("CREATE SECRET ( TYPE type )");
    round_trips("CREATE TEMPORARY SECRET s ( TYPE s3, KEY_ID 'abc' )");
    round_trips("DROP PERSISTENT SECRET IF EXISTS secret FROM storage");
    round_trips("DROP SECRET secret");
    round_trips("DROP TEMPORARY SECRET secret");
    rejected("CREATE SECRET ( key1 value1 )");
    rejected("CREATE SECRET s");
    rejected("CREATE TEMPORARY PERSISTENT SECRET ( TYPE t )");
    rejected("DROP TEMPORARY PERSISTENT SECRET s");
    rejected("DROP SECRET");
}

#[test]
fn attach_and_detach() {
    round_trips(
        "ATTACH DATABASE IF NOT EXISTS 'sqlite_file.db' AS sqlite_db (READ_ONLY false, TYPE SQLITE)",
    );
    round_trips("ATTACH 'postgres://user.name:pass-word@some.url.com:5432/postgres'");
    round_trips("ATTACH 'a.db' (READ_ONLY)");
    round_trips("ATTACH 'a.db' (READ_ONLY true)");
    round_trips("DETACH DATABASE IF EXISTS db_name");
    round_trips("DETACH db_name");
    rejected("ATTACH 'a.db' (ENCRYPTED)");
    rejected("ATTACH");
    rejected("DETACH");
}

#[test]
fn named_arguments() {
    round_trips("SELECT FUN(a := '1', b := '2') FROM foo");
    round_trips("SELECT FUN(a => '1')");
    round_trips("SELECT FUN(a = '1')");
    assert!(parse_sql::<Core>("SELECT FUN(a := '1')").is_err());
}

#[test]
fn use_names() {
    for name in [
        "mydb",
        "SCHEMA",
        "DATABASE",
        "CATALOG",
        "WAREHOUSE",
        "DEFAULT",
    ] {
        round_trips(&format!("USE {name}"));
        round_trips(&format!("USE \"{name}\""));
        round_trips(&format!("USE '{name}'"));
    }
    round_trips("USE \"CATALOG\".\"my_schema\"");
    round_trips("USE 'CATALOG'.'my_schema'");
    round_trips("USE mydb.my_schema");
}

#[test]
fn trim_with_characters() {
    round_trips(
        r#"SELECT customer_id, TRIM(item_price_id, '"', "a") AS item_price_id FROM models_staging.subscriptions"#,
    );
    round_trips("SELECT TRIM('xyz', 'a')");
    rejected("SELECT TRIM('xyz' 'a')");
}

#[test]
fn extract_takes_a_quoted_part() {
    round_trips("SELECT EXTRACT('month' FROM my_timestamp) FROM my_table");
    round_trips("SELECT EXTRACT(MONTH FROM my_timestamp) FROM my_table");
}

#[test]
fn lambdas() {
    round_trips("SELECT [3, 4, 5, 6].list_filter(lambda x : x > 4)");
    round_trips("SELECT list_filter([1, 2, 3], x -> x > 1)");
    round_trips("SELECT list_filter([1, 3, 1, 5], lambda x, i : x > i)");
    round_trips("SELECT list_transform([1, 2, 3], lambda x : x * 2)");
    round_trips("SELECT list_transform([1, 2, 3], (x, i) -> x + i)");
    parses_to(
        "SELECT list_filter(a, lambda x:x > 1)",
        "SELECT list_filter(a, lambda x : x > 1)",
    );
    parses_to("SELECT lambda x", "SELECT lambda AS x");
    round_trips("SELECT lambda FROM t");
}

#[test]
fn method_calls_chain() {
    round_trips("SELECT 'a'.upper().lower()");
    round_trips("SELECT [1].list_append(2).list_append(3)");
    round_trips("SELECT f(a).g(b)");
    round_trips("SELECT a.b(c)");
    round_trips("SELECT [1].x");
    round_trips("SELECT {'a': 1}.a");
    round_trips("SELECT (a).g(b)");
}

#[test]
fn slices() {
    round_trips("SELECT a[1:2], a[:2], a[1:], a[:], a[1:2:3]");
    rejected("SELECT a[::2]");
    round_trips("SELECT a[1]");
    rejected("SELECT a[]");
}

#[test]
fn map_types() {
    round_trips("CREATE TABLE t (a MAP(VARCHAR, INT))");
    round_trips("SELECT x::MAP(VARCHAR, STRUCT(a INT))");
}

#[test]
fn computed_columns_without_generated() {
    round_trips("CREATE TABLE t (a INT AS (1), b INT AS (2) STORED, c INT AS (3) VIRTUAL)");
    round_trips("CREATE TABLE t (b INT GENERATED ALWAYS AS (2))");
}

#[test]
fn drop_temporary() {
    round_trips("DROP TEMPORARY TABLE t");
    rejected("DROP PERSISTENT TABLE t");
    assert!(parse_sql::<Postgres>("DROP TEMPORARY TABLE t").is_err());
}

#[test]
fn nested_block_comments() {
    parses_to(
        "SELECT a /* outer /* inner */ still comment */ FROM t",
        "SELECT a FROM t",
    );
    rejected("SELECT a /* outer /* inner */ FROM t");
}

#[test]
fn from_first() {
    round_trips("FROM t");
    round_trips("FROM t SELECT a");
    round_trips("SELECT * FROM (FROM t)");
}

#[test]
fn order_and_group_by_all() {
    round_trips("SELECT a FROM t ORDER BY ALL");
    round_trips("SELECT a FROM t ORDER BY ALL DESC NULLS LAST");
    round_trips("SELECT a, SUM(b) FROM t GROUP BY ALL");
    rejected("SELECT a FROM t ORDER BY ALL, a");
    rejected("SELECT a FROM t ORDER BY a, ALL");
}

#[test]
fn trailing_commas() {
    parses_to("SELECT a, b, FROM t", "SELECT a, b FROM t");
    rejected("SELECT a FROM t, u,");
    parses_to("SELECT f(1, 2,)", "SELECT f(1, 2)");
    parses_to("SELECT [1, 2,]", "SELECT [1, 2]");
    parses_to("SELECT {'a': 1,}", "SELECT {'a': 1}");
}

#[test]
fn string_aliases_need_as() {
    round_trips("SELECT 1 AS 'a'");
    round_trips("SELECT * FROM t AS 'a'");
    rejected("SELECT 1 'a'");
    rejected("SELECT * FROM t 'a'");
}

#[test]
fn numbers_with_underscores() {
    round_trips("SELECT 1_000");
    round_trips("SELECT 1_000.5_5");
    round_trips("SELECT 1_0e1_0");
    round_trips("SELECT _1");
    rejected("SELECT 1__0");
    rejected("SELECT 1_");
}

#[test]
fn dollar_in_names() {
    round_trips("SELECT a$b FROM c$d");
    round_trips("SELECT $1");
    round_trips("SELECT $$text$$");
}

#[test]
fn operators() {
    round_trips("SELECT a << 1, a >> 1");
    round_trips("SELECT a ~~ b, a !~~ b, a ~~* b, a !~~* b");
    round_trips("SELECT a ILIKE b");
    parses_to("SELECT a NOTNULL", "SELECT a IS NOT NULL");
    parses_to("SELECT a NOT NULL", "SELECT a IS NOT NULL");
    round_trips("CREATE TABLE t (a INT DEFAULT 1 NOT NULL)");
    parses_to(
        "CREATE TABLE t (a INT DEFAULT (1 NOT NULL) NOT NULL)",
        "CREATE TABLE t (a INT DEFAULT (1 IS NOT NULL) NOT NULL)",
    );
    round_trips("SELECT * FROM t WHERE a BETWEEN b = c AND d");
}

#[test]
fn cte_materialization() {
    round_trips("WITH a AS MATERIALIZED (SELECT 1) SELECT * FROM a");
    round_trips("WITH a AS NOT MATERIALIZED (SELECT 1) SELECT * FROM a");
    assert!(
        parse_sql::<sqlparser::dialect::Core>("WITH a AS MATERIALIZED (SELECT 1) SELECT 1")
            .is_err()
    );
}

#[test]
fn aggregate_order_by_without_arguments() {
    round_trips("SELECT ARRAY_AGG(ORDER BY a) FROM t");
    rejected("SELECT ARRAY_AGG(DISTINCT ORDER BY a) FROM t");
}

#[test]
fn explain_takes_options() {
    round_trips("EXPLAIN (ANALYZE, VERBOSE) SELECT 1");
}

#[test]
fn group_by_forms() {
    round_trips("SELECT a FROM t GROUP BY ROLLUP (a)");
    round_trips("SELECT a FROM t QUALIFY a > 1");
}

#[test]
fn add_column_if_not_exists() {
    round_trips("ALTER TABLE t ADD COLUMN IF NOT EXISTS a INT");
}

mod shape {
    use sqlparser::ast::*;
    use sqlparser::dialect::DuckDb;
    use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement};
    use sqlparser::dialect::duckdb::{AttachOption, DuckDbExpr, DuckDbStatement, MacroDefinition};
    use sqlparser::parse_sql;

    fn statement(sql: &str) -> DuckDbStatement<'_> {
        match parse_sql::<DuckDb>(sql).unwrap().remove(0).kind {
            StatementKind::Ext(BuiltinStatement::DuckDb(statement)) => statement,
            other => panic!("not a DuckDB statement: {other:?}"),
        }
    }

    fn query(sql: &str) -> Query<'_, Builtin> {
        match parse_sql::<DuckDb>(sql).unwrap().remove(0).kind {
            StatementKind::Query(query) => *query,
            other => panic!("not a query: {other:?}"),
        }
    }

    fn first_expr(sql: &str) -> Expr<'_, Builtin> {
        let SetExpr::Select(select) = query(sql).body else {
            panic!("not a SELECT");
        };
        let mut select = *select;
        match select.projection.remove(0).kind {
            SelectItemKind::Expr { expr, .. } => expr,
            other => panic!("not an expression: {other:?}"),
        }
    }

    fn duckdb_expr(sql: &str) -> DuckDbExpr<'_> {
        match first_expr(sql).kind {
            ExprKind::Ext(BuiltinExpr::DuckDb(expr)) => expr,
            other => panic!("not a DuckDB expression: {other:?}"),
        }
    }

    #[test]
    fn struct_type_keeps_its_fields() {
        let parsed = parse_sql::<DuckDb>("CREATE TABLE t (s STRUCT(v VARCHAR, i INTEGER)[])")
            .unwrap()
            .remove(0);
        let StatementKind::CreateTable(create) = parsed.kind else {
            panic!("not CREATE TABLE");
        };
        let ty = &create.columns[0].data_type;
        assert_eq!(ty.name.text, "STRUCT");
        assert_eq!(ty.array, ArrayType::Brackets(vec![None]));
        let Some(AngleArguments::ParenFields(fields)) = ty.angle.as_deref() else {
            panic!("no fields: {ty:?}");
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name.unwrap().text, "v");
        assert_eq!(fields[1].data_type.name.text, "INTEGER");
    }

    #[test]
    fn macro_keeps_defaults_and_kind() {
        let DuckDbStatement::CreateMacro(create) =
            statement("CREATE MACRO add_default(a, b := 5) AS a + b")
        else {
            panic!("not a macro");
        };
        let args = create.args.as_ref().unwrap();
        assert!(args[0].default.is_none());
        assert_eq!(args[1].name.text, "b");
        assert!(matches!(
            args[1].default,
            Some(Expr {
                kind: ExprKind::Literal(Literal::Number("5")),
                ..
            })
        ));
        assert!(matches!(create.definition, MacroDefinition::Expr(_)));

        let DuckDbStatement::CreateMacro(create) =
            statement("CREATE OR REPLACE TEMPORARY MACRO f() AS TABLE SELECT 1")
        else {
            panic!("not a macro");
        };
        assert!(create.or_replace && create.temporary);
        assert!(create.args.is_none());
        assert!(matches!(create.definition, MacroDefinition::Table(_)));
    }

    #[test]
    fn by_name_is_recorded() {
        for (sql, expected) in [
            ("SELECT 1 UNION BY NAME SELECT 2", SetQuantifier::None),
            ("SELECT 1 UNION ALL BY NAME SELECT 2", SetQuantifier::All),
            (
                "SELECT 1 UNION DISTINCT BY NAME SELECT 2",
                SetQuantifier::Distinct,
            ),
        ] {
            let SetExpr::SetOperation {
                quantifier,
                by_name,
                ..
            } = query(sql).body
            else {
                panic!("not a set operation");
            };
            assert_eq!(quantifier, expected);
            assert!(by_name);
        }
    }

    #[test]
    fn secret_records_persistence() {
        let DuckDbStatement::CreateSecret(create) = statement(
            "CREATE OR REPLACE PERSISTENT SECRET IF NOT EXISTS name IN storage ( TYPE type, key1 value1 )",
        ) else {
            panic!("not a secret");
        };
        assert_eq!(create.temporary, Some(false));
        assert_eq!(create.name.unwrap().text, "name");
        assert_eq!(create.storage.unwrap().text, "storage");
        assert_eq!(create.secret_type.text, "type");
        assert_eq!(create.options[0].0.text, "key1");
        assert_eq!(create.options[0].1.text, "value1");

        let DuckDbStatement::CreateSecret(create) = statement("CREATE SECRET ( TYPE type )") else {
            panic!("not a secret");
        };
        assert_eq!(create.temporary, None);
        assert!(create.name.is_none());
    }

    #[test]
    fn attach_keeps_its_options() {
        let DuckDbStatement::Attach(attach) =
            statement("ATTACH DATABASE IF NOT EXISTS 'f.db' AS d (READ_ONLY false, TYPE SQLITE)")
        else {
            panic!("not ATTACH");
        };
        assert!(attach.database && attach.if_not_exists);
        assert_eq!(attach.path.text, "f.db");
        assert_eq!(attach.path.quote, Some('\''));
        assert_eq!(attach.alias.unwrap().text, "d");
        assert_eq!(attach.options[0], AttachOption::ReadOnly(Some(false)));
        let AttachOption::Type(kind) = &attach.options[1] else {
            panic!("not TYPE");
        };
        assert_eq!(kind.text, "SQLITE");
    }

    #[test]
    fn struct_literal_keeps_quoted_keys() {
        let DuckDbExpr::Struct(fields) = duckdb_expr("SELECT {'a': 1, b: 2}") else {
            panic!("not a struct");
        };
        assert_eq!(fields[0].0.text, "a");
        assert_eq!(fields[0].0.quote, Some('\''));
        assert_eq!(fields[1].0.quote, None);
    }

    #[test]
    fn list_literal_is_not_an_array_constructor() {
        let DuckDbExpr::List(items) = duckdb_expr("SELECT [1, 2]") else {
            panic!("not a list");
        };
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn subscript_on_a_list_is_the_core_node() {
        let ExprKind::Special(special) = first_expr("SELECT ['a'][3]").kind else {
            panic!("not a subscript");
        };
        assert!(matches!(*special, SpecialExpr::Subscript { .. }));
    }

    #[test]
    fn arrow_after_a_name_is_a_lambda() {
        let ExprKind::Function { args, .. } = first_expr("SELECT list_filter(a, x -> x > 1)").kind
        else {
            panic!("not a function");
        };
        let FunctionArg::Expr(Expr {
            kind: ExprKind::Special(special),
            ..
        }) = &args[1]
        else {
            panic!("not a special expression: {:?}", args[1]);
        };
        assert!(matches!(**special, SpecialExpr::Lambda { .. }));
    }

    #[test]
    fn keyword_lambda_keeps_its_parameters() {
        let ExprKind::Function { args, .. } =
            first_expr("SELECT list_filter(a, lambda x, i : x > i)").kind
        else {
            panic!("not a function");
        };
        let FunctionArg::Expr(Expr {
            kind: ExprKind::Ext(BuiltinExpr::DuckDb(DuckDbExpr::Lambda(lambda))),
            ..
        }) = &args[1]
        else {
            panic!("not a lambda: {:?}", args[1]);
        };
        assert_eq!(lambda.params.len(), 2);
    }

    #[test]
    fn assignment_arguments_are_named() {
        let ExprKind::Function { args, .. } = first_expr("SELECT f(a := 1)").kind else {
            panic!("not a function");
        };
        assert!(matches!(
            args[0],
            FunctionArg::NamedWith { operator: ":=", .. }
        ));
    }

    #[test]
    fn map_keeps_pairs() {
        let DuckDbExpr::Map(entries) = duckdb_expr("SELECT MAP {1 + 1: 2}") else {
            panic!("not a map");
        };
        assert!(matches!(entries[0].0.kind, ExprKind::Binary { .. }));
    }

    #[test]
    fn cte_records_materialization() {
        let parsed = query("WITH a AS NOT MATERIALIZED (SELECT 1) SELECT 1");
        assert_eq!(parsed.with[0].materialized, Some(false));
        let parsed = query("WITH a AS (SELECT 1) SELECT 1");
        assert_eq!(parsed.with[0].materialized, None);
    }
}
