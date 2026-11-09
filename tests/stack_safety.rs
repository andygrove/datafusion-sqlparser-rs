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

//! The deepest and tallest SQL each rule accepts must be safe to parse, clone,
//! compare, format and render on a 2 MiB stack in a debug build.
//!
//! A failure here aborts the test process instead of reporting a panic. Each
//! shape is listed as text around a nested copy of itself, so a new recursive
//! rule only needs a new row.

use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{
    BigQuery, ClickHouse, Core, Databricks, Dialect, DuckDb, Hive, MsSql, MySql, Oracle, Postgres,
    Redshift, Snowflake, Spark, Sqlite, Teradata,
};
use sqlparser::parse_sql;

/// Far past any limit, so finding the deepest accepted SQL also proves that
/// the rule is rejected rather than overflowing when it is too deep.
const TOO_DEEP: usize = 4_000;
const TOO_TALL: usize = 8_000;

fn exercise<D: Dialect<Ext = Builtin>>(sql: &str) -> bool {
    let Ok(tree) = parse_sql::<D>(sql) else {
        return false;
    };
    let copy = tree.clone();
    assert!(copy == tree);
    assert!(!format!("{tree:?}").is_empty());
    for statement in &tree {
        assert!(!statement.to_string().is_empty());
        assert!(!format!("{statement:#}").is_empty());
    }
    #[cfg(feature = "visitor")]
    visit(tree);
    #[cfg(feature = "serde")]
    round_trip(&copy);
    true
}

/// Serializes the tree and reads it back, with no limit on the JSON's depth.
#[cfg(feature = "serde")]
fn round_trip(tree: &[sqlparser::ast::Statement<'_, Builtin>]) {
    use sqlparser::ast::{Statement, StringArena, deserialize_with_arena};

    let json = serde_json::to_string(tree).unwrap();
    let arena = StringArena::new();
    let mut deserializer = serde_json::Deserializer::from_str(&json);
    deserializer.disable_recursion_limit();
    let back: Vec<Statement<'_, Builtin>> =
        deserialize_with_arena(&arena, &mut deserializer).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), json);
}

/// Walks the tree with both kinds of visitor, with every hook left at its default.
#[cfg(feature = "visitor")]
fn visit<'a>(mut tree: Vec<sqlparser::ast::Statement<'a, Builtin>>) {
    use sqlparser::ast::{Visitor, VisitorMut};

    struct Noop;

    impl<'a> Visitor<'a, Builtin> for Noop {
        type Break = ();
    }

    impl<'a> VisitorMut<'a, Builtin> for Noop {
        type Break = ();
    }

    assert!(sqlparser::ast::Visit::visit(&tree, &mut Noop).is_continue());
    assert!(sqlparser::ast::VisitMut::visit(&mut tree, &mut Noop).is_continue());
}

/// Exercises the largest count of repeats that parses, assuming that a count
/// which parses means every smaller count does too.
fn exercise_largest<D: Dialect<Ext = Builtin>>(limit: usize, sql: impl Fn(usize) -> String) {
    assert!(
        parse_sql::<D>(&sql(limit)).is_err(),
        "accepted without limit: {}",
        sql(1)
    );
    let (mut low, mut high) = (1, limit);
    assert!(parse_sql::<D>(&sql(low)).is_ok(), "rejected: {}", sql(1));
    while high - low > 1 {
        let middle = (low + high) / 2;
        if parse_sql::<D>(&sql(middle)).is_ok() {
            low = middle;
        } else {
            high = middle;
        }
    }
    assert!(exercise::<D>(&sql(low)));
}

fn nesting<D: Dialect<Ext = Builtin>>(start: &str, open: &str, inner: &str, close: &str) {
    exercise_largest::<D>(TOO_DEEP, |depth| {
        format!(
            "{start}{}{inner}{}",
            open.repeat(depth),
            close.repeat(depth)
        )
    });
}

