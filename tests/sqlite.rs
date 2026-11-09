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

//! SQLite: the statements and syntax only this dialect has, and what stays out.

use sqlparser::dialect::Sqlite;
use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::{parse_expr, parse_sql};

fn round_trips(sql: &str) {
    let statements = parse_sql::<Sqlite>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<Sqlite>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), expected);
}

fn expr_round_trips(sql: &str) {
    let expr = parse_expr::<Sqlite>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(expr.to_string(), sql);
}

fn expr_parses_to(sql: &str, expected: &str) {
    let expr = parse_expr::<Sqlite>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(expr.to_string(), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Sqlite>(sql).is_err(), "accepted: {sql}");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("SQLite").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
}

#[test]
fn pragma_round_trips() {
    for sql in [
        "PRAGMA cache_size",
        "PRAGMA cache_size = 10",
        "PRAGMA cache_size(10)",
        "PRAGMA table_info = 'sqlite_master'",
        r#"PRAGMA table_info("sqlite_master")"#,
        "PRAGMA table_info = ?",
        "PRAGMA main.cache_size = 10",
        "PRAGMA 'a'",
        "PRAGMA foreign_keys = ON",
        "PRAGMA cache_size = -2000",
    ] {
        round_trips(sql);
    }
    rejected("PRAGMA");
    rejected("PRAGMA a = ");
    rejected("PRAGMA a()");
    rejected("PRAGMA a(1, 2)");
    rejected("PRAGMA a = 1 2");
}

#[test]
fn attach_database_round_trips() {
    round_trips("ATTACH DATABASE 'test.db' AS test");
    round_trips("ATTACH 'test.db' AS test");
    rejected("ATTACH 'a'");
    rejected("ATTACH DATABASE 'a' b");
}

#[test]
fn create_table_options_round_trip() {
    round_trips("CREATE TABLE t (a INT) WITHOUT ROWID");
    round_trips("CREATE TABLE Fruits (id TEXT NOT NULL PRIMARY KEY) STRICT");
    for suffix in [
        " WITHOUT ROWID, STRICT",
        " STRICT, WITHOUT ROWID",
        " STRICT, STRICT",
        ", STRICT",
        ", WITHOUT ROWID, STRICT",
    ] {
        round_trips(&format!("CREATE TABLE t (a INT){suffix}"));
    }
    parses_to(
        "CREATE TABLE t (a INT) , STRICT",
        "CREATE TABLE t (a INT), STRICT",
    );
    for suffix in [
        " WITHOUT ROWID STRICT",
        " STRICT WITHOUT ROWID",
        " STRICT,",
        " STRICT, WITHOUT",
        ",",
        ", , STRICT",
    ] {
        rejected(&format!("CREATE TABLE t (a INT){suffix}"));
    }
}

#[test]
fn virtual_tables_round_trip() {
    round_trips("CREATE VIRTUAL TABLE IF NOT EXISTS t USING module_name (arg1, arg2)");
    round_trips("CREATE VIRTUAL TABLE t USING module_name");
    round_trips("CREATE VIRTUAL TABLE a.t USING m (b, c)");
    round_trips("CREATE VIRTUAL TABLE 't' USING 'm' ('b')");
    rejected("CREATE VIRTUAL TABLE t USING m ()");
}

#[test]
fn temporary_views_round_trip() {
    round_trips("CREATE TEMPORARY VIEW IF NOT EXISTS myschema.myview AS SELECT foo FROM bar");
}

#[test]
fn double_equals_reads_as_equals() {
    parses_to("SELECT a==b FROM t", "SELECT a = b FROM t");
    parses_to("UPDATE t SET a == 1", "UPDATE t SET a = 1");
    parses_to("UPDATE t SET a == 1, b == 2", "UPDATE t SET a = 1, b = 2");
    round_trips("UPDATE t SET a = 1");
}

#[test]
fn column_options_round_trip() {
    round_trips("CREATE TABLE foo (bar INT PRIMARY KEY AUTOINCREMENT)");
    round_trips("CREATE TABLE foo (bar INT PRIMARY KEY ASC)");
    round_trips("CREATE TABLE foo (bar INT PRIMARY KEY DESC)");
    round_trips("CREATE TABLE t (a INT PRIMARY KEY DESC AUTOINCREMENT)");
    round_trips("CREATE TABLE t (a INT CONSTRAINT c NOT NULL)");
    round_trips("CREATE TABLE t (a CONSTRAINT c PRIMARY KEY)");
    round_trips("CREATE TABLE t (a INT, b TEXT DEFAULT -1 NOT NULL)");
    rejected("CREATE TABLE t ('a' CONSTRAINT c)");
    for keyword in ["ROLLBACK", "ABORT", "FAIL", "IGNORE", "REPLACE"] {
        round_trips(&format!(
            "CREATE TABLE t1 (a INT, b INT ON CONFLICT {keyword})"
        ));
    }
    round_trips("CREATE TABLE t (a INT NOT NULL ON CONFLICT FAIL)");
    rejected("CREATE TABLE t1 (a INT, b INT ON CONFLICT BOH)");
    rejected("CREATE TABLE t (a INT UNIQUE DEFERRABLE INITIALLY DEFERRED NOT DEFERRABLE)");
}

