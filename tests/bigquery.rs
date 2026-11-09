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

//! Google BigQuery: GoogleSQL strings, hyphenated names, its DDL and scripting.

use sqlparser::dialect::builtin::{BuiltinExpr, BuiltinStatement, DialectKind, parse_sql_as};
use sqlparser::dialect::{BigQuery, Core};
use sqlparser::{parse_expr, parse_sql};

fn round_trips(sql: &str) {
    let statements = parse_sql::<BigQuery>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<BigQuery>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements[0].to_string(), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<BigQuery>(sql).is_err(), "accepted: {sql}");
}

fn expr_round_trips(sql: &str) {
    let expr = parse_expr::<BigQuery>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(expr.to_string(), sql);
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("bigquery").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
}

#[test]
fn string_literals_in_all_their_quotes() {
    round_trips(concat!(
        "SELECT ",
        "'single', ",
        r#""double", "#,
        "'''triple-single''', ",
        r#""""triple-double""", "#,
        r#"'single\'escaped', "#,
        r#"'''triple-single\'escaped''', "#,
        r#"'''triple-single'unescaped''', "#,
        r#""double\"escaped", "#,
        r#""""triple-double\"escaped""", "#,
        r#""""triple-double"unescaped""", "#,
        r#""""triple-double'unescaped""", "#,
        r#"'''triple-single"unescaped'''"#,
    ));
}

#[test]
fn byte_literals_upper_case_their_prefix() {
    round_trips(concat!(
        "SELECT ",
        "B'abc', ",
        r#"B"abc", "#,
        r#"B'f\(abc,(.*),def\)', "#,
        r#"B"f\(abc,(.*),def\)", "#,
        r#"B'''abc''', "#,
        r#"B"""abc""""#,
    ));
    parses_to(
        r#"SELECT b'123', b"123", b'''123''', b"""123""""#,
        r#"SELECT B'123', B"123", B'''123''', B"""123""""#,
    );
}

