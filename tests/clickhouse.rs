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

//! ClickHouse: its query clauses, table engines and administrative statements.

mod common;

use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::dialect::{ClickHouse, Core, Hive, MySql, Postgres};
use sqlparser::{parse_expr, parse_query, parse_sql};

fn round_trips(sql: &str) {
    let statements = parse_sql::<ClickHouse>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<ClickHouse>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements[0].to_string(), expected, "{sql}");
}

fn expr_parses_to(sql: &str, expected: &str) {
    let expr = parse_expr::<ClickHouse>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(expr.to_string(), expected, "{sql}");
}

fn rejected(sql: &str) {
    assert!(parse_sql::<ClickHouse>(sql).is_err(), "accepted: {sql}");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("ClickHouse").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
}

#[test]
fn map_access_and_arrays() {
    round_trips(
        "SELECT string_values[indexOf(string_names, 'endpoint')] FROM foos WHERE id = 'test' AND string_value[indexOf(string_name, 'app')] <> 'foo'",
    );
    round_trips("SELECT ['1', '2'] FROM test");
    round_trips("SELECT array(x1, x2) FROM foo");
}

#[test]
fn kill() {
    round_trips("KILL MUTATION 5");
    round_trips("KILL QUERY 5");
    round_trips("KILL CONNECTION 5");
    round_trips("KILL 5");
    rejected("KILL MUTATION");
}