#[test]
fn quoted_names_round_trip() {
    round_trips("CREATE TABLE `PRIMARY` (\"KEY\" INT, [INDEX] INT)");
    round_trips("SELECT `a``b`, \"c\"\"d\", [e] FROM [t]");
    round_trips("SELECT [a].[b] AS [c] FROM t AS [u]");
}

#[test]
fn generated_columns_round_trip() {
    for sql in [
        "CREATE TABLE t1 (a INT, b INT GENERATED ALWAYS AS (a * 2))",
        "CREATE TABLE t1 (a INT, b INT GENERATED ALWAYS AS (a * 2) VIRTUAL)",
        "CREATE TABLE t1 (a INT, b INT GENERATED ALWAYS AS (a * 2) STORED)",
        "CREATE TABLE t1 (a INT, b INT AS (a * 2))",
        "CREATE TABLE t1 (a INT, b INT AS (a * 2) VIRTUAL)",
        "CREATE TABLE t1 (a INT, b INT AS (a * 2) STORED)",
    ] {
        round_trips(sql);
    }
}

#[test]
fn columns_can_be_untyped() {
    round_trips("CREATE TABLE t1 (a, b AS (a * 2), c NOT NULL)");
    round_trips("CREATE TABLE t (a, b TEXT)");
    round_trips("CREATE TABLE t (a PRIMARY KEY)");
    round_trips("ALTER TABLE t ADD COLUMN a");
    round_trips("CREATE TABLE t (a) AS SELECT 1");
}