fn chain<D: Dialect<Ext = Builtin>>(start: &str, link: &str) {
    exercise_largest::<D>(TOO_TALL, |count| format!("{start}{}", link.repeat(count)));
}

/// Nesting through expressions: (text before, text after).
const EXPRESSIONS: [(&str, &str); 21] = [
    ("(", ")"),
    ("-", ""),
    ("NOT ", ""),
    ("f(", ")"),
    ("f(a, ", ")"),
    ("f(a ORDER BY ", ")"),
    ("f() OVER (ORDER BY ", ")"),
    ("f() OVER (PARTITION BY ", ")"),
    ("f() OVER (ROWS ", " PRECEDING)"),
    ("f() WITHIN GROUP (ORDER BY ", ")"),
    ("CAST(", " AS int)"),
    ("TRY_CAST(", " AS int)"),
    ("CASE WHEN ", " THEN 1 END"),
    ("CASE ", " WHEN 1 THEN 1 END"),
    ("ARRAY[", "]"),
    ("EXTRACT(YEAR FROM ", ")"),
    ("SUBSTRING(", " FROM 1)"),
    ("TRIM(BOTH a FROM ", ")"),
    ("POSITION(", " IN a)"),
    ("OVERLAY(a PLACING ", " FROM 1)"),
    ("CEIL(", " TO DAY)"),
];

/// Nesting through queries and tables: (text once, before, innermost, after).
const QUERIES: [(&str, &str, &str, &str); 7] = [
    ("", "(SELECT ", "1", ")"),
    ("SELECT ", "EXISTS (SELECT ", "1", ")"),
    ("SELECT ", "1 IN (SELECT ", "1", ")"),
    ("", "SELECT 1 FROM (", "SELECT 1", ") AS x"),
    ("SELECT 1 FROM ", "(t JOIN ", "u", " ON 1)"),
    ("", "WITH x AS (", "SELECT 1", ") SELECT 1"),
    ("", "VALUES ((", "SELECT 1", "))"),
];

/// Links that each wrap the expression before them.
const EXPRESSION_CHAINS: [&str; 6] = [
    " = ANY(a)",
    " <> ALL(a)",
    "[1]",
    " COLLATE y",
    " AT TIME ZONE 'UTC'",
    " + 1",
];

fn every_common_shape<D: Dialect<Ext = Builtin>>() {
    for (open, close) in EXPRESSIONS {
        nesting::<D>("SELECT ", open, "1", close);
    }
    for (start, open, inner, close) in QUERIES {
        nesting::<D>(start, open, inner, close);
    }
    for link in EXPRESSION_CHAINS {
        chain::<D>("SELECT x", link);
    }
}

#[test]
fn core_shapes() {
    every_common_shape::<Core>();
}

#[test]
fn mysql_shapes() {
    every_common_shape::<MySql>();
}

#[test]
fn mysql_nesting() {
    nesting::<MySql>(
        "",
        "CREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW ",
        "SELECT 1",
        "",
    );
    nesting::<MySql>(
        "",
        "CREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW BEGIN ",
        "SELECT 1",
        "; END",
    );
    nesting::<MySql>("SELECT ", "CONVERT(", "1", ", CHAR)");
    nesting::<MySql>("SELECT ", "CONVERT(", "1", " USING utf8)");
    nesting::<MySql>("SELECT ", "BINARY ", "1", "");
    nesting::<MySql>("SELECT ", "1 MEMBER OF(", "1", ")");
    nesting::<MySql>("SELECT ", "@a := ", "1", "");
    exercise_largest::<MySql>(TOO_DEEP, |depth| {
        format!(
            "SELECT * FROM JSON_TABLE('[]', '$' COLUMNS({}a INT PATH '$'{}))",
            "NESTED PATH '$' COLUMNS (".repeat(depth),
            ")".repeat(depth)
        )
    });
    chain::<MySql>("SELECT x", " MEMBER OF(1)");
}