#[test]
fn delimited_identifiers() {
    round_trips(r#"CREATE TABLE "foo" ("bar" "int")"#);
    round_trips(r#"ALTER TABLE foo ADD CONSTRAINT "bar" PRIMARY KEY (baz)"#);
}

#[test]
fn reserved_words_can_be_table_names() {
    for table_name in ["into", "limit", "having", "where", "on"] {
        round_trips(&format!("SELECT * FROM {table_name}"));
        round_trips(&format!("SELECT * FROM t JOIN {table_name} ON true"));
    }
}

#[test]
fn create_table_engine_and_keys() {
    round_trips(r#"CREATE TABLE "x" ("a" "int") ENGINE = MergeTree ORDER BY ("x")"#);
    round_trips(r#"CREATE TABLE "x" ("a" "int") ENGINE = MergeTree ORDER BY "x""#);
    round_trips(
        r#"CREATE TABLE "x" ("a" "int") ENGINE = MergeTree ORDER BY "x" AS SELECT * FROM "t" WHERE true"#,
    );
    parses_to(
        "CREATE TABLE x (a int) ENGINE = MergeTree() ORDER BY a",
        "CREATE TABLE x (a INT) ENGINE = MergeTree ORDER BY a",
    );
    round_trips("CREATE TABLE t (a INT) ENGINE = MergeTree ORDER BY a PARTITION BY a");
    round_trips("CREATE TABLE t (a INT) ENGINE = MergeTree ORDER BY a PARTITION BY a % 64");
    round_trips("CREATE TABLE t (d DATE) ENGINE = MergeTree ORDER BY d PARTITION BY toYYYYMM(d)");
    rejected("CREATE TABLE t (a INT) ENGINE = MergeTree ORDER BY a PARTITION BY");
    round_trips(r#"CREATE TABLE t (a INT) ENGINE = "MergeTree""#);
    round_trips("CREATE TABLE t (a INT) ENGINE = `MergeTree`");
    round_trips("CREATE TABLE t (a INT) ENGINE = ``");
    round_trips("CREATE TABLE t (a INT) ENGINE = MergeTree");
    parses_to(
        "CREATE TABLE t (a INT) ENGINE=MergeTree",
        "CREATE TABLE t (a INT) ENGINE = MergeTree",
    );
    parses_to(
        concat!(
            "CREATE TABLE IF NOT EXISTS \"MyTable\" (`col1` Int64, `col2` Int32) ",
            "ENGINE = MergeTree() ",
            "PRIMARY KEY (toDate(toDateTime(`col2`)), `col1`, `col2`) ",
            "ORDER BY (toDate(toDateTime(`col2`)), `col1`, `col2`) ",
            "PARTITION BY col1 % 64"
        ),
        concat!(
            "CREATE TABLE IF NOT EXISTS \"MyTable\" (`col1` INT64, `col2` Int32) ",
            "ENGINE = MergeTree ",
            "PRIMARY KEY (toDate(toDateTime(`col2`)), `col1`, `col2`) ",
            "ORDER BY (toDate(toDateTime(`col2`)), `col1`, `col2`) ",
            "PARTITION BY col1 % 64"
        ),
    );
    round_trips(concat!(
        "CREATE TABLE db.table (`i` INT, `k` INT)",
        " ENGINE = SharedMergeTree('/clickhouse/tables/{uuid}/{shard}', '{replica}')",
        " PRIMARY KEY tuple(i)",
        " ORDER BY tuple(i)",
    ));
    rejected(concat!(
        "CREATE TABLE db.table (`i` Int, `k` Int)",
        " ORDER BY tuple(i), tuple(k)",
    ));
    rejected("CREATE TABLE events (a UInt8) ENGINE = MergeTree ORDER BY (a");
}

#[test]
fn local_temporary_table_on_commit() {
    round_trips("CREATE LOCAL TEMPORARY TABLE test ON COMMIT PRESERVE ROWS AS SELECT 1");
}

#[test]
fn insert_into_function() {
    round_trips(
        "INSERT INTO TABLE FUNCTION remote('localhost', default.simple_table) VALUES (100, 'inserted via remote()')",
    );
    round_trips(
        "INSERT INTO FUNCTION remote('localhost', default.simple_table) VALUES (100, 'inserted via remote()')",
    );
}

#[test]
fn insert_format_and_settings() {
    for sql in [
        r#"INSERT INTO tbl FORMAT JSONEachRow {"id": 1, "value": "foo"}, {"id": 2, "value": "bar"}"#,
        r#"INSERT INTO tbl FORMAT JSONEachRow ["first", "second", "third"]"#,
        r#"INSERT INTO tbl FORMAT JSONEachRow [{"first": 1}]"#,
        r#"INSERT INTO tbl (foo) FORMAT JSONAsObject {"foo": {"bar": {"x": "y"}, "baz": 1}}"#,
        r#"INSERT INTO tbl (foo, bar) FORMAT JSON {"foo": 1, "bar": 2}"#,
        r#"INSERT INTO tbl FORMAT CSV col1, col2, col3"#,
        r#"INSERT INTO tbl FORMAT LineAsString "I love apple", "I love banana", "I love orange""#,
        r#"INSERT INTO tbl (foo) SETTINGS input_format_json_read_bools_as_numbers = true FORMAT JSONEachRow {"id": 1, "value": "foo"}"#,
        r#"INSERT INTO tbl SETTINGS format_template_resultset = '/some/path/resultset.format', format_template_row = '/some/path/row.format' FORMAT Template"#,
        r#"INSERT INTO tbl SETTINGS input_format_json_read_bools_as_numbers = true FORMAT JSONEachRow {"id": 1, "value": "foo"}"#,
    ] {
        round_trips(sql);
    }
}

#[test]
fn alter_table_partitions() {
    for operation in ["ATTACH", "DETACH"] {
        round_trips(&format!("ALTER TABLE t0 {operation} PARTITION part"));
        round_trips(&format!("ALTER TABLE t1 {operation} PART part"));
        rejected(&format!("ALTER TABLE t0 {operation} PARTITION"));
        rejected(&format!("ALTER TABLE t0 {operation} PART"));
    }
    for operation in ["FREEZE", "UNFREEZE"] {
        round_trips(&format!("ALTER TABLE t {operation} PARTITION '2024-08-14'"));
        round_trips(&format!(
            "ALTER TABLE t {operation} PARTITION '2024-08-14' WITH NAME 'hello'"
        ));
        rejected(&format!("ALTER TABLE t0 {operation} PARTITION"));
        rejected(&format!("ALTER TABLE t0 {operation} PARTITION p0 WITH"));
        rejected(&format!(
            "ALTER TABLE t0 {operation} PARTITION p0 WITH NAME"
        ));
    }
}

#[test]
fn alter_table_projections() {
    round_trips(concat!(
        "ALTER TABLE t0 ADD PROJECTION IF NOT EXISTS my_name",
        " (SELECT a, b GROUP BY a ORDER BY b)",
    ));
    round_trips("ALTER TABLE t0 ADD PROJECTION my_name (SELECT a, b GROUP BY a ORDER BY b)");
    round_trips("ALTER TABLE t0 ADD PROJECTION my_name (SELECT a, b ORDER BY b)");
    round_trips("ALTER TABLE t0 ADD PROJECTION my_name (SELECT a, b GROUP BY a)");
    rejected("ALTER TABLE t0 ADD PROJECTION my_name");
    rejected("ALTER TABLE t0 ADD PROJECTION my_name ()");
    rejected("ALTER TABLE t0 ADD PROJECTION my_name (SELECT)");
    round_trips("ALTER TABLE t0 DROP PROJECTION IF EXISTS my_name");
    round_trips("ALTER TABLE t0 DROP PROJECTION my_name");
    rejected("ALTER TABLE t0 DROP PROJECTION");
    for keyword in ["CLEAR", "MATERIALIZE"] {
        round_trips(&format!(
            "ALTER TABLE t0 {keyword} PROJECTION IF EXISTS my_name IN PARTITION p0"
        ));
        round_trips(&format!(
            "ALTER TABLE t0 {keyword} PROJECTION my_name IN PARTITION p0"
        ));
        round_trips(&format!("ALTER TABLE t0 {keyword} PROJECTION my_name"));
        rejected(&format!("ALTER TABLE t0 {keyword} PROJECTION"));
        rejected(&format!(
            "ALTER TABLE t0 {keyword} PROJECTION my_name IN PARTITION"
        ));
        rejected(&format!("ALTER TABLE t0 {keyword} PROJECTION my_name IN"));
    }
}

#[test]
fn alter_table_columns_and_order() {
    round_trips("ALTER TABLE t ADD COLUMN c Nullable(UInt8) FIRST");
    round_trips("ALTER TABLE t ADD COLUMN c UInt8 DEFAULT 0 AFTER a");
    round_trips("ALTER TABLE t ON CLUSTER cl ADD COLUMN c UInt8 AFTER a");
    round_trips("ALTER TABLE t MODIFY COLUMN c UInt16 FIRST");
    round_trips("ALTER TABLE t ADD COLUMN c UInt8 AFTER `order`");
    round_trips(r#"ALTER TABLE t ADD COLUMN c UInt8 AFTER "order""#);
    round_trips("ALTER TABLE t ADD COLUMN c UInt8 FIRST, ADD COLUMN d UInt8 AFTER c");
    rejected("ALTER TABLE t ADD COLUMN c UInt8 AFTER");
    round_trips("ALTER TABLE events MODIFY ORDER BY (region, user.id)");
    round_trips("ALTER TABLE events MODIFY ORDER BY region");
    round_trips("ALTER TABLE events MODIFY ORDER BY ()");
    round_trips("ALTER TABLE events MODIFY ORDER BY tuple()");
    round_trips("ALTER TABLE events ON CLUSTER c MODIFY ORDER BY (a, toDate(ts))");
    round_trips("ALTER TABLE events ADD COLUMN b UInt8, MODIFY ORDER BY (a, b)");
    round_trips("ALTER TABLE events MODIFY COLUMN `order` UInt8");
    round_trips("ALTER TABLE events MODIFY ORDER BY (a)");
    rejected("ALTER TABLE events MODIFY ORDER BY");
    rejected("ALTER TABLE events MODIFY ORDER BY (a");
    rejected("ALTER TABLE events MODIFY ORDER BY (,)");
    rejected("ALTER TABLE events MODIFY ORDER BY (a) b");
}

#[test]
fn optimize_table() {
    for sql in [
        "OPTIMIZE TABLE t0",
        "OPTIMIZE TABLE db.t0",
        "OPTIMIZE TABLE t0 ON CLUSTER 'cluster'",
        "OPTIMIZE TABLE t0 ON CLUSTER 'cluster' FINAL",
        "OPTIMIZE TABLE t0 FINAL DEDUPLICATE",
        "OPTIMIZE TABLE t0 DEDUPLICATE",
        "OPTIMIZE TABLE t0 DEDUPLICATE BY id",
        "OPTIMIZE TABLE t0 FINAL DEDUPLICATE BY id",
        "OPTIMIZE TABLE t0 PARTITION tuple('2023-04-22') DEDUPLICATE BY id",
        "OPTIMIZE TABLE t0 ON CLUSTER cluster PARTITION ID '2024-07' FINAL DEDUPLICATE BY id",
    ] {
        round_trips(sql);
    }
    rejected("OPTIMIZE TABLE t0 DEDUPLICATE BY");
    rejected("OPTIMIZE TABLE t0 PARTITION");
    rejected("OPTIMIZE TABLE t0 PARTITION ID");
}

#[test]
fn optimize_keeps_databricks_syntax_out() {
    rejected("OPTIMIZE t0");
    rejected("OPTIMIZE TABLE t0 ZORDER BY (a)");
}

#[test]
fn data_types() {
    let sql = concat!(
        "CREATE TABLE table (",
        "a1 UInt8, a2 UInt16, a3 UInt32, a4 UInt64, a5 UInt128, a6 UInt256,",
        " b1 Int8, b2 Int16, b3 Int32, b4 Int64, b5 Int128, b6 Int256,",
        " c1 Float32, c2 Float64,",
        " d1 Date32, d2 DateTime64(3), d3 DateTime64(3, 'UTC'),",
        " e1 FixedString(255),",
        " f1 LowCardinality(Int32)",
        ") ORDER BY (a1)",
    );
    let canonical = sql
        .replace(" Int8", " INT8")
        .replace(" Int64", " INT64")
        .replace(" Float64", " FLOAT64");
    parses_to(sql, &canonical);
    let nullable = r#"CREATE TABLE table (k UInt8, `a` Nullable(String), `b` Nullable(DateTime64(9, 'UTC')), c Nullable(DateTime64(9)), d Date32 NULL) ENGINE = MergeTree ORDER BY (`k`)"#;
    parses_to(nullable, &nullable.replace("String", "STRING"));
    parses_to(
        concat!(
            "CREATE TABLE table (",
            " i Nested(a Array(Int16), b LowCardinality(String)),",
            " k Array(Tuple(FixedString(128), Int128)),",
            " l Tuple(a DateTime64(9), b Array(UUID)),",
            " m Map(String, UInt16)",
            ") ENGINE=MergeTree ORDER BY (k)"
        ),
        concat!(
            "CREATE TABLE table (",
            "i Nested(a Array(Int16), b LowCardinality(STRING)), ",
            "k Array(Tuple(FixedString(128), Int128)), ",
            "l Tuple(a DateTime64(9), b Array(UUID)), ",
            "m Map(STRING, UInt16)",
            ") ENGINE = MergeTree ORDER BY (k)"
        ),
    );
    rejected("CREATE TABLE t (a ARRAY<INT>)");
    round_trips(concat!(
        "CREATE TABLE table (",
        "a DATETIME MATERIALIZED now(),",
        " b DATETIME EPHEMERAL now(),",
        " c DATETIME EPHEMERAL,",
        " d STRING ALIAS toString(c)",
        ") ENGINE = MergeTree"
    ));
}

#[test]
fn object_types() {
    round_trips("CREATE TABLE t (o Object('json'))");
    round_trips("CREATE TABLE t (o OBJECT())");
    round_trips("CREATE TABLE t (o OBJECT(city VARCHAR NOT NULL))");
    round_trips("CREATE TABLE t (o OBJECT(10))");
    round_trips("CREATE TABLE t (o OBJECT(foo))");
    round_trips("CREATE TABLE t (o OBJECT(foo, bar))");
}

#[test]
fn not_null_in_column_options() {
    let canonical = concat!(
        "CREATE TABLE foo (",
        "abc INT DEFAULT (42 IS NOT NULL) NOT NULL,",
        " not_null BOOL MATERIALIZED (abc IS NOT NULL),",
        " CHECK (abc IS NOT NULL)",
        ")",
    );
    round_trips(canonical);
    parses_to(
        concat!(
            "CREATE TABLE foo (",
            "abc INT DEFAULT (42 NOT NULL) NOT NULL,",
            " not_null BOOL MATERIALIZED (abc NOT NULL),",
            " CHECK (abc NOT NULL)",
            ")",
        ),
        canonical,
    );
}

#[test]
fn views() {
    round_trips(r#"CREATE VIEW v (i "int", f "String") AS SELECT * FROM t"#);
    rejected("CREATE VIEW v (i, f) AS SELECT * FROM t");
    round_trips(concat!(
        "CREATE MATERIALIZED VIEW analytics.monthly_aggregated_data_mv ",
        "TO analytics.monthly_aggregated_data ",
        "AS SELECT toDate(toStartOfMonth(event_time)) ",
        "AS month, domain_name, sumState(count_views) ",
        "AS sumCountViews FROM analytics.hourly_data ",
        "GROUP BY domain_name, month"
    ));
}

#[test]
fn double_equal() {
    parses_to(
        "SELECT foo FROM bar WHERE buz == 'buz'",
        "SELECT foo FROM bar WHERE buz = 'buz'",
    );
}

#[test]
fn limit_by() {
    round_trips(
        "SELECT * FROM default.last_asset_runs_mv ORDER BY created_at DESC LIMIT 1 BY asset",
    );
    round_trips(
        "SELECT * FROM default.last_asset_runs_mv ORDER BY created_at DESC LIMIT 1 BY asset, toStartOfDay(created_at)",
    );
    round_trips("SELECT foo FROM bar LIMIT 2, 1");
    rejected(
        "SELECT * FROM default.last_asset_runs_mv ORDER BY created_at DESC BY asset, toStartOfDay(created_at)",
    );
    rejected("SELECT * FROM T OFFSET 5 BY foo");
}

#[test]
fn settings_in_queries() {
    round_trips("SELECT * FROM t SETTINGS max_threads = 1, max_block_size = 10000");
    round_trips("SELECT * FROM t SETTINGS additional_table_filters = {'table_1': 'x != 2'}");
    round_trips(
        "SELECT * FROM t SETTINGS additional_result_filter = 'x != 2', query_plan_optimize_lazy_materialization = false",
    );
    for sql in [
        "SELECT * FROM t SETTINGS a",
        "SELECT * FROM t SETTINGS a=",
        "SELECT * FROM t SETTINGS a=1, b",
        "SELECT * FROM t SETTINGS a=1, b=",
        "SELECT * FROM t SETTINGS a = {",
        "SELECT * FROM t SETTINGS a = {'b'",
        "SELECT * FROM t SETTINGS a = {'b': ",
        "SELECT * FROM t SETTINGS a = {'b': 'c',}",
        "SELECT * FROM t SETTINGS a = {'b': 'c', 'd'}",
        "SELECT * FROM t SETTINGS a = {'b': 'c', 'd': }",
        "SELECT * FROM t SETTINGS a = {ANY(b)}",
    ] {
        rejected(sql);
    }
}

#[test]
fn wildcard_except_and_replace() {
    round_trips("SELECT * EXCEPT (prev_status) FROM anomalies");
    parses_to(
        "SELECT * EXCEPT prev_status FROM anomalies",
        "SELECT * EXCEPT (prev_status) FROM anomalies",
    );
    round_trips("SELECT * REPLACE (upper(name) AS name) FROM source");
}

#[test]
fn parametric_functions() {
    round_trips("SELECT HISTOGRAM(0.5, 0.6)(x, y) FROM t");
}

#[test]
fn order_by_with_fill_and_interpolate() {
    round_trips(concat!(
        "SELECT id, fname, lname FROM customer WHERE id < 5 ",
        "ORDER BY fname ASC NULLS FIRST WITH FILL FROM 10 TO 20 STEP 2, ",
        "lname DESC NULLS LAST WITH FILL FROM 30 TO 40 STEP 3 ",
        "INTERPOLATE (col1 AS col1 + 1) LIMIT 2",
    ));
    rejected(
        "SELECT id, fname, lname FROM customer ORDER BY fname WITH FILL INTERPOLATE (col1 AS col1 + 1) INTERPOLATE (col2 AS col2 + 2)",
    );
    rejected(concat!(
        "SELECT id, fname, lname FROM customer ",
        "ORDER BY fname WITH FILL INTERPOLATE (col1 AS col1 + 1), ",
        "lname WITH FILL INTERPOLATE (col2 AS col2 + 2)",
    ));
    rejected(
        "SELECT id, fname, lname FROM customer ORDER BY fname WITH FILL INTERPOLATE (col1 AS col1 + 1), lname",
    );
    round_trips("SELECT fname FROM customer ORDER BY fname WITH FILL FROM 10 TO 20 STEP 2");
    rejected("SELECT id, fname, lname FROM customer ORDER BY fname WITH FILL FROM TO 20");
    rejected(
        "SELECT id, fname, lname FROM customer ORDER BY fname WITH FILL FROM TO 20, lname WITH FILL FROM TO STEP 1",
    );
    round_trips(
        "SELECT fname FROM customer ORDER BY fname WITH FILL INTERPOLATE (col1 AS col1 + 1, col2 AS col3, col4 AS col4 + 4)",
    );
    round_trips("SELECT fname FROM customer ORDER BY fname WITH FILL INTERPOLATE");
    round_trips("SELECT fname FROM customer ORDER BY fname WITH FILL INTERPOLATE ()");
}

#[test]
fn prewhere() {
    round_trips("SELECT * FROM t PREWHERE x = 1 WHERE y = 2");
    round_trips("SELECT * FROM t PREWHERE x = 1 AND y = 2");
}

#[test]
fn use_statements() {
    for object_name in [
        "mydb",
        "SCHEMA",
        "DATABASE",
        "CATALOG",
        "WAREHOUSE",
        "DEFAULT",
    ] {
        round_trips(&format!("USE {object_name}"));
        for quote in ['"', '`'] {
            round_trips(&format!("USE {quote}{object_name}{quote}"));
        }
    }
}

#[test]
fn format_clause() {
    for format in ["TabSeparated", "JSONCompact", "NULL"] {
        round_trips(&format!("SELECT * FROM t FORMAT {format}"));
    }
    rejected("SELECT * FROM t FORMAT");
    rejected("SELECT * FROM t FORMAT TabSeparated JSONCompact");
    rejected("SELECT * FROM t FORMAT TabSeparated TabSeparated");
}

#[test]
fn table_function_settings() {
    round_trips("SELECT * FROM table_function(arg, SETTINGS s0 = 3, s1 = 's')");
    round_trips("SELECT * FROM table_function(arg)");
    round_trips("SELECT * FROM table_function(SETTINGS s0 = 3, s1 = 's')");
    for sql in [
        "SELECT * FROM t(SETTINGS a)",
        "SELECT * FROM t(SETTINGS a=)",
        "SELECT * FROM t(SETTINGS a=1, b)",
        "SELECT * FROM t(SETTINGS a=1, b=)",
    ] {
        rejected(sql);
    }
}

#[test]
fn describe_and_explain_table() {
    for sql in [
        "DESCRIBE test.table",
        "DESCRIBE TABLE test.table",
        "DESC test.table",
        "DESC TABLE test.table",
        "EXPLAIN TABLE test_identifier",
    ] {
        round_trips(sql);
    }
}

#[test]
fn table_sample() {
    round_trips("SELECT * FROM tbl SAMPLE 0.1");
    round_trips("SELECT * FROM tbl SAMPLE 1000");
    round_trips("SELECT * FROM tbl SAMPLE 1 / 10");
    round_trips("SELECT * FROM tbl SAMPLE 1 / 10 OFFSET 1 / 2");
}

#[test]
fn array_join() {
    round_trips("SELECT x FROM t ARRAY JOIN arr AS x");
    round_trips("SELECT x FROM t JOIN u ON t.id = u.id ARRAY JOIN arr AS x");
    rejected("SELECT x FROM t ARRAY JOIN");
    round_trips("SELECT x FROM t LEFT ARRAY JOIN arr AS x");
    round_trips("SELECT x FROM t INNER ARRAY JOIN arr AS x");
}

#[test]
fn in_without_parentheses() {
    expr_parses_to("x IN 'a'", "x IN ('a')");
    expr_parses_to("x IN (1, 2, 3)", "x IN (1, 2, 3)");
    round_trips("SELECT * FROM t WHERE x IN (SELECT y FROM u)");
    expr_parses_to("x IN {ids:Array(UInt64)}", "x IN ({ids: Array(UInt64)})");
    expr_parses_to(
        "x NOT IN {ids:Array(UInt64)}",
        "x NOT IN ({ids: Array(UInt64)})",
    );
    expr_parses_to("x IN ({ids: Array(UInt64)})", "x IN ({ids: Array(UInt64)})");
    expr_parses_to(
        "x IN ({p: Array(UInt64)}) AND y = 1",
        "x IN ({p: Array(UInt64)}) AND y = 1",
    );
    parses_to(
        "SELECT * FROM t WHERE x IN t2 AND y = 2",
        "SELECT * FROM t WHERE x IN (t2) AND y = 2",
    );
    parses_to(
        "SELECT * FROM t WHERE x IN t2 = 1",
        "SELECT * FROM t WHERE x IN (t2) = 1",
    );
    parses_to(
        "SELECT * FROM t WHERE x IN 1 + 2",
        "SELECT * FROM t WHERE x IN (1 + 2)",
    );
}

#[test]
fn global_in() {
    round_trips("SELECT * FROM t WHERE x GLOBAL IN (1, 2)");
    round_trips("SELECT * FROM t WHERE x GLOBAL NOT IN (SELECT y FROM t2)");
    parses_to(
        "SELECT * FROM t WHERE x GLOBAL IN t2",
        "SELECT * FROM t WHERE x GLOBAL IN (t2)",
    );
    round_trips("SELECT * FROM t WHERE x GLOBAL IN (1) AND y = 2");
    rejected("SELECT * FROM t WHERE x GLOBAL y");
    parses_to(
        "SELECT * FROM t WHERE x GLOBAL IN t2 AND y = 2",
        "SELECT * FROM t WHERE x GLOBAL IN (t2) AND y = 2",
    );
    parses_to(
        "SELECT * FROM t WHERE x GLOBAL NOT IN t2 OR y = 2",
        "SELECT * FROM t WHERE x GLOBAL NOT IN (t2) OR y = 2",
    );
    expr_parses_to("x GLOBAL IN t2 = 1", "x GLOBAL IN (t2) = 1");
}

#[test]
fn tuple_element_access() {
    for (sql, canonical) in [
        ("SELECT t.1 FROM t", "SELECT t . 1 FROM t"),
        (
            "SELECT t.1 AS a, t.2 AS b FROM (SELECT (1, 'x') AS t)",
            "SELECT t . 1 AS a, t . 2 AS b FROM (SELECT (1, 'x') AS t)",
        ),
        ("SELECT (1, 'a').1", "SELECT (1, 'a') . 1"),
        ("SELECT tuple(1, 'a').2", "SELECT tuple(1, 'a') . 2"),
        ("SELECT arr[1].1 FROM t", "SELECT arr[1] . 1 FROM t"),
        ("SELECT `t`.1 FROM t", "SELECT `t` . 1 FROM t"),
        ("SELECT t.1 + 1 FROM t", "SELECT t . 1 + 1 FROM t"),
        (
            "SELECT * FROM t WHERE t.1 = 1",
            "SELECT * FROM t WHERE t . 1 = 1",
        ),
        ("SELECT t.1.2 FROM t", "SELECT t . 1 . 2 FROM t"),
        (
            "SELECT ((1, 2), 3).1.2, tuple(1, (2, 3)).2.1",
            "SELECT ((1, 2), 3) . 1 . 2, tuple(1, (2, 3)) . 2 . 1",
        ),
    ] {
        parses_to(sql, canonical);
    }
    round_trips("SELECT t . 1 FROM t");
    round_trips("SELECT t . 1 . 2 FROM t");
    round_trips("SELECT 1.5, 1 + 0.5");
    round_trips("SELECT .5, 1e3");
    rejected("SELECT t.");
    rejected("SELECT t.1.");
}

#[test]
fn delete_and_update_clauses() {
    round_trips("DELETE FROM db.hits ON CLUSTER c1 WHERE x = 1");
    round_trips("DELETE FROM hits ON CLUSTER '{cluster}' WHERE x = 1");
    round_trips("DELETE FROM hits WHERE x = 1 SETTINGS lightweight_deletes_sync = 0");
    round_trips("DELETE FROM hits ON CLUSTER c1 WHERE x = 1 SETTINGS a = 1, b = 'x'");
    rejected("DELETE FROM hits SETTINGS a = 1 WHERE x = 1");
    rejected("DELETE FROM hits WHERE x = 1 ON CLUSTER c1");
    rejected("DELETE FROM hits ON CLUSTER 1 WHERE x = 1");
    rejected("DELETE FROM hits WHERE x = 1 SETTINGS a");
    round_trips("UPDATE db.hits ON CLUSTER c1 SET x = 1 WHERE y = 2");
    round_trips("UPDATE hits ON CLUSTER '{cluster}' SET x = 1, z = 2 WHERE y = 2");
    round_trips("UPDATE hits SET x = 1 WHERE y = 2 SETTINGS update_parallel_mode = 'auto'");
    round_trips("UPDATE hits ON CLUSTER c1 SET x = 1 WHERE y = 2 SETTINGS a = 1");
    rejected("UPDATE hits SET x = 1 ON CLUSTER c1 WHERE y = 2");
    rejected("UPDATE hits SET x = 1 SETTINGS a = 1 WHERE y = 2");
    rejected("UPDATE hits ON CLUSTER 1 SET x = 1 WHERE y = 2");
    rejected("UPDATE hits SET x = 1 WHERE y = 2 SETTINGS a");
}

#[test]
fn dollar_in_identifiers() {
    round_trips("SELECT a$b, c$$, d$e$ FROM t");
    round_trips("SELECT $$abc$$, $tag$abc$tag$");
    rejected("INSERT I$$$$$$$$$$$$$$$$$$$$$$$$$$");
}

#[test]
fn other_dialects_keep_their_grammar() {
    assert!(parse_sql::<Core>("SELECT * FROM t PREWHERE x = 1").is_err());
    assert!(parse_sql::<Core>("SELECT * FROM t LIMIT 1 BY a").is_err());
    assert!(parse_sql::<Core>("SELECT * FROM t FORMAT JSON").is_err());
    assert!(parse_sql::<Core>("SELECT * FROM t SETTINGS a = 1").is_err());
    assert!(parse_sql::<Core>("SELECT * FROM t ORDER BY a WITH FILL").is_err());
    assert!(parse_sql::<MySql>("OPTIMIZE TABLE t").is_err());
    assert!(parse_sql::<Postgres>("ALTER TABLE t ADD PROJECTION p (SELECT a)").is_err());
    assert!(parse_sql::<Hive>("SELECT x FROM t ARRAY JOIN arr AS x").is_err());
    assert!(parse_expr::<Core>("x IN 'a'").is_err());
    assert!(parse_expr::<Postgres>("x GLOBAL IN (1)").is_err());
    assert!(parse_query::<Postgres>("SELECT t.1 FROM t").is_err());
}

mod shape {
    use sqlparser::ast::*;
    use sqlparser::dialect::ClickHouse;
    use sqlparser::dialect::builtin::{
        Builtin, BuiltinExpr, BuiltinQueryClauses, BuiltinStatement,
    };
    use sqlparser::dialect::clickhouse::{
        ClickHouseAlterOp, ClickHouseExpr, ClickHouseFormat, ClickHouseStatement,
    };
    use sqlparser::{parse_expr, parse_query, parse_sql};

    fn ext<'a>(statement: Statement<'a, Builtin>) -> ClickHouseStatement<'a> {
        match statement.kind {
            StatementKind::Ext(BuiltinStatement::ClickHouse(s)) => s,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn query_clauses_hold_their_parts() {
        let query = parse_query::<ClickHouse>(
            "SELECT 1 FROM t PREWHERE a = 1 ORDER BY b WITH FILL FROM 1 LIMIT 2 BY c SETTINGS x = 1 FORMAT JSON",
        )
        .unwrap();
        let SetExpr::Select(select) = &query.body else {
            panic!("{query:?}");
        };
        assert!(select.prewhere.is_some());
        assert!(query.order_by[0].ext.with_fill.is_some());
        assert_eq!(query.limit.as_ref().unwrap().by.len(), 1);
        let Some(BuiltinQueryClauses::ClickHouse(tail)) = query.tail.0.as_deref() else {
            panic!("no ClickHouse tail");
        };
        assert_eq!(tail.settings.len(), 1);
        assert_eq!(
            tail.format.as_ref().unwrap(),
            &ClickHouseFormat::Name(Ident {
                text: "JSON".into(),
                quote: None,
                span: Span::default(),
            })
        );
    }

    #[test]
    fn group_by_modifiers_chain() {
        let query =
            parse_query::<ClickHouse>("SELECT 1 GROUP BY a WITH ROLLUP WITH TOTALS").unwrap();
        let SetExpr::Select(select) = &query.body else {
            panic!("{query:?}");
        };
        assert_eq!(
            select.group_by_modifier,
            Some(GroupByModifier::Chain(vec![
                GroupByModifier::Rollup,
                GroupByModifier::Totals
            ]))
        );
    }

    #[test]
    fn nested_type_is_held_as_parenthesized_fields() {
        let statements = parse_sql::<ClickHouse>("CREATE TABLE t (a Nested(b Int8))").unwrap();
        let StatementKind::CreateTable(table) = &statements[0].kind else {
            panic!("{:?}", statements[0]);
        };
        assert!(matches!(
            table.columns[0].data_type.angle.as_deref(),
            Some(AngleArguments::ParenFields(fields)) if fields.len() == 1
        ));
    }

    #[test]
    fn tuple_element_is_a_dialect_expression() {
        let expr = parse_expr::<ClickHouse>("t.1").unwrap();
        assert!(matches!(
            expr.kind,
            ExprKind::Ext(BuiltinExpr::ClickHouse(ClickHouseExpr::TupleElement {
                index: std::borrow::Cow::Borrowed("1"),
                ..
            }))
        ));
    }

    #[test]
    fn alter_table_mixes_core_and_dialect_operations() {
        let statement = ext(parse_sql::<ClickHouse>(
            "ALTER TABLE t ON CLUSTER c ADD COLUMN a Int8, DROP PROJECTION p",
        )
        .unwrap()
        .remove(0));
        let ClickHouseStatement::AlterTable(alter) = statement else {
            panic!("{statement:?}");
        };
        assert_eq!(alter.operations.len(), 2);
        assert!(matches!(alter.operations[0], ClickHouseAlterOp::Core(_)));
        assert!(matches!(
            alter.operations[1],
            ClickHouseAlterOp::DropProjection { .. }
        ));
    }

    #[test]
    fn plain_statements_stay_core_nodes() {
        let statements = parse_sql::<ClickHouse>("ALTER TABLE t ADD COLUMN a Int8").unwrap();
        assert!(matches!(statements[0].kind, StatementKind::AlterTable(_)));
        let statements = parse_sql::<ClickHouse>("INSERT INTO t VALUES (1)").unwrap();
        assert!(matches!(statements[0].kind, StatementKind::Insert(_)));
        let statements = parse_sql::<ClickHouse>("DELETE FROM t WHERE a = 1").unwrap();
        assert!(matches!(statements[0].kind, StatementKind::Delete(_)));
        let statements = parse_sql::<ClickHouse>("CREATE VIEW v AS SELECT 1").unwrap();
        assert!(matches!(statements[0].kind, StatementKind::CreateView(_)));
        let statements = parse_sql::<ClickHouse>("CREATE TABLE t (a Int8)").unwrap();
        assert!(matches!(statements[0].kind, StatementKind::CreateTable(_)));
    }
}