#[test]
fn raw_literals_upper_case_their_prefix() {
    round_trips(concat!(
        "SELECT ",
        "R'abc', ",
        r#"R"abc", "#,
        r#"R'f\(abc,(.*),def\)', "#,
        r#"R"f\(abc,(.*),def\)", "#,
        r#"R'''abc''', "#,
        r#"R"""abc""""#,
    ));
    parses_to(
        r#"SELECT r'123', r"123", r'''123''', r"""123""""#,
        r#"SELECT R'123', R"123", R'''123''', R"""123""""#,
    );
    round_trips("SELECT B'it''s'");
    round_trips(r#"SELECT B"it""s""#);
    round_trips("SELECT R'it''s'");
    round_trips(r#"SELECT R"it""s""#);
}

#[test]
fn unterminated_strings_are_rejected() {
    rejected("SELECT 'abc");
    rejected("SELECT '''abc''");
    rejected("SELECT B'abc");
    rejected("SELECT R\"abc");
}

#[test]
fn words_reserved_elsewhere_are_columns_here() {
    round_trips("SELECT OFFSET, EXPLAIN, ANALYZE, SORT, TOP, VIEW FROM T");
    round_trips("SELECT 1 AS OFFSET, 2 AS EXPLAIN, 3 AS ANALYZE FROM T");
}

#[test]
fn at_at_variables() {
    round_trips("SELECT @@error.stack_trace, @@error.message");
}

#[test]
fn begin_blocks() {
    round_trips(
        r#"BEGIN SELECT 1; EXCEPTION WHEN ERROR THEN SELECT 2; RAISE USING MESSAGE = FORMAT('ERR: %s', 'Bad'); END"#,
    );
    round_trips("BEGIN SELECT 1; SELECT 2; EXCEPTION WHEN ERROR THEN SELECT 2; SELECT 4; END");
    round_trips("BEGIN SELECT 1; EXCEPTION WHEN ERROR THEN SELECT @@error.stack_trace; END");
    round_trips("BEGIN EXCEPTION WHEN ERROR THEN SELECT 2; END");
    round_trips("BEGIN SELECT 1; SELECT 2; EXCEPTION WHEN ERROR THEN END");
    round_trips("BEGIN EXCEPTION WHEN ERROR THEN END");
    round_trips("BEGIN SELECT 1; SELECT 2; END");
    round_trips("BEGIN END");
    rejected("BEGIN SELECT 1; SELECT 2 END");
    rejected("BEGIN SELECT 1; EXCEPTION WHEN ERROR THEN SELECT 2 END");
}

#[test]
fn begin_transaction_is_not_a_block() {
    round_trips("BEGIN TRANSACTION");
    round_trips("BEGIN");
}

#[test]
fn raise_statements() {
    round_trips("RAISE");
    round_trips("RAISE USING MESSAGE = 'x'");
}

#[test]
fn delete_needs_no_from() {
    round_trips(r#"DELETE "table" WHERE 1"#);
    round_trips("DELETE FROM t WHERE 1");
}

#[test]
fn create_view_with_options() {
    round_trips(concat!(
        "CREATE VIEW myproject.mydataset.newview ",
        r#"(name, age OPTIONS(description = "field age")) "#,
        r#"OPTIONS(expiration_timestamp = TIMESTAMP_ADD(CURRENT_TIMESTAMP(), INTERVAL 48 HOUR), "#,
        r#"friendly_name = "newview", description = "a view that expires in 2 days", labels = [("org_unit", "development")]) "#,
        "AS SELECT column_1, column_2, column_3 FROM myproject.mydataset.mytable",
    ));
    round_trips("CREATE VIEW IF NOT EXISTS mydataset.newview AS SELECT foo FROM bar");
    round_trips("CREATE VIEW IF NOT EXISTS my-pro-ject.mydataset.myview AS SELECT 1");
}

#[test]
fn create_table_with_options() {
    round_trips("CREATE TABLE my-pro-ject.mydataset.mytable (x INT64)");
    round_trips(concat!(
        "CREATE TABLE mydataset.newtable ",
        r#"(x INT64 NOT NULL OPTIONS(description = "field x"), "#,
        r#"y BOOL OPTIONS(description = "field y")) "#,
        "PARTITION BY _PARTITIONDATE ",
        "CLUSTER BY userid, age ",
        r#"OPTIONS(partition_expiration_days = 1, description = "table option description")"#
    ));
    round_trips(concat!(
        "CREATE TABLE mydataset.newtable ",
        r#"(x INT64 NOT NULL OPTIONS(description = "field x"), "#,
        r#"y BOOL OPTIONS(description = "field y")) "#,
        "CLUSTER BY userid ",
        r#"OPTIONS(partition_expiration_days = 1, "#,
        r#"description = "table option description")"#
    ));
    round_trips("CREATE TABLE foo (x INT64) OPTIONS()");
    round_trips(
        "CREATE TABLE db.schema.test (x INT64 OPTIONS(description = 'An optional INTEGER field')) OPTIONS()",
    );
    round_trips(
        "CREATE TABLE my_table (f0 STRING, f1 STRUCT<a STRING, b INT64>, f2 STRING) PARTITION BY f0",
    );
}

#[test]
fn trailing_commas_in_lists_of_columns() {
    parses_to(
        "CREATE TABLE t (a INT64, b INT64, )",
        "CREATE TABLE t (a INT64, b INT64)",
    );
    parses_to(
        "CREATE TABLE t (a STRUCT<x INT64>, )",
        "CREATE TABLE t (a STRUCT<x INT64>)",
    );
}

#[test]
fn create_external_table() {
    round_trips(
        "CREATE EXTERNAL TABLE dataset_id.table1 (hvr_tx_seq STRING) OPTIONS(format = 'CSV')",
    );
    round_trips(
        "CREATE EXTERNAL TABLE dataset_id.table1 (hvr_tx_seq STRING) OPTIONS(format = 'CSV', allow_quoted_newlines = true, encoding = 'UTF8')",
    );
    parses_to(
        concat!(
            "CREATE OR REPLACE EXTERNAL TABLE `proj.ds.tbl` ",
            "WITH CONNECTION `projects/proj/locations/us/connections/c` ",
            r#"OPTIONS(format = "ICEBERG", uris = ["gs://b/m.json"])"#,
        ),
        concat!(
            "CREATE OR REPLACE EXTERNAL TABLE `proj`.`ds`.`tbl` () ",
            "WITH CONNECTION `projects/proj/locations/us/connections/c` ",
            r#"OPTIONS(format = "ICEBERG", uris = ["gs://b/m.json"])"#,
        ),
    );
    parses_to(
        "CREATE EXTERNAL TABLE t WITH CONNECTION c",
        "CREATE EXTERNAL TABLE t () WITH CONNECTION c",
    );
    round_trips(concat!(
        "CREATE EXTERNAL TABLE t (a INT64, b STRING) ",
        r#"WITH CONNECTION c OPTIONS(uris = ["gs://x"])"#,
    ));
    round_trips(r#"CREATE EXTERNAL TABLE t (a INT64) OPTIONS(uris = ["gs://x"])"#);
}

#[test]
fn create_snapshot_table() {
    round_trips("CREATE SNAPSHOT TABLE dataset_id.table1 CLONE dataset_id.table2");
    round_trips("CREATE SNAPSHOT TABLE IF NOT EXISTS dataset_id.table1 CLONE dataset_id.table2");
    round_trips(
        "CREATE SNAPSHOT TABLE dataset_id.table1 CLONE dataset_id.table2 FOR SYSTEM_TIME AS OF TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 1 HOUR)",
    );
    round_trips(
        "CREATE SNAPSHOT TABLE dataset_id.table1 CLONE dataset_id.table2 OPTIONS(expiration_timestamp = TIMESTAMP '2025-01-01 00:00:00 UTC', friendly_name = 'my_table')",
    );
    round_trips(
        "CREATE SNAPSHOT TABLE IF NOT EXISTS dataset_id.table1 CLONE dataset_id.table2 FOR SYSTEM_TIME AS OF TIMESTAMP_SUB(CURRENT_TIMESTAMP(), INTERVAL 1 HOUR) OPTIONS(expiration_timestamp = TIMESTAMP '2025-01-01 00:00:00 UTC')",
    );
}

#[test]
fn nested_data_types() {
    round_trips(
        "CREATE TABLE table (x STRUCT<a ARRAY<INT64>, b BYTES(42)>, y ARRAY<STRUCT<INT64>>)",
    );
    round_trips("CREATE TABLE foo (x ANY)");
    round_trips(
        "CREATE TABLE my_table (f0 STRING, f1 STRUCT<a STRING, b STRUCT<c INT64, d STRING>>, f2 STRING)",
    );
    round_trips(concat!(
        "CREATE TABLE my_table (",
        "f0 STRING, ",
        "f1 STRUCT<a STRING, b STRUCT<c INT64, d STRUCT<e STRING>>>, ",
        "f2 STRUCT<h STRING, i STRUCT<j INT64, k STRUCT<l STRUCT<m STRING>>>>, ",
        "f3 STRUCT<e STRING, f STRUCT<c INT64>>",
        ")",
    ));
    rejected("CREATE TABLE my_table(f1 STRUCT<a STRING, b INT64)");
    rejected("CREATE TABLE my_table(f1 STRUCT<a STRING, b INT64>>)");
    rejected("CREATE TABLE my_table(f1 STRUCT<a STRUCT<b INT>>>, c INT64)");
    rejected("SELECT STRUCT<INT64>>(NULL)");
    rejected("SELECT STRUCT<STRUCT<INT64>>>(NULL)");
    rejected("CREATE TABLE table (x STRUCT<STRUCT<INT64>>>)");
}

#[test]
fn struct_field_options() {
    round_trips(concat!(
        "CREATE TABLE my_table (",
        "f0 STRUCT<a STRING, b INT64>, ",
        "f1 STRUCT<",
        "a STRING OPTIONS(description = 'This is a string', type = 'string'), ",
        "b INT64",
        "> OPTIONS(description = 'This is a struct field')",
        ")",
    ));
}

#[test]
fn tuple_literals() {
    round_trips("SELECT (1, 2, 3), (1, 1.0, '123', true)");
}

#[test]
fn typeless_struct_literals() {
    round_trips(
        "SELECT STRUCT(1, 2, 3), STRUCT('abc'), STRUCT(1, t.str_col), STRUCT(1 AS a, 'abc' AS b), STRUCT(str_col AS abc)",
    );
    round_trips("SELECT STRUCT(STRUCT(1 AS a, NULL AS b) AS c, NULL AS d).c.a");
}

#[test]
fn typed_struct_literals() {
    round_trips(
        "SELECT STRUCT<INT64>(5), STRUCT<x INT64, y STRING>(1, t.str_col), STRUCT<arr ARRAY<FLOAT64>, str STRUCT<BOOL>>(nested_col)",
    );
    round_trips("SELECT STRUCT<x STRUCT, y ARRAY<STRUCT>>(nested_col)");
    round_trips("SELECT STRUCT<BOOL>(true), STRUCT<BYTES(42)>(B'abc')");
    round_trips(
        r#"SELECT STRUCT<DATE>("2011-05-05"), STRUCT<DATETIME>(DATETIME '1999-01-01 01:23:34.45'), STRUCT<FLOAT64>(5.0), STRUCT<INT64>(1)"#,
    );
    round_trips(
        r#"SELECT STRUCT<INTERVAL>(INTERVAL '2' HOUR), STRUCT<JSON>(JSON '{"class" : {"students" : [{"name" : "Jane"}]}}')"#,
    );
    round_trips(
        r#"SELECT STRUCT<STRING(42)>("foo"), STRUCT<TIMESTAMP>(TIMESTAMP '2008-12-25 15:30:00 America/Los_Angeles'), STRUCT<TIME>(TIME '15:30:00')"#,
    );
    round_trips("SELECT STRUCT<NUMERIC>(NUMERIC '1'), STRUCT<BIGNUMERIC>(BIGNUMERIC '1')");
    round_trips("SELECT STRUCT<key INT64, value INT64>(1, 2)");
    round_trips(r#"SELECT STRUCT<x INT64>(5), STRUCT<y STRING>("foo")"#);
    round_trips("SELECT STRUCT<x INT64, y INT64>(5, 5)");
    round_trips("SELECT STRUCT<x INT64>(5), STRUCT<y STRING>('foo')");
    round_trips(
        r#"SELECT STRUCT<STRING(42)>('foo'), STRUCT<TIMESTAMP>(TIMESTAMP '2008-12-25 15:30:00 America/Los_Angeles'), STRUCT<TIME>(TIME '15:30:00')"#,
    );
    rejected("SELECT STRUCT<x INT64>(1 AS y)");
}

#[test]
fn table_names() {
    let table = |name: &str| format!("SELECT 1 FROM {name}");
    for name in [
        "`spa ce`",
        "`!@#$%^&*()-=_+`",
        "_5abc.dataField",
        "`5abc`.dataField",
        "abc5.dataField",
        "`GROUP`.dataField",
        "abc5.GROUP",
        "foo-bar.baz-123",
    ] {
        round_trips(&table(name));
    }
    for (name, canonical) in [
        ("`foo.bar.baz`", "`foo`.`bar`.`baz`"),
        ("`foo.bar`.`baz`", "`foo`.`bar`.`baz`"),
        ("`foo`.`bar.baz`", "`foo`.`bar`.`baz`"),
        ("`foo`.`bar`.`baz`", "`foo`.`bar`.`baz`"),
        ("`5abc.dataField`", "`5abc`.`dataField`"),
        ("`_5abc.da-sh-es`", "`_5abc`.`da-sh-es`"),
    ] {
        parses_to(&table(name), &table(canonical));
    }
    for name in [
        "5abc.dataField",
        "abc5!.dataField",
        "foo-`bar`",
        "`foo`-bar",
        "foo-123a",
        "foo - bar",
        "123-bar",
        "bar-",
    ] {
        rejected(&table(name));
    }
}

#[test]
fn hyphenated_tables_only_in_table_position() {
    parses_to(
        "select * from foo-bar f join baz-qux b on f.id = b.id",
        "SELECT * FROM foo-bar f JOIN baz-qux b ON f.id = b.id",
    );
    parses_to("select * from foo-123.bar", "SELECT * FROM foo-123.bar");
    parses_to("SELECT foo-bar.x FROM t", "SELECT foo - bar.x FROM t");
}

#[test]
fn time_travel() {
    round_trips("SELECT 1 FROM t1 FOR SYSTEM_TIME AS OF '2023-08-18 23:08:18'");
    rejected("SELECT 1 FROM t1 FOR SYSTEM TIME AS OF 'some_timestamp'");
}

#[test]
fn unnest_tables() {
    round_trips("SELECT * FROM t1 JOIN UNNEST(t1.a) AS f ON c1 = c2");
    round_trips("SELECT * FROM UNNEST(expr) WITH OFFSET");
    round_trips("SELECT * FROM UNNEST(expr) AS numbers WITH OFFSET");
    round_trips("SELECT * FROM UNNEST(expr) AS numbers WITH OFFSET AS off");
}

#[test]
fn in_unnest_expression() {
    round_trips("SELECT * FROM customers WHERE segment IN UNNEST(expr)");
    round_trips("SELECT * FROM customers WHERE segment NOT IN UNNEST(expr)");
}

#[test]
fn merge_clauses() {
    round_trips(concat!(
        "MERGE inventory AS T USING newArrivals AS S ON false ",
        "WHEN NOT MATCHED AND 1 THEN INSERT (product, quantity) VALUES (1, 2) ",
        "WHEN NOT MATCHED BY TARGET AND 1 THEN INSERT (product, quantity) VALUES (1, 2) ",
        "WHEN NOT MATCHED BY TARGET THEN INSERT (product, quantity) VALUES (1, 2) ",
        "WHEN NOT MATCHED BY SOURCE AND 2 THEN DELETE ",
        "WHEN NOT MATCHED BY SOURCE THEN DELETE ",
        "WHEN NOT MATCHED BY SOURCE AND 1 THEN UPDATE SET a = 1, b = 2 ",
        "WHEN NOT MATCHED AND 1 THEN INSERT (product, quantity) ROW ",
        "WHEN NOT MATCHED THEN INSERT (product, quantity) ROW ",
        "WHEN NOT MATCHED AND 1 THEN INSERT ROW ",
        "WHEN NOT MATCHED THEN INSERT ROW ",
        "WHEN MATCHED AND 1 THEN DELETE ",
        "WHEN MATCHED THEN UPDATE SET a = 1, b = 2 ",
        "WHEN NOT MATCHED THEN INSERT (a, b) VALUES (1, DEFAULT) ",
        "WHEN NOT MATCHED THEN INSERT VALUES (1, DEFAULT)",
    ));
    rejected("MERGE T USING U ON TRUE WHEN NOT MATCHED BY SOURCE THEN INSERT(a) VALUES (b)");
    rejected("MERGE INTO T USING U ON TRUE WHEN NOT MATCHED BY TARGET THEN DELETE");
    rejected("MERGE INTO T USING U ON TRUE WHEN NOT MATCHED BY TARGET THEN UPDATE SET a = b");
    rejected("MERGE T USING U ON TRUE WHEN MATCHED BY TARGET AND 1 THEN DELETE");
    rejected("MERGE T USING U ON TRUE WHEN MATCHED BY SOURCE AND 1 THEN DELETE");
}

#[test]
fn trailing_commas_in_the_select_list() {
    for (sql, canonical) in [
        ("SELECT a,", "SELECT a"),
        ("SELECT 1,", "SELECT 1"),
        ("SELECT 1,2,", "SELECT 1, 2"),
        ("SELECT a, b,", "SELECT a, b"),
        ("SELECT a, b AS c,", "SELECT a, b AS c"),
        ("SELECT a, b AS c, FROM t", "SELECT a, b AS c FROM t"),
        ("SELECT a, b, FROM t", "SELECT a, b FROM t"),
        ("SELECT a, b, LIMIT 1", "SELECT a, b LIMIT 1"),
        ("SELECT a, (SELECT 1, )", "SELECT a, (SELECT 1)"),
    ] {
        parses_to(sql, canonical);
    }
}

#[test]
fn casts() {
    round_trips("SELECT SAFE_CAST(1 AS INT64)");
    round_trips(
        "SELECT CAST(date_valid_from AS DATE FORMAT 'YYYY-MM-DD') AS date_valid_from FROM foo",
    );
    round_trips("SELECT CAST(TIME '21:30:00' AS STRING FORMAT 'PM') AS date_time_to_string");
    round_trips(
        "SELECT CAST(TIMESTAMP '2008-12-25 00:00:00+00:00' AS STRING FORMAT 'TZH' AT TIME ZONE 'Asia/Kolkata') AS date_time_to_string",
    );
    round_trips("SELECT CAST('Hello' AS BYTES FORMAT 'ASCII') AS string_to_bytes");
    round_trips(
        r"SELECT CAST(B'\x48\x65\x6c\x6c\x6f' AS STRING FORMAT 'ASCII') AS bytes_to_string",
    );
    round_trips("SELECT SAFE_CAST(1 AS INT64 FORMAT 'x')");
}

#[test]
fn aggregate_clauses() {
    round_trips("SELECT ARRAY_AGG(x ORDER BY x) AS a FROM T");
    round_trips("SELECT ARRAY_AGG(x ORDER BY x LIMIT 2) FROM tbl");
    round_trips("SELECT ARRAY_AGG(DISTINCT x ORDER BY x LIMIT 2) FROM tbl");
    expr_round_trips("ARRAY_AGG(state)");
    expr_round_trips("ARRAY_CONCAT_AGG(x LIMIT 2)");
    expr_round_trips("ARRAY_AGG(state IGNORE NULLS LIMIT 10)");
    expr_round_trips("ARRAY_AGG(state RESPECT NULLS ORDER BY population)");
    expr_round_trips("ARRAY_AGG(DISTINCT state IGNORE NULLS ORDER BY population DESC LIMIT 10)");
    expr_round_trips("ARRAY_CONCAT_AGG(x ORDER BY ARRAY_LENGTH(x))");
    expr_round_trips("ANY_VALUE(fruit)");
    expr_round_trips(
        "ANY_VALUE(fruit) OVER (ORDER BY LENGTH(fruit) ROWS BETWEEN 1 PRECEDING AND CURRENT ROW)",
    );
    expr_round_trips("ANY_VALUE(fruit HAVING MAX sold)");
    expr_round_trips("ANY_VALUE(fruit HAVING MIN sold)");
}

#[test]
fn declare_statements() {
    round_trips("DECLARE x INT64");
    round_trips("DECLARE x INT64 DEFAULT 42");
    round_trips("DECLARE x, y, z INT64 DEFAULT 42");
    round_trips("DECLARE x DEFAULT 42");
    rejected("DECLARE x");
    rejected("DECLARE x 42");
}

#[test]
fn map_access() {
    expr_round_trips("users[-1][safe_offset(2)].a.b");
    round_trips("SELECT myfunc()[-1].a[SAFE_OFFSET(2)].b");
}

#[test]
fn create_function() {
    round_trips(concat!(
        "CREATE OR REPLACE TEMPORARY FUNCTION ",
        "project1.mydataset.myfunction(x FLOAT64) ",
        "RETURNS FLOAT64 ",
        "OPTIONS(x = 'y') ",
        "AS 42"
    ));
    for sql in [
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "RETURNS ARRAY<FLOAT64> ",
            "OPTIONS(a = [1, 2], b = 'two', c = [('k1', 'v1'), ('k2', 'v2')]) ",
            "AS ((SELECT 1 FROM mytable))"
        ),
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "RETURNS ARRAY<FLOAT64> ",
            "AS ((SELECT 1 FROM mytable)) ",
            "OPTIONS(a = [1, 2], b = 'two', c = [('k1', 'v1'), ('k2', 'v2')])",
        ),
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION IF NOT EXISTS ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "RETURNS ARRAY<FLOAT64> ",
            "OPTIONS(a = [1, 2]) ",
            "AS ((SELECT 1 FROM mytable))"
        ),
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "OPTIONS(a = [1, 2]) ",
            "AS ((SELECT 1 FROM mytable))"
        ),
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "DETERMINISTIC ",
            "LANGUAGE js ",
            "OPTIONS(a = [1, 2]) ",
            "AS \"console.log('hello');\""
        ),
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "NOT DETERMINISTIC ",
            "LANGUAGE js ",
            "AS \"console.log('hello');\" ",
            "OPTIONS(a = [1, 2])",
        ),
        concat!(
            "CREATE OR REPLACE TEMPORARY FUNCTION ",
            "myfunction(a FLOAT64, b INT64, c STRING) ",
            "RETURNS INT64 ",
            "REMOTE WITH CONNECTION us.myconnection ",
            "OPTIONS(a = [1, 2])",
        ),
    ] {
        round_trips(sql);
    }
    round_trips(concat!(
        "CREATE OR REPLACE TEMPORARY FUNCTION ",
        "my_function(param1 ANY TYPE) ",
        "AS (",
        "(SELECT 1)",
        ")",
    ));
    rejected(concat!(
        "CREATE TEMPORARY FUNCTION myfunction() ",
        "OPTIONS(a = [1, 2]) ",
        "AS ((SELECT 1 FROM mytable)) ",
        "OPTIONS(a = [1, 2])",
    ));
    rejected("CREATE TEMPORARY FUNCTION myfunction() IMMUTABLE AS ((SELECT 1 FROM mytable)) ");
    rejected("CREATE TEMPORARY FUNCTION myfunction() AS \"console.log('hello');\" LANGUAGE js ");
}