#[test]
fn postgres_shapes() {
    every_common_shape::<Postgres>();
}

#[test]
fn redshift_shapes() {
    every_common_shape::<Redshift>();
}

#[test]
fn snowflake_shapes() {
    every_common_shape::<Snowflake>();
}

#[test]
fn spark_shapes() {
    every_common_shape::<Spark>();
}

#[test]
fn databricks_shapes() {
    every_common_shape::<Databricks>();
}

#[test]
fn databricks_from_first_queries() {
    nesting::<Databricks>("", "FROM (", "FROM t", ")");
}

#[test]
fn databricks_time_travel() {
    nesting::<Databricks>("", "SELECT 1 FROM t TIMESTAMP AS OF (", "SELECT 1", ")");
}

#[test]
fn hive_shapes() {
    every_common_shape::<Hive>();
}

#[test]
fn hive_partition_values() {
    nesting::<Hive>("ALTER TABLE t DROP PARTITION ", "(", "a = 1", ")");
    nesting::<Hive>(
        "WITH c AS (SELECT 1) FROM c INSERT INTO TABLE t ",
        "SELECT (",
        "1",
        ")",
    );
}

#[test]
fn teradata_shapes() {
    every_common_shape::<Teradata>();
}

#[test]
fn teradata_statements() {
    nesting::<Teradata>("", "SELECT TOP ((", "SELECT 1", ")) 1");
    nesting::<Teradata>("EXECUTE a", "(", "1", ")");
    nesting::<Teradata>("EXECUTE IMMEDIATE ", "(", "1", ")");
    nesting::<Teradata>("EXECUTE a USING ", "(", "1", ")");
    nesting::<Teradata>("CREATE TABLE t AS ", "(", "SELECT 1", ")");
    nesting::<Teradata>("SELECT ", "FIRST_VALUE(", "1", " IGNORE NULLS)");
}

#[test]
fn oracle_shapes() {
    every_common_shape::<Oracle>();
}

#[test]
fn oracle_prior_operators() {
    nesting::<Oracle>("SELECT 1 FROM t CONNECT BY ", "PRIOR ", "a", "");
}

#[test]
fn oracle_insert_targets() {
    exercise_largest::<Oracle>(TOO_DEEP, |depth| {
        format!(
            "INSERT INTO (SELECT 1 FROM {}t{}) VALUES (1)",
            "(SELECT 1 FROM ".repeat(depth),
            ")".repeat(depth)
        )
    });
}

#[test]
fn mssql_shapes() {
    every_common_shape::<MsSql>();
}

#[test]
fn mssql_control_of_flow() {
    nesting::<MsSql>("", "IF 1 = 1 ", "SELECT 1", "");
    nesting::<MsSql>("", "IF 1 = 1 SELECT 1; ELSE ", "SELECT 2", "");
    nesting::<MsSql>("", "IF 1 = 1 BEGIN ", "SELECT 1", " END");
    nesting::<MsSql>("", "WHILE 1 = 0 BEGIN ", "SELECT 1;", " END;");
    nesting::<MsSql>("", "BEGIN ", "SELECT 1;", " END;");
}

#[test]
fn mssql_procedures_and_triggers() {
    nesting::<MsSql>("", "CREATE PROCEDURE p AS BEGIN ", "SELECT 1;", " END;");
    nesting::<MsSql>(
        "",
        "CREATE TRIGGER t ON u AFTER INSERT AS BEGIN ",
        "SELECT 1;",
        " END;",
    );
}

#[test]
fn mssql_method_chain() {
    chain::<MsSql>("SELECT (x)", ".value('.')");
}

#[test]
fn bigquery_shapes() {
    every_common_shape::<BigQuery>();
}

