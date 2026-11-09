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

//! Teradata: what it adds to the core grammar, and what stays out of it.

use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::dialect::{Core, Hive, MySql, Postgres, Snowflake, Teradata};
use sqlparser::parse_sql;

fn round_trips(sql: &str) {
    let statements = parse_sql::<Teradata>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<Teradata>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements[0].to_string(), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Teradata>(sql).is_err(), "accepted: {sql}");
}

/// Other dialects must not pick up Teradata's syntax.
fn only_teradata(sql: &str) {
    round_trips(sql);
    assert!(parse_sql::<Core>(sql).is_err(), "core accepted: {sql}");
    assert!(parse_sql::<MySql>(sql).is_err(), "mysql accepted: {sql}");
    assert!(
        parse_sql::<Postgres>(sql).is_err(),
        "postgres accepted: {sql}"
    );
    assert!(
        parse_sql::<Snowflake>(sql).is_err(),
        "snowflake accepted: {sql}"
    );
    assert!(parse_sql::<Hive>(sql).is_err(), "hive accepted: {sql}");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("Teradata").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
}

#[test]
fn identifiers_may_hold_hash_and_dollar() {
    round_trips(concat!(
        "SELECT ",
        "NULL AS foo, ",
        "NULL AS _bar, ",
        "NULL AS #baz, ",
        "NULL AS $qux, ",
        "NULL AS a$1, ",
        "NULL AS a#1, ",
        "NULL AS a_1, ",
        r#"NULL AS "quoted id""#
    ));
    round_trips("SELECT a#b FROM t$1");
    round_trips("SELECT #_of_values");
    round_trips("SELECT $1, $hello$$ FROM t");
    assert!(parse_sql::<Core>("SELECT NULL AS #baz").is_err());
}

#[test]
fn identifiers_follow_unicode_letters() {
    round_trips("SELECT àà FROM مصطفى");
    rejected("SELECT * FROM t 💝");
    rejected("SELECT a💝b");
}

#[test]
fn multiset_and_set_tables() {
    only_teradata("CREATE MULTISET TABLE foo (id INT)");
    only_teradata("CREATE SET TABLE foo (id INT)");
}

#[test]
fn volatile_tables() {
    // Snowflake has `VOLATILE` too.
    round_trips("CREATE VOLATILE TABLE foo (id INT)");
    assert!(parse_sql::<Core>("CREATE VOLATILE TABLE foo (id INT)").is_err());
    only_teradata("CREATE MULTISET VOLATILE TABLE foo (id INT)");
    only_teradata("CREATE SET VOLATILE TABLE IF NOT EXISTS foo (id INT)");
    round_trips("CREATE MULTISET GLOBAL TEMPORARY TABLE foo (id INT)");
    round_trips("CREATE OR REPLACE TABLE foo (id INT)");
    parses_to(
        "CREATE TEMP TABLE foo (id INT)",
        "CREATE TEMPORARY TABLE foo (id INT)",
    );
}

#[test]
fn fallback_follows_a_leading_comma() {
    only_teradata("CREATE TABLE foo, FALLBACK (id INT)");
    only_teradata("CREATE TABLE foo, NO FALLBACK (id INT)");
    only_teradata("CREATE MULTISET TABLE foo, NO FALLBACK (id INT)");
    round_trips("CREATE TABLE foo, FALLBACK AS (SELECT 1 AS a) WITH DATA");
    rejected("CREATE TABLE foo, BAR (id INT)");
    rejected("CREATE TABLE foo, (id INT)");
    rejected("CREATE TABLE foo FALLBACK (id INT)");
    rejected("CREATE TABLE foo, NO (id INT)");
}

#[test]
fn create_table_as_with_data() {
    only_teradata("CREATE TABLE foo AS (SELECT 1 AS a) WITH DATA");
    only_teradata("CREATE TABLE foo AS (SELECT 1 AS a) WITH NO DATA");
    only_teradata("CREATE TABLE foo AS (SELECT 1 AS a) WITH DATA AND STATISTICS");
    only_teradata("CREATE TABLE foo AS (SELECT 1 AS a) WITH DATA AND NO STATISTICS");
    only_teradata("CREATE TABLE foo AS (SELECT 1 AS a) WITH NO DATA AND STATISTICS");
    only_teradata("CREATE TABLE foo AS (SELECT 1 AS a) WITH NO DATA AND NO STATISTICS");
    round_trips("CREATE TABLE foo AS SELECT a FROM t WITH DATA");
    round_trips("CREATE TABLE foo AS SELECT a FROM t ORDER BY a WITH NO DATA");
    rejected("CREATE TABLE foo AS (SELECT 1 AS a) WITH");
    rejected("CREATE TABLE foo AS (SELECT 1 AS a) WITH DATA AND");
    rejected("CREATE TABLE foo (a INT) WITH DATA");
}