#[test]
fn string_literals_can_name_things() {
    round_trips("CREATE TABLE t ('a')");
    round_trips(r#"CREATE TABLE '""' ('id' INT UNSIGNED NOT NULL)"#);
    round_trips(
        r#"CREATE TABLE '""' ('id' INT UNSIGNED NOT NULL, 'name' TEXT NOT NULL, 'zip' INT UNSIGNED NULL)"#,
    );
    round_trips("SELECT 't'.*, t.'x' FROM 't'");
    round_trips("SELECT 'a'.'b'.'c'.*");
    round_trips("SELECT x.y.'z'");
    round_trips("SELECT 1 + 'a'.b");
    round_trips("SELECT 'a'.b('c')");
    round_trips("CREATE TABLE a.'b' (x)");
    round_trips("SELECT * FROM 'a'.'b'");
    round_trips("SELECT * FROM t JOIN 'u' ON 1");
    round_trips("INSERT INTO 't' VALUES (1)");
    round_trips("UPDATE 't' SET a = 1");
    round_trips("DELETE FROM 't'");
}

#[test]
fn string_literals_can_be_aliases() {
    parses_to("SELECT t.n'' FROM t", "SELECT t.n AS '' FROM t");
    parses_to("SELECT n'' FROM t", "SELECT n AS '' FROM t");
    parses_to("SELECT N'hello'", "SELECT N AS 'hello'");
    parses_to("SELECT 'a' 'b'", "SELECT 'a' AS 'b'");
    parses_to("SELECT a 'b'", "SELECT a AS 'b'");
    round_trips("SELECT a AS 'b'");
    round_trips("SELECT * FROM t AS 'x'");
    round_trips("SELECT * FROM t 'x'");
    rejected("SELECT 1 2");
    rejected("SELECT a b c");
    rejected("SELECT 'a' AS");
    rejected("SELECT 1 AS 2");
}

#[test]
fn placeholders_round_trip() {
    round_trips("SELECT @xxx");
    round_trips("SELECT :xxx");
    expr_round_trips("id = $id");
    expr_round_trips("id = $$");
    round_trips("SELECT $a$b, :a$b, @a$b, $$a$$, $");
    round_trips("SELECT :1, @1, ?1, ?");
    rejected("SELECT c:se");
    rejected("SELECT a:b FROM t");
    rejected("SELECT :");
    rejected("SELECT @");
}

#[test]
fn identifiers_take_dollars_and_non_ascii() {
    round_trips("SELECT a$ FROM t");
    round_trips("SELECT 1 AS a$");
    parses_to("CREATE TABLE t(a$ INT)", "CREATE TABLE t (a$ INT)");
    round_trips("INSERT INTO t$ VALUES (1)");
    parses_to("SELECT a AS$", "SELECT a AS AS$");
    round_trips("SELECT \u{DFFFE}");
    round_trips("SELECT \u{12000}");
    round_trips("SELECT \u{12000}\u{12001}");
    rejected("SELECT \u{7f}");
}

#[test]
fn numbers_take_underscores() {
    round_trips("SELECT 1_000");
    round_trips("SELECT 1_000.5");
    round_trips("SELECT 1e1_0");
    round_trips("SELECT 1.e1");
    round_trips("SELECT .5e1");
    round_trips("SELECT 1_000_000");
    rejected("SELECT 1__0");
    rejected("SELECT 1_");
    rejected("SELECT 1_a");
    rejected("SELECT 1e_1");
}

#[test]
fn comments_do_not_nest_and_can_be_left_open() {
    parses_to("SELECT 1 /* x", "SELECT 1");
    parses_to("SELECT 1 /* x;", "SELECT 1");
    assert_eq!(parse_sql::<Sqlite>("/* ;").unwrap().len(), 0);
    rejected("SELECT 1 /* a /* b */ c */");
}

#[test]
fn keyword_aliases_round_trip() {
    for keyword in [
        "analyze",
        "cluster",
        "distribute",
        "end",
        "exclude",
        "explain",
        "fetch",
        "lateral",
        "minus",
        "offset",
        "sort",
        "top",
        "view",
        "with",
    ] {
        parses_to(
            &format!("SELECT a {keyword} FROM t"),
            &format!("SELECT a AS {keyword} FROM t"),
        );
        parses_to(
            &format!("SELECT 1 {keyword}"),
            &format!("SELECT 1 AS {keyword}"),
        );
    }
    rejected("SELECT 1 WHERE");
    rejected("SELECT 1 GROUP");
    round_trips("SELECT with FROM t");
    round_trips("SELECT offset");
    parses_to("SELECT 1 JOIN", "SELECT 1 AS JOIN");
    parses_to("SELECT 1 FOR", "SELECT 1 AS FOR");
}

#[test]
fn over_can_be_an_alias() {
    parses_to("SELECT f() OVER", "SELECT f() AS OVER");
    parses_to("SELECT f() OVER FROM t", "SELECT f() AS OVER FROM t");
    round_trips("SELECT f() OVER partition");
    round_trips("SELECT f() OVER ()");
    round_trips("SELECT f() OVER 'partition'");
    round_trips(r#"SELECT f() OVER "partition""#);
    parses_to(
        "SELECT f() FILTER (WHERE x) OVER",
        "SELECT f() FILTER (WHERE x) AS OVER",
    );
}

#[test]
fn window_functions_can_filter() {
    for name in [
        "row_number",
        "rank",
        "max",
        "count",
        "user_defined_function",
    ] {
        round_trips(&format!("SELECT {name}(x) FILTER (WHERE y) OVER () FROM t"));
    }
}

#[test]
fn functions_round_trip() {
    round_trips("SELECT SUBSTRING('SQLITE', 3, 4)");
    round_trips("SELECT SUBSTR('SQLITE', 3, 4)");
    round_trips("SELECT SUBSTRING('SQLITE', 3)");
    round_trips("SELECT SUBSTR('SQLITE', 3)");
    round_trips("SELECT f(ORDER BY x)");
    round_trips("SELECT left(a, 1)");
    round_trips("SELECT right(a, 1)");
}

#[test]
fn cast_type_can_be_empty() {
    round_trips("SELECT CAST(a AS)");
    parses_to("SELECT CAST(a AS  ) FROM t", "SELECT CAST(a AS) FROM t");
    round_trips("SELECT CAST(a AS INT)");
}

#[test]
fn update_forms_round_trip() {
    round_trips("UPDATE x SET (a, b) = (1, 2)");
    round_trips("UPDATE t SET (a, b) = (SELECT 1, 2) WHERE c");
    round_trips("UPDATE t SET a = 1, (b, c) = (1, 2)");
    round_trips("UPDATE foo SET bar = 1 LIMIT 99");
    round_trips("UPDATE OR REPLACE t SET a = 1");
    round_trips("UPDATE t SET a = 1 FROM u");
    round_trips("UPDATE t SET a = 1 RETURNING *");
    round_trips("UPDATE t SET a = 1 RETURNING a LIMIT 1");
    parses_to(
        "UPDATE t SET a = 1 FROM u, v ON u.a = v.a",
        "UPDATE t SET a = 1 FROM u JOIN v ON u.a = v.a",
    );
    rejected("UPDATE t SET a = 1 ORDER BY b LIMIT 1");
    rejected("UPDATE t SET a = 1 LIMIT 1 OFFSET 2");
    rejected("UPDATE t SET a = 1 LIMIT 1 RETURNING a");
}

#[test]
fn delete_forms_round_trip() {
    round_trips("DELETE FROM foo LIMIT 99");
    round_trips("DELETE FROM t ORDER BY b LIMIT 1");
    round_trips("DELETE FROM t ORDER BY b");
    round_trips("DELETE FROM t RETURNING a");
    round_trips("DELETE FROM t RETURNING a LIMIT 1");
    rejected("DELETE FROM t LIMIT 1 RETURNING a");
    rejected("DELETE FROM t LIMIT 1 OFFSET 2");
}

#[test]
fn insert_forms_round_trip() {
    round_trips("INSERT OR REPLACE INTO t VALUES (1)");
    round_trips("INSERT OR IGNORE INTO t (a) VALUES (1), (2)");
    round_trips("INSERT OR ROLLBACK INTO t VALUES (1)");
    round_trips("INSERT OR ABORT INTO t DEFAULT VALUES RETURNING *");
    round_trips("INSERT OR FAIL INTO t VALUES (1)");
    parses_to(
        "REPLACE INTO t VALUES (1)",
        "INSERT OR REPLACE INTO t VALUES (1)",
    );
    parses_to(
        "REPLACE INTO t SELECT 1",
        "INSERT OR REPLACE INTO t SELECT 1",
    );
    round_trips("INSERT INTO t DEFAULT VALUES");
    rejected("INSERT INTO t (a) DEFAULT VALUES");
    round_trips("INSERT INTO t VALUES (1) RETURNING a, b AS c");
    rejected("INSERT OR BOGUS INTO t VALUES (1)");
    rejected("INSERT INTO t AS x VALUES (1)");
}

#[test]
fn upsert_round_trips() {
    round_trips("INSERT INTO t VALUES (1, 2, 3) ON CONFLICT(a) WHERE b > 0 DO NOTHING");
    round_trips("INSERT INTO t VALUES (1) ON CONFLICT DO NOTHING");
    parses_to(
        "INSERT INTO t VALUES (1) ON CONFLICT (a) DO UPDATE SET b = excluded.b WHERE c",
        "INSERT INTO t VALUES (1) ON CONFLICT(a) DO UPDATE SET b = excluded.b WHERE c",
    );
    parses_to(
        "INSERT INTO t VALUES (1) ON CONFLICT (a, b) WHERE c DO UPDATE SET (d, e) = (1, 2)",
        "INSERT INTO t VALUES (1) ON CONFLICT(a, b) WHERE c DO UPDATE SET (d, e) = (1, 2)",
    );
    round_trips("INSERT INTO t VALUES (1) ON CONFLICT DO NOTHING RETURNING *");
    rejected("INSERT INTO t VALUES (1) ON CONFLICT (a + 1) DO NOTHING");
    rejected("INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = 1");
    rejected("INSERT INTO t VALUES (1) ON CONFLICT ON CONSTRAINT c DO NOTHING");
}

#[test]
fn triggers_round_trip() {
    for sql in [
        "CREATE TRIGGER trg_inherit_asset_models AFTER INSERT ON assets FOR EACH ROW BEGIN INSERT INTO users (name) SELECT pam.name FROM users AS pam; END",
        "CREATE TRIGGER log_new_user AFTER INSERT ON users BEGIN INSERT INTO user_log (user_id, action, timestamp) VALUES (NEW.id, 'created', datetime('now')); END",
        "CREATE TRIGGER cleanup_orders AFTER DELETE ON customers BEGIN DELETE FROM orders WHERE customer_id = OLD.id; DELETE FROM invoices WHERE customer_id = OLD.id; END",
        "CREATE TRIGGER trg_before_update BEFORE UPDATE ON products FOR EACH ROW WHEN NEW.price < 0 BEGIN SELECT RAISE(ABORT, 'Price cannot be negative'); END",
        "CREATE TRIGGER trg_instead_of_insert INSTEAD OF INSERT ON my_view BEGIN INSERT INTO my_table (col1, col2) VALUES (NEW.col1, NEW.col2); END",
        "CREATE TEMPORARY TRIGGER temp_trigger AFTER INSERT ON temp_table BEGIN UPDATE log_table SET count = count + 1; END",
        "CREATE TRIGGER trg_inherit_asset_models INSERT ON assets FOR EACH ROW BEGIN INSERT INTO users (name) SELECT pam.name FROM users AS pam; END",
        "CREATE TRIGGER a.t UPDATE OF b, c ON x BEGIN SELECT 1; END",
        "CREATE TRIGGER t INSERT ON x WHEN a BEGIN SELECT 1; END",
    ] {
        round_trips(sql);
    }
    parses_to(
        "CREATE TEMP TRIGGER t INSERT ON x BEGIN SELECT 1; END",
        "CREATE TEMPORARY TRIGGER t INSERT ON x BEGIN SELECT 1; END",
    );
    round_trips("CREATE TRIGGER IF NOT EXISTS t INSERT ON x BEGIN SELECT 1; END");
    assert_eq!(
        parse_sql::<Sqlite>(
            "CREATE TRIGGER t AFTER INSERT ON x BEGIN SELECT 1; SELECT 2; END; SELECT 3"
        )
        .unwrap()
        .len(),
        2
    );
    round_trips("DROP TRIGGER IF EXISTS trg_inherit_asset_models");
    round_trips("DROP TRIGGER a.t");
    rejected("CREATE TRIGGER t INSERT ON x BEGIN SELECT 1 END");
    rejected("CREATE TRIGGER t INSERT ON x BEGIN END");
}

#[test]
fn begin_modifiers_round_trip() {
    for sql in [
        "BEGIN DEFERRED TRANSACTION",
        "BEGIN IMMEDIATE TRANSACTION",
        "BEGIN EXCLUSIVE TRANSACTION",
        "BEGIN DEFERRED",
        "BEGIN IMMEDIATE",
        "BEGIN EXCLUSIVE",
        "BEGIN",
        "BEGIN TRANSACTION",
    ] {
        round_trips(sql);
    }
    rejected("BEGIN DEFERRED TRANSACTION x");
}

#[test]
fn in_lists_can_be_empty() {
    round_trips("SELECT * FROM t1 WHERE a IN ()");
    round_trips("SELECT a NOT IN ()");
    round_trips("SELECT 1 IN () IN ()");
    rejected("SELECT 1 IN (,)");
}

#[test]
fn in_can_name_a_table() {
    for sql in [
        "SELECT 1 IN view",
        "SELECT 1 IN offset",
        "SELECT 1 IN key",
        "SELECT 1 IN abort",
        r#"SELECT 1 IN "from""#,
        "SELECT a IN b.c",
        "SELECT a IN f(1)",
        "SELECT a NOT IN b",
    ] {
        round_trips(sql);
    }
    for sql in [
        "SELECT 1 IN FROM t",
        "SELECT 1 IN select",
        "SELECT 1 IN where",
        "SELECT 1 IN NULL",
        "SELECT 1 IN table",
        "SELECT a FROM t WHERE a IN AND b",
    ] {
        rejected(sql);
    }
}

#[test]
fn pattern_operators_round_trip() {
    expr_round_trips("col MATCH 'pattern'");
    expr_round_trips("col REGEXP 'pattern'");
    expr_round_trips("col GLOB 'pattern'");
    round_trips("SELECT * FROM email WHERE email MATCH 'fts5'");
    round_trips(r#"SELECT count(*) FROM messages WHERE msg_text REGEXP '\d+'"#);
    round_trips("SELECT count(*) FROM files WHERE name GLOB '*.txt'");
    round_trips("SELECT a NOT REGEXP b");
    round_trips("SELECT a NOT GLOB b");
    round_trips("SELECT a NOT MATCH b");
    for sql in ["SELECT 1 REGEXP", "SELECT 1 MATCH", "SELECT 1 GLOB"] {
        rejected(sql);
    }
}

#[test]
fn is_takes_any_operand() {
    for sql in [
        "a IS b",
        "a IS NOT b",
        "a IS b + c",
        "a IS b AND c IS NOT d",
        "a IS b = c",
        "a IS b IN (1)",
        "a IS NOT json('{}')",
        "a IS NOT json",
        "a IS unknown",
        "a IS NOT normalized",
        "a IS NOT nfc",
        "a IS NULL",
        "a IS NOT NULL",
        "a IS TRUE",
        "a IS NOT FALSE",
        "a IS DISTINCT FROM b",
        "a IS NOT DISTINCT FROM b",
    ] {
        expr_round_trips(sql);
    }
    round_trips("SELECT a IS json('{}'), a IS NOT json('{}') FROM t");
    rejected("SELECT a IS");
    rejected("SELECT a IS NOT");
    rejected("SELECT a FROM t WHERE a IS NFC NORMALIZED");
    rejected("SELECT a FROM t WHERE a IS NOT JSON OBJECT");
}

#[test]
fn postfix_null_tests_round_trip() {
    expr_parses_to("a NOTNULL", "a IS NOT NULL");
    expr_parses_to("a NOT NULL", "a IS NOT NULL");
    expr_parses_to("a ISNULL", "a IS NULL");
    expr_parses_to("a BETWEEN b NOTNULL AND c", "a BETWEEN b IS NOT NULL AND c");
}

#[test]
fn between_bounds_take_comparisons() {
    expr_round_trips("a BETWEEN b > 1 AND c");
    expr_round_trips("a BETWEEN b LIKE 'x' AND c");
    expr_round_trips("a BETWEEN b IS NOT NULL AND c");
    expr_round_trips("a BETWEEN b BETWEEN 1 AND 2 AND c");
    expr_round_trips("5 BETWEEN 1 AND NULL IS NULL");
    expr_round_trips("5 BETWEEN 1 AND 10 = 1");
    rejected("SELECT a BETWEEN (1 AND 2 FROM t");
}

#[test]
fn operators_round_trip() {
    for sql in [
        "a << 1",
        "a >> 1",
        "a -> b",
        "a ->> b",
        "a & b | c",
        "a::int",
    ] {
        expr_round_trips(sql);
    }
    parses_to("SELECT ~a", "SELECT ~ a");
    round_trips("SELECT ~ a");
    round_trips("SELECT 1::CHAR(-1)");
    round_trips("SELECT a << 1 FROM t");
    round_trips("SELECT a >> 1 FROM t");
    rejected("SELECT a::");
    rejected("SELECT a <=> b");
}

#[test]
fn numeric_type_modifiers_round_trip() {
    for sql in [
        "CREATE TABLE t (a CHAR(+10))",
        "CREATE TABLE t (a CHAR(-1))",
        "CREATE TABLE t (a VARCHAR(2.5))",
        "CREATE TABLE t (a VARCHAR(.5))",
        "CREATE TABLE t (a INT(-1))",
        "CREATE TABLE t (a NUMERIC(10.5))",
        "CREATE TABLE t (a FLOAT(2.5))",
        "CREATE TABLE t (a NVARCHAR(+5))",
        "CREATE TABLE t (a TIMESTAMP(-1))",
        "SELECT CAST(x AS REAL(1.5))",
        "CREATE TABLE t (a CHAR(1, 1.5))",
        "CREATE TABLE t (a INT(1, -2))",
        "CREATE TABLE t (a DECIMAL(1, 2.5))",
        "CREATE TABLE t (a REAL(-1.5, +2.5))",
        "CREATE TABLE t (a DECIMAL(10, 99999999999999999999999))",
        "CREATE TABLE t (a VARCHAR(1, .5))",
        "CREATE TABLE t (a CHAR(0.0))",
        "CREATE TABLE t (a CHAR(-0))",
        "CREATE TABLE t (a CHAR(1.))",
        "CREATE TABLE t (a CHAR(1e3))",
        "CREATE TABLE t (a CHAR(+1.5e-3))",
        "CREATE TABLE t (a CHAR(+1, +2))",
        "CREATE TABLE t (a CHAR(-1) NOT NULL DEFAULT NULL)",
        "CREATE TABLE t (a CHAR(1.5), b CHAR(10))",
        "CREATE TABLE t (a char(-1))",
        r#"CREATE TABLE t (a "CHAR"(-1))"#,
        "CREATE TABLE t (a [CHAR](-1))",
        "SELECT abs(-1), round(1, 1.5), char(+1, -2)",
        "CREATE TABLE t (a CHAR(10))",
        "CREATE TABLE t (a INT(10))",
        "CREATE TABLE t (a INT UNSIGNED)",
        "CREATE TABLE t (a INT(10) UNSIGNED)",
    ] {
        round_trips(sql);
    }
    parses_to(
        "CREATE TABLE t (a CHAR ( - 1 ))",
        "CREATE TABLE t (a CHAR(-1))",
    );
    parses_to(
        "CREATE TABLE t (a CHAR(-1,-2))",
        "CREATE TABLE t (a CHAR(-1, -2))",
    );
    parses_to(
        "SELECT CAST(a AS DECIMAL(99999999999999999999999,2))",
        "SELECT CAST(a AS DECIMAL(99999999999999999999999, 2))",
    );
    for sql in [
        "CREATE TABLE t (a CHAR(1.5, 2, 3))",
        "CREATE TABLE t (a CHAR(1.5,))",
        "CREATE TABLE t (a CHAR(+-1))",
        "CREATE TABLE t (a CHAR(NULL))",
    ] {
        rejected(sql);
    }
}

#[test]
fn limit_takes_a_comma() {
    round_trips("SELECT 1 LIMIT 1, 2");
    round_trips("SELECT 1 LIMIT 1 OFFSET 2");
    rejected("SELECT * FROM t ORDER BY 1 LIMIT 1, 2 OFFSET 3");
}

#[test]
fn comma_can_join_with_a_constraint() {
    parses_to(
        "SELECT * FROM t, u ON t.a = u.a",
        "SELECT * FROM t JOIN u ON t.a = u.a",
    );
    parses_to(
        "SELECT * FROM t, u USING (a)",
        "SELECT * FROM t JOIN u USING(a)",
    );
    parses_to(
        "SELECT * FROM t, u ON t.a = u.a, v ON v.a = u.a",
        "SELECT * FROM t JOIN u ON t.a = u.a JOIN v ON v.a = u.a",
    );
    parses_to(
        "SELECT * FROM t, u ON t.a = u.a JOIN v ON v.a = u.a",
        "SELECT * FROM t JOIN u ON t.a = u.a JOIN v ON v.a = u.a",
    );
    parses_to(
        "SELECT * FROM t, u USING (a), v",
        "SELECT * FROM t JOIN u USING(a), v",
    );
    round_trips("SELECT * FROM t, u");
    rejected("SELECT * FROM t, u ON");
    rejected("SELECT * FROM t, u USING a");
    rejected("SELECT * FROM t, u ON t.a = u.a CROSS");
}

#[test]
fn parentheses_can_wrap_a_table() {
    for (sql, canonical) in [
        ("SELECT * FROM (t)", "SELECT * FROM t"),
        ("SELECT * FROM ((t))", "SELECT * FROM t"),
        ("SELECT * FROM (t) AS a", "SELECT * FROM t AS a"),
        ("SELECT * FROM (t AS a)", "SELECT * FROM t AS a"),
        (
            "SELECT * FROM ((SELECT 1) AS s)",
            "SELECT * FROM (SELECT 1) AS s",
        ),
        ("SELECT * FROM f(), (t)", "SELECT * FROM f(), t"),
        ("SELECT * FROM (t), (u)", "SELECT * FROM t, u"),
        (
            "SELECT * FROM (t INDEXED BY i) AS b",
            "SELECT * FROM t AS b INDEXED BY i",
        ),
    ] {
        parses_to(sql, canonical);
    }
    round_trips("SELECT * FROM (t JOIN u)");
    round_trips("SELECT * FROM (((a NATURAL JOIN b)))");
    rejected("SELECT * FROM (t AS a) AS b");
}

#[test]
fn indexed_by_round_trips() {
    for sql in [
        "SELECT * FROM t INDEXED BY i WHERE a = 1",
        "SELECT * FROM t NOT INDEXED",
        "SELECT * FROM t AS x NOT INDEXED",
        "SELECT * FROM t INDEXED BY i, t AS u NOT INDEXED",
        "SELECT * FROM t AS x INDEXED BY i WHERE a = 1",
        "DELETE FROM t INDEXED BY i WHERE a = 1",
        "UPDATE t INDEXED BY i SET a = 2 WHERE a = 1",
        "SELECT * FROM t INDEXED BY idx_b",
        "SELECT * FROM f() INDEXED BY i",
        "SELECT * FROM t a NOT INDEXED",
    ] {
        round_trips(sql);
    }
    rejected("SELECT * FROM t INDEXED BY i AS a");
    rejected("SELECT * FROM t NOT INDEXED AS a");
    rejected("SELECT * FROM (SELECT 1) INDEXED BY i");
    rejected("SELECT * FROM (t) NOT INDEXED");
    rejected("SELECT * FROM t INDEXED");
}

#[test]
fn qualified_wildcards_stay_in_the_select_list() {
    rejected("SELECT * FROM t WHERE a.*");
    round_trips("SELECT [t].*, `t`.* FROM t");
}

#[test]
fn precedence_shows_through_parentheses() {
    use sqlparser::ast::{BinaryOp, ExprKind};
    let op = |sql: &'static str| match parse_expr::<Sqlite>(sql).unwrap().kind {
        ExprKind::Binary { op, .. } => op,
        other => panic!("not binary: {other:?}"),
    };
    // Comparison binds tighter than equality, so the root is `=`.
    assert_eq!(op("a = b < c"), BinaryOp::Eq);
    assert_eq!(op("a < b = c"), BinaryOp::Eq);
    // The bitwise operators share a level, so the last one is the root.
    assert_eq!(op("6 | 1 & 3"), BinaryOp::BitAnd);
    assert_eq!(op("6 & 3 | 8"), BinaryOp::BitOr);
    assert_eq!(op("1 << 2 & 4"), BinaryOp::BitAnd);
    assert_eq!(op("8 >> 1 | 2"), BinaryOp::BitOr);
    // Shifts bind looser than addition and tighter than equality.
    assert_eq!(op("a << b + c"), BinaryOp::ShiftLeft);
    assert_eq!(op("a + b << c"), BinaryOp::ShiftLeft);
    assert_eq!(op("a << b = c"), BinaryOp::Eq);
    // `||` binds tighter than `*`, unlike in the original.
    assert_eq!(op("a * b || c"), BinaryOp::Mul);
    // The pattern operators stay above AND and below string concatenation.
    assert_eq!(op("a REGEXP 'p' AND b = 1"), BinaryOp::And);
    assert_eq!(op("a GLOB 'p' || 'q'"), BinaryOp::Named("GLOB"));
}

mod shape {
    use sqlparser::ast::{
        BinaryOp, ColumnOption, DataTypeSuffix, ExprKind, IsTest, JoinConstraint, JoinKind,
        StatementKind, TableFactor,
    };
    use sqlparser::dialect::Sqlite;
    use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement, BuiltinTableFactor};
    use sqlparser::dialect::sqlite::{
        OnConflictAction, PragmaForm, SqliteAssignmentTarget, SqliteExpr, SqliteStatement,
        SqliteTableOption, TriggerEvent, TriggerPeriod,
    };
    use sqlparser::parse_sql;

    fn statement(sql: &str) -> SqliteStatement<'_> {
        match parse_sql::<Sqlite>(sql).unwrap().remove(0).kind {
            StatementKind::Ext(BuiltinStatement::Sqlite(statement)) => statement,
            other => panic!("not a SQLite statement: {other:?}"),
        }
    }

    fn select(sql: &str) -> sqlparser::ast::Select<'_, Builtin> {
        match parse_sql::<Sqlite>(sql).unwrap().remove(0).kind {
            StatementKind::Query(query) => match query.body {
                sqlparser::ast::SetExpr::Select(select) => *select,
                other => panic!("not a select: {other:?}"),
            },
            other => panic!("not a query: {other:?}"),
        }
    }

    #[test]
    fn pragma_records_its_form() {
        let SqliteStatement::Pragma { name, value } = statement("PRAGMA cache_size = 10") else {
            panic!("not PRAGMA");
        };
        assert_eq!(name.to_string(), "cache_size");
        let (form, value) = value.unwrap();
        assert_eq!(form, PragmaForm::Equals);
        assert_eq!(value.to_string(), "10");
        let SqliteStatement::Pragma { value, .. } = statement("PRAGMA a(1)") else {
            panic!("not PRAGMA");
        };
        assert_eq!(value.unwrap().0, PragmaForm::Call);
    }

    #[test]
    fn table_options_stay_in_order() {
        let SqliteStatement::CreateTable(create) =
            statement("CREATE TABLE t (a INT), STRICT, WITHOUT ROWID")
        else {
            panic!("not CREATE TABLE");
        };
        assert!(create.leading_comma);
        assert_eq!(
            create.options,
            [SqliteTableOption::Strict, SqliteTableOption::WithoutRowid]
        );
    }

    #[test]
    fn plain_create_table_uses_the_core_node() {
        let parsed = parse_sql::<Sqlite>("CREATE TABLE t (a, b TEXT)").unwrap();
        let StatementKind::CreateTable(create) = &parsed[0].kind else {
            panic!("not CREATE TABLE");
        };
        assert!(create.columns[0].data_type.is_unspecified());
        assert!(!create.columns[1].data_type.is_unspecified());
    }

    #[test]
    fn column_options_keep_their_keywords() {
        let parsed = parse_sql::<Sqlite>(
            "CREATE TABLE t (a INT PRIMARY KEY AUTOINCREMENT ON CONFLICT FAIL)",
        )
        .unwrap();
        let StatementKind::CreateTable(create) = &parsed[0].kind else {
            panic!("not CREATE TABLE");
        };
        let options = &create.columns[0].options;
        assert_eq!(options[0], ColumnOption::PrimaryKey);
        assert_eq!(
            options[1],
            ColumnOption::Keyword {
                keyword: "AUTOINCREMENT",
                expr: None
            }
        );
        assert_eq!(
            options[2],
            ColumnOption::Keyword {
                keyword: "ON CONFLICT FAIL",
                expr: None
            }
        );
    }

    #[test]
    fn signed_type_arguments_keep_their_signs() {
        let parsed = parse_sql::<Sqlite>("CREATE TABLE t (a CHAR(-1, 2.5))").unwrap();
        let StatementKind::CreateTable(create) = &parsed[0].kind else {
            panic!("not CREATE TABLE");
        };
        let ty = &create.columns[0].data_type;
        assert_eq!(ty.args, ["1", "2.5"]);
        assert_eq!(ty.suffix, DataTypeSuffix::SignedArgs([b'-', 0]));
    }

    #[test]
    fn triggers_record_their_parts() {
        let SqliteStatement::CreateTrigger(trigger) = statement(
            "CREATE TEMPORARY TRIGGER t BEFORE UPDATE OF a, b ON x FOR EACH ROW WHEN NEW.a < 0 BEGIN SELECT 1; DELETE FROM y; END",
        ) else {
            panic!("not CREATE TRIGGER");
        };
        assert!(trigger.temporary);
        assert_eq!(trigger.period, Some(TriggerPeriod::Before));
        let TriggerEvent::Update(columns) = &trigger.event else {
            panic!("not UPDATE");
        };
        assert_eq!(columns.len(), 2);
        assert!(trigger.for_each_row);
        assert!(trigger.when.is_some());
        assert_eq!(trigger.statements.len(), 2);
    }

    #[test]
    fn update_tuple_targets_keep_their_columns() {
        let SqliteStatement::Update(update) = statement("UPDATE x SET (a, b) = (1, 2)") else {
            panic!("not UPDATE");
        };
        let SqliteAssignmentTarget::Tuple(columns) = &update.assignments[0].target else {
            panic!("not a tuple");
        };
        assert_eq!(columns.len(), 2);
    }

    #[test]
    fn plain_update_uses_the_core_node() {
        let parsed = parse_sql::<Sqlite>("UPDATE t SET a = 1 WHERE b").unwrap();
        assert!(matches!(parsed[0].kind, StatementKind::Update(_)));
    }

    #[test]
    fn upsert_records_target_and_action() {
        let SqliteStatement::Insert(insert) =
            statement("INSERT INTO t VALUES (1) ON CONFLICT(a) WHERE b > 0 DO NOTHING")
        else {
            panic!("not INSERT");
        };
        let conflict = insert.on_conflict.unwrap();
        assert_eq!(conflict.columns.len(), 1);
        assert_eq!(conflict.predicate.unwrap().to_string(), "b > 0");
        assert_eq!(conflict.action, OnConflictAction::DoNothing);
    }

    #[test]
    fn replace_reads_as_or_replace() {
        let SqliteStatement::Insert(insert) = statement("REPLACE INTO t VALUES (1)") else {
            panic!("not INSERT");
        };
        assert_eq!(insert.or_action, Some("REPLACE"));
    }

    #[test]
    fn indexed_tables_carry_the_alias() {
        let select = select("SELECT * FROM (t INDEXED BY i) AS b");
        let TableFactor::Ext(BuiltinTableFactor::Sqlite(indexed)) = &select.from[0].relation.factor
        else {
            panic!("not an indexed table");
        };
        assert_eq!(indexed.index.unwrap().text, "i");
        assert_eq!(indexed.table.alias.as_ref().unwrap().name.text, "b");
    }

    #[test]
    fn comma_with_a_constraint_is_a_join() {
        let select = select("SELECT * FROM t, u ON t.a = u.a");
        let join = &select.from[0].joins[0];
        assert_eq!(join.kind, JoinKind::Inner);
        assert!(matches!(join.constraint, JoinConstraint::On(_)));
    }

    #[test]
    fn in_can_hold_a_table() {
        let select = select("SELECT 1 IN t");
        let sqlparser::ast::SelectItemKind::Expr { expr, .. } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        let ExprKind::Ext(BuiltinExpr::Sqlite(SqliteExpr::InTable {
            target, negated, ..
        })) = &expr.kind
        else {
            panic!("not IN table");
        };
        assert_eq!(target.to_string(), "t");
        assert!(!negated);
    }

    #[test]
    fn notnull_is_an_is_test() {
        let select = select("SELECT a NOTNULL");
        let sqlparser::ast::SelectItemKind::Expr { expr, .. } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        let ExprKind::Is { test, negated, .. } = &expr.kind else {
            panic!("not IS");
        };
        assert_eq!(*test, IsTest::Null);
        assert!(*negated);
    }

    #[test]
    fn is_with_an_operand_is_binary() {
        let select = select("SELECT a IS NOT b");
        let sqlparser::ast::SelectItemKind::Expr { expr, .. } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        let ExprKind::Binary { op, .. } = &expr.kind else {
            panic!("not binary");
        };
        assert_eq!(*op, BinaryOp::Named("IS NOT"));
    }
}