#[test]
fn bigquery_expressions() {
    nesting::<BigQuery>("SELECT ", "[", "1", "]");
    nesting::<BigQuery>("SELECT ", "STRUCT(", "1", ")");
    nesting::<BigQuery>("SELECT ", "STRUCT<INT64>(", "1", ")");
    nesting::<BigQuery>("SELECT ", "SAFE_CAST(", " 1 AS INT64 FORMAT 'x'", ")");
    nesting::<BigQuery>("SELECT ", "ARRAY_AGG(a LIMIT ", "1", ")");
    nesting::<BigQuery>("SELECT ", "ANY_VALUE(a HAVING MAX ", "1", ")");
    nesting::<BigQuery>("SELECT ", "ARRAY(SELECT ", "1", ")");
    chain::<BigQuery>("SELECT (x)", ".a");
    chain::<BigQuery>("SELECT x", " NOT IN UNNEST(a)");
}

#[test]
fn bigquery_types() {
    nesting::<BigQuery>("SELECT CAST(a AS ", "ARRAY<", "INT64", ">)");
    nesting::<BigQuery>("SELECT CAST(a AS ", "STRUCT<x ", "INT64", ">)");
}

#[test]
fn bigquery_blocks() {
    nesting::<BigQuery>("", "BEGIN ", "SELECT 1;", " END;");
    nesting::<BigQuery>("", "BEGIN EXCEPTION WHEN e THEN ", "SELECT 1;", " END;");
}

#[test]
fn clickhouse_shapes() {
    every_common_shape::<ClickHouse>();
}

#[test]
fn clickhouse_expressions() {
    nesting::<ClickHouse>("SELECT ", "[", "1", "]");
    nesting::<ClickHouse>("SELECT ", "{'k': ", "1", "}");
    nesting::<ClickHouse>("SELECT 1 FROM t SETTINGS s = ", "{'k': ", "1", "}");
    nesting::<ClickHouse>("SELECT ", "x -> ", "1", "");
    nesting::<ClickHouse>("SELECT ", "(x Int8) -> ", "1", "");
    nesting::<ClickHouse>("SELECT ", "f(0.5)(", "1", ")");
    nesting::<ClickHouse>("SELECT a IN ", "(", "1", ")");
    nesting::<ClickHouse>("SELECT a GLOBAL IN ", "(", "1", ")");
    nesting::<ClickHouse>("SELECT ", "1 GLOBAL IN (SELECT ", "1", ")");
    nesting::<ClickHouse>("SELECT 1 ORDER BY a WITH FILL FROM ", "(", "1", ")");
    nesting::<ClickHouse>("SELECT 1 ORDER BY a INTERPOLATE (b AS ", "(", "1", "))");
    chain::<ClickHouse>("SELECT x", ".1");
    chain::<ClickHouse>("SELECT x", " . 1");
    chain::<ClickHouse>("SELECT 1 IN x", " = 1");
}

#[test]
fn clickhouse_types() {
    nesting::<ClickHouse>("SELECT CAST(a AS ", "Array(", "Int8", "))");
    nesting::<ClickHouse>("SELECT CAST(a AS ", "Nullable(", "Int8", "))");
    nesting::<ClickHouse>("SELECT CAST(a AS ", "Tuple(b ", "Int8", "))");
    nesting::<ClickHouse>("SELECT CAST(a AS ", "Nested(b ", "Int8", "))");
    nesting::<ClickHouse>("SELECT CAST(a AS ", "Map(Int8, ", "Int8", "))");
}

#[test]
fn clickhouse_statements() {
    nesting::<ClickHouse>("CREATE TABLE t (a Int8 DEFAULT ", "(", "1", "))");
    nesting::<ClickHouse>("OPTIMIZE TABLE t PARTITION ", "(", "1", ")");
    nesting::<ClickHouse>("ALTER TABLE t ATTACH PARTITION ", "(", "1", ")");
    nesting::<ClickHouse>("ALTER TABLE t MODIFY ORDER BY ", "(", "1", ")");
    nesting::<ClickHouse>("INSERT INTO t FORMAT x ", "[", "1", "]");
    nesting::<ClickHouse>("DELETE FROM t WHERE a SETTINGS s = ", "(", "1", ")");
}