#[test]
fn trim_with_a_comma() {
    round_trips(
        r#"SELECT customer_id, TRIM(item_price_id, '"', "a") AS item_price_id FROM models_staging.subscriptions"#,
    );
    round_trips("SELECT TRIM('xyz', 'a')");
    rejected("SELECT TRIM('xyz' 'a')");
}

#[test]
fn extract_with_a_week_start() {
    round_trips("SELECT EXTRACT(WEEK(MONDAY) FROM d)");
    round_trips("SELECT EXTRACT(DAY FROM d)");
}

#[test]
fn select_expr_star() {
    round_trips("SELECT STRUCT<STRING>((SELECT foo FROM T WHERE true)).* FROM T");
    round_trips("SELECT [STRUCT<STRING>('foo')][0].* EXCEPT (foo) FROM T");
    round_trips("SELECT myfunc()[0].* FROM T");
}

#[test]
fn select_as_struct_and_value() {
    round_trips("SELECT * FROM (SELECT AS STRUCT STRUCT(123 AS a, false AS b))");
    round_trips("SELECT * FROM (SELECT DISTINCT AS STRUCT STRUCT(123 AS a, false AS b))");
    parses_to(
        "SELECT * FROM (SELECT ALL AS STRUCT STRUCT(123 AS a, false AS b))",
        "SELECT * FROM (SELECT AS STRUCT STRUCT(123 AS a, false AS b))",
    );
    round_trips("SELECT AS STRUCT 1 AS a, 2 AS b");
    round_trips(
        "SELECT * FROM (SELECT AS VALUE STRUCT(5 AS star_rating, false AS up_down_rating))",
    );
    round_trips(
        "SELECT * FROM (SELECT DISTINCT AS VALUE STRUCT(5 AS star_rating, false AS up_down_rating))",
    );
    parses_to(
        "SELECT * FROM (SELECT ALL AS VALUE STRUCT(5 AS star_rating, false AS up_down_rating))",
        "SELECT * FROM (SELECT AS VALUE STRUCT(5 AS star_rating, false AS up_down_rating))",
    );
    round_trips("SELECT AS VALUE STRUCT(1 AS a, 2 AS b) AS xyz");
}

