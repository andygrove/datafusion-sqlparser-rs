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

//! Redshift's table attributes, `COPY`, `VACUUM` and PartiQL `FROM` forms.

use sqlparser::ast::{Statement, StatementKind};
use sqlparser::dialect::builtin::{Builtin, BuiltinStatement};
use sqlparser::dialect::redshift::RedshiftStatement;
use sqlparser::dialect::{Core, Postgres, Redshift};
use sqlparser::parse_sql;

fn roundtrip(sql: &str) -> Statement<'_, Builtin> {
    let mut statements = parse_sql::<Redshift>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    let statement = statements.remove(0);
    assert_eq!(statement.to_string(), sql);
    statement
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Redshift>(sql).is_err(), "accepted: {sql}");
}

#[test]
fn table_attributes() {
    for sql in [
        "CREATE TABLE public.users (id INT, name VARCHAR(255)) BACKUP YES",
        "CREATE TABLE staging.events (event_id INT) BACKUP NO",
        "CREATE TABLE t1 (c1 INT) DISTSTYLE ALL",
        "CREATE TABLE t1 (c1 INT) DISTSTYLE AUTO",
        "CREATE TABLE t1 (c1 INT) DISTSTYLE EVEN",
        "CREATE TABLE t1 (c1 INT) DISTSTYLE KEY DISTKEY(c1)",
        "CREATE TABLE t1 (c1 INT, c2 INT) DISTSTYLE KEY DISTKEY(c1) SORTKEY(c1, c2)",
        "CREATE TABLE t1 (c1 INT, c2 INT) SORTKEY(c1, c2)",
        "CREATE TABLE t1 DISTKEY(1) SORTKEY(1, 3) AS SELECT eventid, venueid, dateid, eventname FROM event",
        "CREATE TEMPORARY TABLE tmp_sbk_summary_pp DISTSTYLE KEY DISTKEY(bet_id) AS SELECT 1 AS bet_id",
        "CREATE TABLE public.users_backup_test BACKUP YES DISTSTYLE AUTO AS SELECT id, name, email FROM public.users",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE TABLE t (a INT) DISTSTYLE");
    rejected("CREATE TABLE t (a INT) BACKUP");
    rejected("CREATE TABLE t (a INT) SORTKEY");

    let StatementKind::Ext(BuiltinStatement::Redshift(RedshiftStatement::CreateTable(create))) =
        roundtrip("CREATE TABLE t1 (c1 INT) BACKUP NO DISTSTYLE EVEN").kind
    else {
        panic!("expected a Redshift CREATE TABLE");
    };
    assert_eq!(create.backup, Some(false));
    assert_eq!(create.diststyle, Some("EVEN"));

    assert!(parse_sql::<Postgres>("CREATE TABLE t (a INT) DISTSTYLE EVEN").is_err());
    assert!(parse_sql::<Core>("CREATE TABLE t (a INT) BACKUP YES").is_err());
}

#[test]
fn views_and_copy() {
    roundtrip("CREATE VIEW myevent AS SELECT eventname FROM event WITH NO SCHEMA BINDING");
    roundtrip(
        "COPY t1 FROM 's3://bucket/file.csv' CREDENTIALS 'aws_access_key_id=AK;aws_secret_access_key=SK' CSV",
    );
    rejected("COPY t1 FROM");
    assert!(parse_sql::<Core>("CREATE VIEW v AS SELECT 1 WITH NO SCHEMA BINDING").is_err());
    assert!(parse_sql::<Postgres>("CREATE VIEW v AS SELECT 1 WITH NO SCHEMA BINDING").is_err());
    assert!(parse_sql::<Postgres>("COPY t FROM 'x' CREDENTIALS 'k' GZIP").is_err());
}

#[test]
fn vacuum() {
    roundtrip("VACUUM tbl");
    roundtrip("VACUUM FULL");
    roundtrip(
        "VACUUM FULL SORT ONLY DELETE ONLY REINDEX RECLUSTER db1.sc1.tbl1 TO 20 PERCENT BOOST",
    );
    assert!(parse_sql::<Postgres>("VACUUM tbl").is_err());
}

#[test]
fn partiql_from() {
    roundtrip("SELECT * FROM src[0].a AS a");
    roundtrip("SELECT * FROM src[0].a[1].b AS a");
    roundtrip("SELECT * FROM lineitem AS l (a, b, c) AT idx");
    roundtrip(
        "SELECT index, val FROM (SELECT array('AAA', 'BBB') AS val) AS b, b.val AS val AT index",
    );
    roundtrip("SELECT t.id, k, v FROM test_colors AS t, UNPIVOT t AS v AT k");
    roundtrip("SELECT t.id, k, v FROM test_colors AS t, UNPIVOT t.count_by_color AS v AT k");
    roundtrip("SELECT a[0].b FROM t");
}

#[test]
fn names_and_literals() {
    roundtrip("SELECT #_of_values");
    roundtrip("SELECT a # b FROM t");
    roundtrip("SELECT [col1] FROM [test_schema].[test_table]");
    roundtrip("SELECT 1 AS [\" 1 \"]");
    roundtrip("SELECT 1 AS [\"a[b]\"] FROM a");
    roundtrip("SELECT E'a\\n'");
    roundtrip("SELECT EXTRACT('month' FROM my_timestamp) FROM my_table");
    roundtrip("SELECT FIRST_VALUE(1 IGNORE NULLS) OVER () FROM (SELECT 1) AS t");
    roundtrip("SELECT APPROXIMATE PERCENTILE_DISC(0.5) WITHIN GROUP (ORDER BY totalprice)");
    assert!(parse_sql::<Core>("SELECT [col1] FROM t").is_err());
}