#[test]
fn duckdb_shapes() {
    every_common_shape::<DuckDb>();
}

#[test]
fn duckdb_literals() {
    nesting::<DuckDb>("SELECT ", "[", "1", "]");
    nesting::<DuckDb>("SELECT ", "{'a': ", "1", "}");
    nesting::<DuckDb>("SELECT ", "MAP {1: ", "1", "}");
    nesting::<DuckDb>("SELECT ", "MAP {", "1", ": 1}");
    nesting::<DuckDb>("SELECT ", "lambda x : ", "1", "");
    nesting::<DuckDb>("SELECT ", "x -> ", "1", "");
    nesting::<DuckDb>("SELECT ", "a[1:", "1", "]");
}

#[test]
fn duckdb_types() {
    nesting::<DuckDb>("SELECT x::", "STRUCT(a ", "INT", ")");
    nesting::<DuckDb>("SELECT x::", "UNION(a ", "INT", ")");
    nesting::<DuckDb>("SELECT x::", "MAP(INT, ", "INT", ")");
}

#[test]
fn duckdb_postfix_chains() {
    chain::<DuckDb>("SELECT x", ".f()");
    chain::<DuckDb>("SELECT [1]", ".f");
    chain::<DuckDb>("SELECT x", "[1:2]");
    chain::<DuckDb>("SELECT x", "[1]");
    chain::<DuckDb>("SELECT x", " // 1");
}

#[test]
fn duckdb_macro_bodies() {
    nesting::<DuckDb>("CREATE MACRO f() AS ", "(", "1", ")");
}

#[test]
fn field_access_chain() {
    chain::<Postgres>("SELECT (x)", ".a");
}

/// Nesting PostgreSQL adds through expressions, statements and `FROM`.
#[test]
fn postgres_nesting() {
    nesting::<Postgres>("SELECT ", "@ ", "1", "");
    nesting::<Postgres>("SELECT ", "~ ", "1", "");
    nesting::<Postgres>("SELECT ", "ARRAY(SELECT ", "1", ")");
    nesting::<Postgres>("SELECT ", "XMLFOREST(", "1", ")");
    nesting::<Postgres>("SELECT ", "XMLPARSE(CONTENT ", "'x'", ")");
    nesting::<Postgres>("SELECT ", "JSON_OBJECT('a' VALUE ", "1", ")");
    nesting::<Postgres>("SELECT 1 FROM t JOIN ", "a JOIN ", "b ON 1", " ON 1");
    nesting::<Postgres>("", "PREPARE p AS ", "SELECT 1", "");
    nesting::<Postgres>("", "CREATE RULE r AS ON UPDATE TO t DO ", "SELECT 1", "");
    nesting::<Postgres>(
        "",
        "CREATE FUNCTION f() BEGIN ATOMIC ",
        "SELECT 1;",
        " END;",
    );
    nesting::<Postgres>("", "WITH x AS (INSERT INTO t ", "SELECT 1", ") SELECT 1");
}

#[test]
fn postgres_chains() {
    chain::<Postgres>("SELECT x", " !");
    chain::<Postgres>("SELECT x", " OPERATOR(+) 1");
    chain::<Postgres>("SELECT x", " ||- 1");
}

#[test]
fn redshift_table_paths() {
    chain::<Redshift>("SELECT 1 FROM a", "[0]");
}

#[test]
fn snowflake_option_values() {
    nesting::<Snowflake>("CREATE STAGE s ", "a=(", "b=1", ")");
}