#[test]
fn create_table_without_as_is_rendered_with_it() {
    parses_to("CREATE TABLE foo SELECT 1", "CREATE TABLE foo AS SELECT 1");
    parses_to(
        "CREATE TABLE foo (a INT) SELECT 1 WITH DATA",
        "CREATE TABLE foo (a INT) AS SELECT 1 WITH DATA",
    );
    assert!(parse_sql::<Core>("CREATE TABLE foo SELECT 1").is_err());
}

#[test]
fn on_commit() {
    only_teradata(concat!(
        "CREATE MULTISET VOLATILE TABLE foo, NO FALLBACK ",
        "(id INT, name VARCHAR(100)) ",
        "ON COMMIT PRESERVE ROWS"
    ));
    round_trips("CREATE VOLATILE TABLE foo (id INT) ON COMMIT DELETE ROWS");
    round_trips("CREATE VOLATILE TABLE foo (id INT) ON COMMIT PRESERVE ROWS AS SELECT 1");
    round_trips("CREATE VOLATILE TABLE foo AS (SELECT 1 AS a) WITH DATA ON COMMIT PRESERVE ROWS");
    rejected("CREATE TEMPORARY TABLE dst AS (SELECT * FROM src) ON COMMIT PRESERVE ROWS");
    rejected("CREATE TABLE foo (id INT) ON COMMIT");
    rejected("CREATE TABLE foo (id INT) ON COMMIT DROP");
    rejected("CREATE TABLE foo (id INT) ON COMMIT PRESERVE ROWS ON COMMIT PRESERVE ROWS");
}

#[test]
fn create_needs_a_table_body() {
    rejected("CREATE TABLE foo");
    rejected("CREATE MULTISET foo (id INT)");
    rejected("CREATE VOLATILE VIEW v AS SELECT 1");
}

#[test]
fn other_create_statements_still_work() {
    round_trips("CREATE VIEW v AS SELECT 1");
    round_trips("CREATE OR REPLACE VIEW v AS SELECT 1");
    round_trips("CREATE INDEX i ON t (a)");
    round_trips("CREATE TABLE foo (a INT, b VARCHAR(10) NOT NULL)");
}

#[test]
fn true_and_false_are_names() {
    round_trips("SELECT TRUE, FALSE FROM t");
    round_trips("SELECT a FROM t WHERE TRUE = b");
    round_trips("SELECT t.TRUE FROM t");
    round_trips("SELECT 1 AS TRUE");
    round_trips("SELECT a IS TRUE, b IS NOT FALSE FROM t");
}

#[test]
fn intervals_need_a_unit_and_take_expressions() {
    round_trips("SELECT INTERVAL 1 + 1 DAY");
    round_trips("SELECT INTERVAL '1' DAY");
    rejected("SELECT INTERVAL '1'");
}

#[test]
fn comment_on() {
    // PostgreSQL has `COMMENT ON` too.
    round_trips("COMMENT ON TABLE t IS 'comment'");
    assert!(parse_sql::<Core>("COMMENT ON TABLE t IS 'comment'").is_err());
    round_trips("COMMENT ON COLUMN tab.name IS 'comment'");
    round_trips("COMMENT ON TABLE t IS NULL");
    round_trips("COMMENT IF EXISTS ON TABLE t IS 'comment'");
    rejected("COMMENT ON t IS 'comment'");
}

#[test]
fn execute() {
    round_trips("EXECUTE a");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1'");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' INTO a");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' INTO a, b");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' USING 1 AS x, y");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' INTO a, b USING 1 AS x, y");
    round_trips("EXECUTE a(1, 't')");
    round_trips("EXECUTE a USING CAST(1337 AS SMALLINT), CAST(7331 AS SMALLINT)");
    round_trips("EXECUTE my_schema.my_macro 1, 2");
    parses_to("EXEC a(1)", "EXECUTE a(1)");
    rejected("EXECUTE a INTO b");
    rejected("EXECUTE a USING");
    rejected("EXECUTE ('SELECT 1')");
}

#[test]
fn top_comes_before_distinct() {
    only_teradata("SELECT TOP 3 DISTINCT a, b, c FROM tbl");
    round_trips("SELECT TOP 3 ALL * FROM tbl");
    round_trips("SELECT TOP 3 * FROM tbl");
    round_trips("SELECT TOP (5) * FROM foo");
    round_trips("SELECT TOP (a + 1) * FROM foo");
    round_trips("SELECT TOP (5) PERCENT * FROM foo");
    round_trips("SELECT TOP 5 PERCENT * FROM foo");
    round_trips("SELECT TOP (5) WITH TIES * FROM foo");
    round_trips("SELECT TOP (10) PERCENT WITH TIES * FROM foo");
    rejected("SELECT DISTINCT TOP 3 a FROM tbl");
    rejected("SELECT TOP a FROM tbl");
    rejected("SELECT TOP 1.5 a FROM tbl");
}

