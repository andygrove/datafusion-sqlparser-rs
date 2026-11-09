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

//! Apache Hive: what it adds to Spark SQL, and what stays out of each.

use sqlparser::dialect::Hive;
use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::parse_sql;

fn round_trips(sql: &str) {
    let statements = parse_sql::<Hive>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<Hive>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements[0].to_string(), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Hive>(sql).is_err(), "accepted: {sql}");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("Hive").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
}

#[test]
fn create_table_clauses_round_trip() {
    round_trips(
        r#"CREATE TABLE IF NOT EXISTS db.table (a BIGINT, b STRING, c TIMESTAMP) PARTITIONED BY (d STRING, e TIMESTAMP) STORED AS ORC LOCATION 's3://...' TBLPROPERTIES ("prop" = "2", "asdf" = '1234', 'asdf' = "1234", "asdf" = 2)"#,
    );
    round_trips(
        r#"CREATE TABLE IF NOT EXISTS db.table (a BIGINT, b STRING, c TIMESTAMP) PARTITIONED BY (d STRING, e TIMESTAMP) STORED AS INPUTFORMAT 'org.apache.hadoop.hive.ql.io.orc.OrcInputFormat' OUTPUTFORMAT 'org.apache.hadoop.hive.ql.io.orc.OrcOutputFormat' LOCATION 's3://...'"#,
    );
    round_trips(
        r#"CREATE EXTERNAL TABLE IF NOT EXISTS db.table (a STRING, b STRING, c STRING) PARTITIONED BY (d STRING, e STRING) ROW FORMAT SERDE 'org.apache.hadoop.hive.serde.config' WITH SERDEPROPERTIES ('prop_a' = 'a', 'prop_b' = 'b') STORED AS TEXTFILE LOCATION 's3://...' TBLPROPERTIES ('prop_c' = 'c')"#,
    );
    round_trips("CREATE EXTERNAL TABLE t (c INT)");
    round_trips("CREATE TABLE db.table_name LIKE db.other_table");
    round_trips("CREATE TEMPORARY TABLE db.table (a INT NOT NULL)");
    parses_to(
        "CREATE TEMP TABLE db.table (a INT NOT NULL)",
        "CREATE TEMPORARY TABLE db.table (a INT NOT NULL)",
    );
    round_trips(
        "CREATE TABLE tab (cola STRING, colb BIGINT) ROW FORMAT DELIMITED FIELDS TERMINATED BY '\t' ESCAPED BY '\"' MAP KEYS TERMINATED BY '\"'",
    );
    round_trips(
        "CREATE TABLE target (kv_map MAP<STRING, STRING> COMMENT 'kv col comment') COMMENT 'this is table comment'",
    );
}

#[test]
fn table_comment_comes_before_partitioning() {
    round_trips(
        "CREATE TABLE db.table_name (a INT, b STRING) COMMENT 'table comment' PARTITIONED BY (a INT, b STRING) CLUSTERED BY (a, b) SORTED BY (a ASC, b DESC) INTO 4 BUCKETS",
    );
    rejected(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) COMMENT 'table comment'",
    );
}

#[test]
fn clustered_by_needs_its_parts() {
    round_trips(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) CLUSTERED BY (a, b) SORTED BY (a ASC, b DESC) INTO 4 BUCKETS",
    );
    round_trips(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) CLUSTERED BY (a, b) INTO 4 BUCKETS",
    );
    rejected(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) CLUSTERED BY (a, b)",
    );
    rejected(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) CLUSTERED BY () INTO 4 BUCKETS",
    );
    rejected(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) CLUSTERED BY (a, b) SORTED BY INTO 4 BUCKETS",
    );
    rejected(
        "CREATE TABLE db.table_name (a INT, b STRING) PARTITIONED BY (a INT, b STRING) CLUSTERED BY (a, b) SORTED BY (a ASC, b DESC) INTO",
    );
}

#[test]
fn describe_round_trips() {
    round_trips("DESCRIBE namespace.`table`");
    round_trips("DESCRIBE namespace.table");
    round_trips("DESCRIBE table");
    round_trips("DESCRIBE FORMATTED test.table");
    round_trips("DESCRIBE EXTENDED test.table");
}

#[test]
fn insert_forms_round_trip() {
    round_trips(
        "INSERT OVERWRITE TABLE db.new_table PARTITION (a = '1', b) SELECT a, b, c FROM db.table",
    );
    round_trips(
        "WITH a AS (SELECT * FROM b) INSERT INTO TABLE db.table_table PARTITION (a) SELECT * FROM b",
    );
    round_trips(
        "INSERT INTO db.table_name PARTITION (a, b) (c, d) SELECT a, b, c, d FROM db.table",
    );
    round_trips(
        "INSERT OVERWRITE LOCAL DIRECTORY '/home/blah' STORED AS TEXTFILE SELECT * FROM db.table",
    );
}