#[test]
fn triple_quoted_typed_strings() {
    expr_round_trips(r#"JSON '''{"foo":"bar's"}'''"#);
    expr_round_trips(r#"JSON """{"foo":"bar's"}""""#);
}

#[test]
fn export_data() {
    let query = "SELECT field1, field2 FROM mydataset.table1 ORDER BY field1 LIMIT 10";
    round_trips(&format!(
        "EXPORT DATA OPTIONS(uri = 'gs://bucket/folder/*', format = 'PARQUET', overwrite = true) AS {query}"
    ));
    round_trips(&format!(
        "EXPORT DATA WITH CONNECTION myconnection.myproject.us OPTIONS(uri = 'gs://bucket/folder/*', format = 'PARQUET', overwrite = true) AS {query}"
    ));
    rejected(&format!("EXPORT DATA OPTIONS() AS {query}"));
    rejected(&format!("EXPORT DATA AS {query}"));
}

#[test]
fn alter_schema() {
    round_trips("ALTER SCHEMA mydataset SET DEFAULT COLLATE 'und:ci'");
    round_trips("ALTER SCHEMA mydataset ADD REPLICA 'us'");
    round_trips("ALTER SCHEMA mydataset ADD REPLICA 'us' OPTIONS (location = 'us')");
    round_trips("ALTER SCHEMA mydataset DROP REPLICA 'us'");
    round_trips("ALTER SCHEMA mydataset SET OPTIONS (location = 'us')");
    round_trips("ALTER SCHEMA IF EXISTS mydataset SET OPTIONS (location = 'us')");
}

#[test]
fn from_first_queries() {
    round_trips("FROM t");
    round_trips("FROM t SELECT a, b");
    round_trips("FROM t |> WHERE a > 1 |> SELECT a");
}

#[test]
fn comments_with_a_hash() {
    parses_to("SELECT 1 # trailing\n", "SELECT 1");
}

#[test]
fn set_with_parentheses() {
    round_trips("SET (a, b) = ((SELECT 22 FROM tbl1), (SELECT 33 FROM tbl3))");
    parses_to(
        "SET (a) = (SELECT 22 FROM tbl1)",
        "SET (a) = ((SELECT 22 FROM tbl1))",
    );
    round_trips("SET (a, b) = (1, 2)");
}

#[test]
fn named_window_can_name_another_window() {
    round_trips("SELECT * FROM mytable WINDOW window1 AS window2");
}

#[test]
fn window_filter_is_not_available() {
    rejected("SELECT COUNT(*) FILTER (WHERE i > 0) FROM t");
}

#[test]
fn clauses_and_outer_null_treatment_do_not_combine() {
    rejected("SELECT LAG(1 IGNORE NULLS) IGNORE NULLS OVER () FROM t1");
}

#[test]
fn group_by_with_modifiers_are_not_bigquery() {
    rejected("SELECT a FROM t GROUP BY a WITH ROLLUP");
}

#[test]
fn bare_array_type_is_rejected() {
    rejected("SELECT CAST(a AS ARRAY) FROM t");
}

#[test]
fn copy_grants_is_not_bigquery() {
    rejected("CREATE OR REPLACE TABLE my_table COPY GRANTS (a number)");
}

#[test]
fn only_array_takes_a_query_argument() {
    rejected("SELECT func(SELECT 1, 2)");
}

#[test]
fn interval_is_not_a_column() {
    rejected("SELECT * FROM t WHERE interval = 78");
    rejected("SELECT MAX(interval) FROM tbl");
    round_trips("SELECT DATE_ADD(d, INTERVAL n DAY)");
    round_trips("SELECT INTERVAL (1 + 1) HOUR");
}

#[test]
fn with_offset_only_follows_unnest() {
    rejected("SELECT * FROM t WITH OFFSET");
    rejected("SELECT * FROM LATERAL UNNEST([1]) AS n WITH OFFSET");
}

#[test]
fn star_except_can_omit_parentheses() {
    parses_to(
        "SELECT * EXCEPT prev_status FROM anomalies",
        "SELECT * EXCEPT (prev_status) FROM anomalies",
    );
}

#[test]
fn strings_can_name_things() {
    round_trips(r#"SELECT "col1" FROM "test_schema"."test_table""#);
    round_trips(r#"CREATE TABLE "A"."B"."C" (v INT64)"#);
}

#[test]
fn struct_field_with_colon_loses_it() {
    parses_to(
        "CREATE TABLE t (col1 STRUCT<a: INT, b: STRING>)",
        "CREATE TABLE t (col1 STRUCT<a INT, b STRING>)",
    );
}

#[test]
fn create_schema_with_options() {
    round_trips("CREATE SCHEMA IF NOT EXISTS a DEFAULT COLLATE 'und:ci' OPTIONS()");
    round_trips("CREATE SCHEMA a OPTIONS(location = 'us')");
    round_trips("CREATE SCHEMA a");
}

#[test]
fn trailing_comma_in_view_columns() {
    parses_to(
        "CREATE VIEW v (a, b, ) AS SELECT 1",
        "CREATE VIEW v (a, b) AS SELECT 1",
    );
}

#[test]
fn execute_immediate() {
    round_trips("EXECUTE IMMEDIATE 'SELECT 1'");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' INTO a USING 1 AS b");
}

#[test]
fn assert_statements() {
    round_trips("ASSERT (SELECT COUNT(*) FROM my_table) > 0");
    round_trips("ASSERT (SELECT COUNT(*) FROM my_table) > 0 AS 'No rows in my_table'");
}

#[test]
fn group_by_all_and_rollup() {
    round_trips("SELECT a, COUNT(1) FROM t GROUP BY ALL");
    round_trips("SELECT a, COUNT(1) FROM t GROUP BY ROLLUP (a)");
}

#[test]
fn qualify_clause() {
    round_trips("SELECT a FROM t QUALIFY ROW_NUMBER() OVER (ORDER BY a) = 1");
}

#[test]
fn add_column_if_not_exists() {
    round_trips("ALTER TABLE t ADD COLUMN IF NOT EXISTS a INT64");
}

#[test]
fn select_star_except_and_replace() {
    round_trips("SELECT * EXCEPT (a) FROM t");
    round_trips("SELECT * REPLACE (a + 1 AS a) FROM t");
    round_trips("SELECT t.* EXCEPT (a) REPLACE (b AS c) FROM t");
}

#[test]
fn array_literal_and_subquery_function() {
    round_trips("SELECT [1, 2, 3]");
    round_trips("SELECT ARRAY(SELECT 1)");
}

#[test]
fn bracket_arrays_stay_in_this_dialect() {
    assert!(parse_sql::<Core>("SELECT [1, 2]").is_err());
    assert!(parse_sql::<Core>("SELECT @@x").is_err());
    assert!(parse_sql::<Core>("DECLARE x INT64").is_err());
}

mod shape {
    use super::*;
    use sqlparser::ast::{
        AngleArguments, ColumnOption, Expr, ExprKind, FunctionArg, Literal, ObjectName,
        SelectItemKind, SetExpr, Statement, StatementKind, StringPrefix, TableFactor,
    };
    use sqlparser::dialect::bigquery::{BigQueryExpr, BigQueryStatement};
    use sqlparser::dialect::builtin::Builtin;

    fn only(sql: &str) -> Statement<'_, Builtin> {
        parse_sql::<BigQuery>(sql).unwrap().remove(0)
    }

    fn first_item<'a>(statement: &'a Statement<'a, Builtin>) -> &'a Expr<'a, Builtin> {
        let StatementKind::Query(query) = &statement.kind else {
            panic!("expected a query: {statement:?}");
        };
        let SetExpr::Select(select) = &query.body else {
            panic!("expected a select");
        };
        let SelectItemKind::Expr { expr, .. } = &select.projection[0].kind else {
            panic!("expected an expression");
        };
        expr
    }

    fn bigquery_statement<'a>(statement: &'a Statement<'a, Builtin>) -> &'a BigQueryStatement<'a> {
        let StatementKind::Ext(BuiltinStatement::BigQuery(statement)) = &statement.kind else {
            panic!("expected a BigQuery statement: {statement:?}");
        };
        statement
    }

    fn name_text(name: &ObjectName<'_>) -> Vec<(String, Option<char>)> {
        name.parts
            .iter()
            .map(|part| (part.text.to_string(), part.quote))
            .collect()
    }

    #[test]
    fn string_literals_record_prefix_and_quotes() {
        let statement = only(r#"SELECT B'''a''', R"b", '''c''', 'd'"#);
        let StatementKind::Query(query) = &statement.kind else {
            panic!();
        };
        let SetExpr::Select(select) = &query.body else {
            panic!();
        };
        let literals: Vec<_> = select
            .projection
            .iter()
            .map(|item| match &item.kind {
                SelectItemKind::Expr {
                    expr:
                        Expr {
                            kind: ExprKind::Literal(Literal::String(s)),
                            ..
                        },
                    ..
                } => (s.text, s.quote, s.triple, s.prefix),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            literals,
            vec![
                ("a", '\'', true, StringPrefix::Byte),
                ("b", '"', false, StringPrefix::Raw),
                ("c", '\'', true, StringPrefix::None),
                ("d", '\'', false, StringPrefix::None),
            ]
        );
    }

    #[test]
    fn string_values_follow_the_literal_form() {
        let values = |sql: &'static str| {
            let expr = parse_expr::<BigQuery>(sql).unwrap();
            let ExprKind::Literal(Literal::String(string)) = expr.kind else {
                panic!();
            };
            string.value().into_owned()
        };
        assert_eq!(values(r"'it\'s'"), "it's");
        assert_eq!(values(r"r'a\nb'"), r"a\nb");
        assert_eq!(values("'''a''b'''"), "a''b");
        assert_eq!(values(r"'''a\nb'''"), "a\nb");
    }

    #[test]
    fn hyphenated_and_dotted_table_names() {
        let statement = only("SELECT 1 FROM foo-bar.`a.b`");
        let StatementKind::Query(query) = &statement.kind else {
            panic!();
        };
        let SetExpr::Select(select) = &query.body else {
            panic!();
        };
        let TableFactor::Named(name) = &select.from[0].relation.factor else {
            panic!();
        };
        assert_eq!(
            name_text(name),
            vec![
                ("foo-bar".to_string(), None),
                ("a".to_string(), Some('`')),
                ("b".to_string(), Some('`')),
            ]
        );
    }

    #[test]
    fn begin_block_holds_statements_and_handlers() {
        let statement = only("BEGIN SELECT 1; EXCEPTION WHEN ERROR THEN SELECT 2; SELECT 3; END");
        let BigQueryStatement::Block(block) = bigquery_statement(&statement) else {
            panic!();
        };
        assert_eq!(block.statements.len(), 1);
        assert_eq!(block.handlers.len(), 1);
        assert_eq!(block.handlers[0].statements.len(), 2);
    }

    #[test]
    fn struct_literal_keeps_field_names() {
        let statement = only("SELECT STRUCT<x INT64, BOOL>(1, true), STRUCT(1 AS a)");
        let expr = first_item(&statement);
        let ExprKind::Ext(BuiltinExpr::BigQuery(BigQueryExpr::Struct(literal))) = &expr.kind else {
            panic!("{expr:?}");
        };
        let ty = literal.ty.as_ref().unwrap();
        let Some(AngleArguments::Fields(fields)) = ty.angle.as_deref() else {
            panic!("{ty:?}");
        };
        assert_eq!(fields[0].name.unwrap().text, "x");
        assert!(fields[1].name.is_none());
        assert_eq!(literal.values.len(), 2);
    }

    #[test]
    fn select_modifiers_record_as_struct() {
        let statement = only("SELECT AS VALUE STRUCT(1 AS a)");
        let StatementKind::Query(query) = &statement.kind else {
            panic!();
        };
        let SetExpr::Select(select) = &query.body else {
            panic!();
        };
        assert_eq!(select.modifiers.bigquery_as, Some("VALUE"));
    }

    #[test]
    fn column_options_are_structured() {
        let statement = only("CREATE TABLE t (a INT64 OPTIONS(description = 'x'))");
        let StatementKind::CreateTable(create) = &statement.kind else {
            panic!();
        };
        let ColumnOption::Options(options) = &create.columns[0].options[0] else {
            panic!();
        };
        assert_eq!(options[0].name.text, "description");
    }

    #[test]
    fn function_call_with_clauses_is_a_dialect_node() {
        let expr = parse_expr::<BigQuery>("ARRAY_AGG(x LIMIT 2)").unwrap();
        assert!(matches!(
            expr.kind,
            ExprKind::Ext(BuiltinExpr::BigQuery(BigQueryExpr::Call(_)))
        ));
        let expr = parse_expr::<BigQuery>("ARRAY_AGG(x)").unwrap();
        assert!(matches!(expr.kind, ExprKind::Function { .. }));
    }

    #[test]
    fn plain_create_table_uses_the_core_node() {
        let statement = only("CREATE TABLE t (a INT64)");
        assert!(matches!(statement.kind, StatementKind::CreateTable(_)));
    }

    #[test]
    fn typed_string_records_the_type() {
        let statement = only("SELECT BIGNUMERIC '1'");
        let expr = first_item(&statement);
        let ExprKind::TypedString(typed) = &expr.kind else {
            panic!("{expr:?}");
        };
        assert_eq!(typed.ty.name.text, "BIGNUMERIC");
    }

    #[test]
    fn function_args_keep_their_values() {
        let expr = parse_expr::<BigQuery>("ANY_VALUE(x HAVING MAX y)").unwrap();
        let ExprKind::Ext(BuiltinExpr::BigQuery(BigQueryExpr::Call(call))) = &expr.kind else {
            panic!("{expr:?}");
        };
        assert!(matches!(call.args[0], FunctionArg::Expr(_)));
        assert!(call.having.as_ref().unwrap().0);
    }
}