#[test]
fn null_treatment_inside_the_call() {
    round_trips("SELECT FIRST_VALUE(a IGNORE NULLS) OVER () FROM t");
    round_trips("SELECT FIRST_VALUE(a RESPECT NULLS) OVER () FROM t");
    round_trips("SELECT LAST_VALUE(a) IGNORE NULLS OVER () FROM t");
    round_trips("SELECT ARRAY_AGG(a IGNORE NULLS ORDER BY b) FROM t");
    assert!(parse_sql::<Core>("SELECT FIRST_VALUE(a IGNORE NULLS) OVER () FROM t").is_err());
    assert!(parse_sql::<MySql>("SELECT FIRST_VALUE(a IGNORE NULLS) OVER () FROM t").is_err());
    rejected("SELECT FIRST_VALUE(a IGNORE) FROM t");
    rejected("SELECT LAG(1 IGNORE NULLS) IGNORE NULLS OVER () FROM t1");
}

#[test]
fn adjacent_strings_join() {
    parses_to("SELECT 'M' 'y' 'S' 'q' 'l'", "SELECT 'MySql'");
    parses_to(
        "SELECT * FROM t WHERE col = 'Hello' ' ' 'World!'",
        "SELECT * FROM t WHERE col = 'Hello World!'",
    );
    parses_to(
        "SELECT 'abc' in ('a' 'b' 'c', 'd' )",
        "SELECT 'abc' IN ('abc', 'd')",
    );
    parses_to("SELECT 'a''b' 'c'", "SELECT 'a''bc'");
    parses_to("SELECT 'a'\n  'b' AS c", "SELECT 'ab' AS c");
    parses_to(r#"SELECT 'a' "b""#, r#"SELECT 'a' AS "b""#);
}

#[test]
fn grouping_forms() {
    round_trips("SELECT a, b FROM t GROUP BY GROUPING SETS ((a), (b), (c))");
    round_trips("SELECT a, b FROM t GROUP BY ROLLUP (a, b)");
    round_trips("SELECT a, b FROM t GROUP BY CUBE (a, b)");
    round_trips("SELECT a, sum(b) FROM t GROUP BY a, ROLLUP (b, c)");
}

mod shape {
    use sqlparser::ast::{
        CreateTable, Expr, ExprKind, Select, SelectItemKind, SetExpr, Statement, StatementKind,
    };
    use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement};
    use sqlparser::dialect::teradata::{
        TeradataCreateTable, TeradataExecute, TeradataExpr, TeradataOnCommit, TeradataSetKind,
        TeradataStatement, TeradataTopQuantity, TeradataWithData,
    };
    use sqlparser::dialect::{Core, Teradata};
    use sqlparser::parse_sql;

    fn statement(sql: &str) -> Statement<'_, Builtin> {
        parse_sql::<Teradata>(sql)
            .unwrap_or_else(|e| panic!("{sql}: {e:?}"))
            .remove(0)
    }

    fn create_table(sql: &str) -> Box<TeradataCreateTable<'_>> {
        match statement(sql).kind {
            StatementKind::Ext(BuiltinStatement::Teradata(TeradataStatement::CreateTable(
                create,
            ))) => create,
            other => panic!("{other:?}"),
        }
    }

    fn select(sql: &str) -> Box<Select<'_, Builtin>> {
        let StatementKind::Query(query) = statement(sql).kind else {
            panic!("{sql}");
        };
        let SetExpr::Select(select) = query.body else {
            panic!("{sql}");
        };
        select
    }

    #[test]
    fn plain_tables_are_core_nodes() {
        let StatementKind::CreateTable(table) = statement("CREATE TABLE foo (id INT)").kind else {
            panic!("expected a core CREATE TABLE");
        };
        let CreateTable { name, columns, .. } = *table;
        assert_eq!(name.to_string(), "foo");
        assert_eq!(columns.len(), 1);
    }

    #[test]
    fn table_clauses() {
        let create = create_table(
            "CREATE MULTISET VOLATILE TABLE foo, NO FALLBACK (id INT) ON COMMIT PRESERVE ROWS",
        );
        assert_eq!(create.set_kind, Some(TeradataSetKind::Multiset));
        assert!(create.volatile);
        assert!(!create.global);
        assert_eq!(create.fallback, Some(false));
        assert_eq!(create.on_commit, Some(TeradataOnCommit::PreserveRows));
        assert!(!create.on_commit_last);
        assert_eq!(create.table.name.to_string(), "foo");

        let create = create_table("CREATE SET TABLE foo, FALLBACK (id INT)");
        assert_eq!(create.set_kind, Some(TeradataSetKind::Set));
        assert_eq!(create.fallback, Some(true));
    }

    #[test]
    fn with_data() {
        let create =
            create_table("CREATE TABLE foo AS (SELECT 1 AS a) WITH NO DATA AND STATISTICS");
        assert_eq!(
            create.with_data,
            Some(TeradataWithData {
                data: false,
                statistics: Some(true),
            })
        );
        assert!(create.table.query.is_some());

        let create =
            create_table("CREATE VOLATILE TABLE foo AS (SELECT 1) WITH DATA ON COMMIT DELETE ROWS");
        assert_eq!(create.on_commit, Some(TeradataOnCommit::DeleteRows));
        assert!(create.on_commit_last);
    }

    #[test]
    fn execute_parts() {
        let StatementKind::Ext(BuiltinStatement::Teradata(TeradataStatement::Execute(execute))) =
            statement("EXECUTE IMMEDIATE 'SELECT 1' INTO a, b USING 1 AS x, y").kind
        else {
            panic!("expected EXECUTE");
        };
        let TeradataExecute {
            immediate,
            name,
            parameters,
            parenthesized,
            into,
            using,
        } = *execute;
        assert!(immediate);
        assert!(name.is_none());
        assert_eq!(parameters.len(), 1);
        assert!(!parenthesized);
        assert_eq!(into.len(), 2);
        assert_eq!(using.len(), 2);
        assert_eq!(using[0].alias.map(|alias| alias.text), Some("x"));
        assert!(using[1].alias.is_none());
    }

    #[test]
    fn plain_top_uses_the_shared_field() {
        let select = select("SELECT TOP 3 DISTINCT a FROM t");
        assert_eq!(select.modifiers.top, Some("3"));
        assert!(select.modifiers.teradata_top.is_none());
    }

    #[test]
    fn extended_top() {
        let select = select("SELECT TOP (a) PERCENT WITH TIES b FROM t");
        assert_eq!(select.modifiers.top, None);
        let top = select.modifiers.teradata_top.as_ref().unwrap();
        assert!(matches!(top.quantity, TeradataTopQuantity::Expr(_)));
        assert!(top.percent);
        assert!(top.with_ties);

        let select = crate::shape::select("SELECT TOP 5 WITH TIES b FROM t");
        let top = select.modifiers.teradata_top.as_ref().unwrap();
        assert!(matches!(top.quantity, TeradataTopQuantity::Number("5")));
        assert!(!top.percent);
    }

    fn first_item<'a>(select: &'a Select<'a, Builtin>) -> &'a Expr<'a, Builtin> {
        match &select.projection[0].kind {
            SelectItemKind::Expr { expr, .. } => expr,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn booleans_are_identifiers() {
        let select = select("SELECT TRUE");
        assert!(
            matches!(&first_item(&select).kind, ExprKind::Ident(ident) if ident.text == "TRUE")
        );
    }

    #[test]
    fn adjacent_strings_borrow_each_part() {
        let select = select("SELECT 'a''b' 'c'");
        let ExprKind::Ext(BuiltinExpr::Teradata(TeradataExpr::AdjacentStrings(parts))) =
            &first_item(&select).kind
        else {
            panic!("expected adjacent strings");
        };
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].text, "a''b");
        assert_eq!(parts[1].text, "c");
    }

    #[test]
    fn adjacent_strings_have_one_value() {
        let select = select("SELECT 'a''b' 'c'");
        let ExprKind::Ext(BuiltinExpr::Teradata(expr)) = &first_item(&select).kind else {
            panic!("expected adjacent strings");
        };
        assert_eq!(expr.value(), "a'bc");
    }

    #[test]
    fn one_string_is_a_core_literal() {
        let select = select("SELECT 'a'");
        assert!(matches!(&first_item(&select).kind, ExprKind::Literal(_)));
    }

    #[test]
    fn inner_null_treatment_is_kept_apart_from_the_outer_one() {
        let inner = select("SELECT FIRST_VALUE(a IGNORE NULLS) OVER () FROM t");
        let outer = select("SELECT FIRST_VALUE(a) IGNORE NULLS OVER () FROM t");
        assert_ne!(first_item(&inner), first_item(&outer));
    }

    #[test]
    fn core_keeps_true_as_a_literal() {
        let statements = parse_sql::<Core>("SELECT TRUE").unwrap();
        assert_eq!(statements[0].to_string(), "SELECT true");
    }
}