#[test]
fn from_first_insert_follows_a_cte() {
    round_trips(
        "WITH cte AS (SELECT * FROM a.b) FROM cte INSERT INTO TABLE a.b PARTITION (a) SELECT *",
    );
}

#[test]
fn maintenance_statements_round_trip() {
    round_trips("TRUNCATE TABLE db.table");
    round_trips(
        "ANALYZE TABLE db.table_name PARTITION (a = '1234', b) COMPUTE STATISTICS NOSCAN CACHE METADATA",
    );
    round_trips(
        "ANALYZE TABLE db.table_name PARTITION (a = '1234', b) COMPUTE STATISTICS FOR COLUMNS",
    );
    round_trips("MSCK REPAIR TABLE db.table_name ADD PARTITIONS");
    round_trips("MSCK REPAIR TABLE db.table_name");
    round_trips("DROP TABLE db.table_name PURGE");
}

#[test]
fn set_round_trips() {
    round_trips("SET HIVEVAR:name = a, b, c_d");
    round_trips("SET hive.tez.java.opts = -Xmx4g");
    rejected("SET hive.tez.java.opts = -");
}

#[test]
fn alter_table_partitions_round_trip() {
    round_trips("ALTER TABLE db.table PARTITION (a = 2) RENAME TO PARTITION (a = 1)");
    round_trips(
        "ALTER TABLE db.table PARTITION (a = 2) RENAME TO PARTITION (a = 1) LOCATION 's3://...'",
    );
    round_trips(
        "ALTER TABLE db.table PARTITION (a = 2) RENAME TO PARTITION (a = 1) SET LOCATION 's3://...'",
    );
    round_trips("ALTER TABLE db.table ADD IF NOT EXISTS PARTITION (a = 'asdf', b = 2)");
    round_trips(
        "ALTER TABLE db.table ADD IF NOT EXISTS PARTITION (`a` = 'asdf', `b` = 2) PARTITION (`a` = 'asdh', `b` = 3)",
    );
    round_trips("ALTER TABLE db.table DROP PARTITION (a = 1)");
    round_trips("ALTER TABLE db.table DROP IF EXISTS PARTITION (a = 'b', c = 'd')");
    round_trips("ALTER TABLE db.table_name RENAME TO db.table_2");
}