#[test]
fn snowflake_table_clauses() {
    let start = "SELECT 1 FROM t";
    chain::<Snowflake>(start, " MATCH_RECOGNIZE(PATTERN (a) DEFINE a AS 1)");
    chain::<Snowflake>(start, " PIVOT(SUM(a) FOR b IN ('x'))");
    chain::<Snowflake>(start, " UNPIVOT(a FOR b IN (c))");
    chain::<Snowflake>(start, " SAMPLE (1)");
}

#[test]
fn spark_table_clauses() {
    let start = "SELECT 1 FROM t";
    chain::<Spark>(start, " PIVOT(SUM(a) FOR b IN ('x'))");
    chain::<Spark>(start, " UNPIVOT(a FOR b IN (c))");
    chain::<Spark>(start, " TABLESAMPLE (1)");
}

#[test]
fn spark_types() {
    nesting::<Spark>("SELECT CAST(a AS ", "ARRAY<", "INT", ">)");
}

#[test]
fn sqlite_shapes() {
    every_common_shape::<Sqlite>();
}

/// Nesting SQLite adds: trigger bodies, tables in parentheses and `IN` targets.
#[test]
fn sqlite_nesting() {
    nesting::<Sqlite>(
        "",
        "CREATE TRIGGER t INSERT ON x BEGIN ",
        "SELECT 1; ",
        "END; ",
    );
    nesting::<Sqlite>("SELECT 1 FROM ", "(", "t", ")");
    nesting::<Sqlite>("SELECT 1 FROM ", "(", "t INDEXED BY i", ")");
    nesting::<Sqlite>("SELECT ", "1 IN (", "2", ")");
    nesting::<Sqlite>("SELECT ", "~", "1", "");
    nesting::<Sqlite>("SELECT ", "a NOT REGEXP (", "1", ")");
    chain::<Sqlite>("SELECT x", " IS y");
    chain::<Sqlite>("SELECT x", " NOTNULL");
    chain::<Sqlite>("SELECT x", " IN ()");
    chain::<Sqlite>("SELECT x", " NOT IN t");
    chain::<Sqlite>("SELECT x", "::int");
}

#[test]
fn snowflake_nesting() {
    nesting::<Snowflake>("SELECT ", "f(SELECT ", "1", ")");
    nesting::<Snowflake>("SELECT ", "{'a': ", "1", "}");
    nesting::<Snowflake>("SELECT ", "FILTER(a, x -> ", "1", ")");
    nesting::<Snowflake>("SELECT ", "FILTER(a, x INT -> ", "1", ")");
    nesting::<Snowflake>("SELECT a::", "ARRAY(", "INT", ")");
    nesting::<Snowflake>("SELECT a::", "OBJECT(a ", "INT", ")");
    chain::<Snowflake>("SELECT * FROM a", " JOIN b");
}

#[test]
fn shared_grammar_shapes() {
    chain::<Postgres>("SELECT x", "[1:2]");
    chain::<DuckDb>("SELECT x", "[1:2:3]");
    let xml_open = "XMLTABLE((SELECT 1 FROM ";
    let xml_close = ") PASSING a COLUMNS c INT)";
    nesting::<Postgres>("SELECT 1 FROM ", xml_open, "t", xml_close);
    nesting::<Oracle>("SELECT 1 FROM ", xml_open, "t", xml_close);
    nesting::<Teradata>("SELECT 1 FROM ", xml_open, "t", xml_close);
    nesting::<Postgres>(
        "UPDATE t SET a = 1 FROM (",
        "SELECT 1 FROM (",
        "SELECT 1",
        ") AS x)",
    );
    nesting::<Postgres>(
        "DELETE FROM t USING (",
        "SELECT 1 FROM (",
        "SELECT 1",
        ") AS x)",
    );
    nesting::<Spark>("SELECT 1 FROM t GROUP BY ", "(", "a", ")");
    nesting::<Snowflake>("ALTER USER u SET a=", "(k=", "1", ")");
    nesting::<MySql>("SELECT ", "{fn f(", "1", ")}");
}