#[test]
fn queries_round_trip() {
    round_trips("SELECT * FROM db.table WHERE a <=> b");
    round_trips("SELECT a AS 3_barrr_asdf FROM db.table_name");
    round_trips("SELECT * FROM limit");
    round_trips("SELECT * FROM t JOIN limit ON true");
    round_trips("SELECT a FROM db.table CLUSTER BY a, b");
    round_trips("SELECT a FROM db.table DISTRIBUTE BY a, b");
    round_trips("SELECT a, b FROM db.table_name JOIN a");
    round_trips("SELECT MIN(MIN(10, 5), 1L) AS a");
    round_trips("SELECT CAST(a AS DECIMAL(18,2)) FROM db.table");
    round_trips(
        "SELECT a FROM db.table LATERAL VIEW explode(a) t AS j, P LATERAL VIEW OUTER explode(a) t AS a, b WHERE a = 1",
    );
    round_trips("SELECT * FROM db.table SORT BY a");
    round_trips("SELECT * FROM db.table SORT BY a, b DESC");
    round_trips(r#"SELECT a.b["asdf"] FROM db.table WHERE a = 2"#);
    parses_to(
        "SELECT name filter FROM region",
        "SELECT name AS filter FROM region",
    );
}

#[test]
fn table_samples_round_trip() {
    round_trips("SELECT * FROM source TABLESAMPLE (BUCKET 3 OUT OF 32 ON rand()) AS s");
    round_trips("SELECT * FROM source TABLESAMPLE (BUCKET 3 OUT OF 16 ON id)");
    round_trips("SELECT * FROM source TABLESAMPLE (100M) AS s");
    round_trips("SELECT * FROM source TABLESAMPLE (0.1 PERCENT) AS s");
    round_trips("SELECT * FROM source TABLESAMPLE (10 ROWS)");
}

#[test]
fn create_function_round_trips() {
    round_trips(
        "CREATE TEMPORARY FUNCTION mydb.myfunc AS 'org.random.class.Name' USING JAR 'hdfs://somewhere.com:8020/very/far'",
    );
    rejected("CREATE TEMPORARY FUNCTION mydb.myfunc AS 'org.random.class.Name' USING JAR");
}

#[test]
fn double_quotes_delimit_names() {
    round_trips(r#"CREATE TABLE "foo" ("bar" "int")"#);
    round_trips(r#"ALTER TABLE foo ADD CONSTRAINT "bar" PRIMARY KEY (baz)"#);
}

#[test]
fn use_round_trips() {
    for name in ["mydb", "SCHEMA", "DATABASE", "CATALOG", "WAREHOUSE"] {
        round_trips(&format!("USE {name}"));
        for quote in ['\'', '"', '`'] {
            round_trips(&format!("USE {quote}{name}{quote}"));
        }
    }
    round_trips("USE DEFAULT");
}

#[test]
fn spark_only_syntax_stays_out() {
    rejected("CREATE TABLE t (a INT) USING parquet");
    rejected("SELECT * EXCEPT (a) FROM t");
    rejected("WITH c (SELECT 1) SELECT * FROM c");
}

#[test]
fn table_clauses_keep_their_order_and_appear_once() {
    round_trips("CREATE TABLE t (a INT) STORED AS ORC LOCATION 'x'");
    round_trips("CREATE TABLE t (a INT) LOCATION 'x' STORED AS ORC");
    rejected("CREATE TABLE t (a INT) TBLPROPERTIES ('a' = 'b') STORED AS ORC");
    rejected("CREATE TABLE t (a INT) STORED AS ORC STORED AS TEXTFILE");
}

#[test]
fn load_data_round_trips() {
    round_trips("LOAD DATA INPATH '/a' INTO TABLE t");
    round_trips("LOAD DATA LOCAL INPATH '/a' OVERWRITE INTO TABLE t PARTITION (a = 1)");
    round_trips("LOAD DATA INPATH '/a' INTO TABLE t INPUTFORMAT 'x' SERDE 'y'");
    rejected("LOAD DATA INTO TABLE t");
}

#[test]
fn comments_and_names_have_hive_forms() {
    assert_eq!(parse_sql::<Hive>("SELECT 1 # note").unwrap().len(), 1);
    round_trips("SELECT a$b, c{d} FROM ${e}");
    round_trips("SELECT 1L");
    round_trips("SELECT * FROM a.1b");
}

#[test]
fn purge_ends_a_drop_only() {
    round_trips("DROP TABLE t CASCADE PURGE");
    rejected("TRUNCATE TABLE t PURGE");
}

mod shape {
    use sqlparser::ast::*;
    use sqlparser::dialect::Hive;
    use sqlparser::dialect::builtin::BuiltinStatement;
    use sqlparser::dialect::hive::{HiveClusteredBy, HiveStatement, HiveTableClause};
    use sqlparser::parse_sql;

    fn statement(sql: &str) -> HiveStatement<'_> {
        match parse_sql::<Hive>(sql).unwrap().remove(0).kind {
            StatementKind::Ext(BuiltinStatement::Hive(statement)) => statement,
            other => panic!("not a Hive statement: {other:?}"),
        }
    }

    #[test]
    fn clustered_by_keeps_its_buckets() {
        let HiveStatement::CreateTable(create) =
            statement("CREATE TABLE t (a INT) CLUSTERED BY (a) INTO 4 BUCKETS")
        else {
            panic!("not CREATE TABLE");
        };
        let HiveTableClause::ClusteredBy(clustered) = &create.clauses[0] else {
            panic!("not CLUSTERED BY");
        };
        let HiveClusteredBy {
            columns, buckets, ..
        } = clustered.as_ref();
        assert_eq!(columns.len(), 1);
        assert_eq!(*buckets, "4");
    }

    #[test]
    fn with_insert_keeps_its_source_table() {
        let HiveStatement::WithInsert { with, from, .. } =
            statement("WITH c AS (SELECT 1) FROM c INSERT INTO TABLE t SELECT *")
        else {
            panic!("not an insert");
        };
        assert_eq!(with.len(), 1);
        assert_eq!(from.unwrap().to_string(), "c");
    }

    #[test]
    fn partitions_are_alter_table_operations() {
        let parsed = parse_sql::<Hive>("ALTER TABLE t DROP PARTITION (a = 1)").unwrap();
        let StatementKind::AlterTable(alter) = &parsed[0].kind else {
            panic!("not ALTER TABLE");
        };
        assert!(matches!(
            alter.operations[0],
            AlterTableOp::DropPartition {
                if_exists: false,
                ..
            }
        ));
    }
}
