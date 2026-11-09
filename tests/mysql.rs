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

//! MySQL syntax taken from the original's tests and from its dialect branches.

mod common;

use common::{cast as cast_node, col, ident, num};
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement, BuiltinTableFactor};
use sqlparser::dialect::mysql::{
    MySqlConvert, MySqlExpr, MySqlInsert, MySqlLock, MySqlStatement, MySqlTableFactor,
    MySqlTriggerBody,
};
use sqlparser::dialect::{Core, MySql, Postgres};
use sqlparser::{parse_expr, parse_sql};

fn renders(sql: &str) -> String {
    let statements = parse_sql::<MySql>(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips(sql: &str) {
    assert_eq!(renders(sql), sql);
}

fn parses_to(sql: &str, expected: &str) {
    assert_eq!(renders(sql), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<MySql>(sql).is_err(), "accepted: {sql}");
}

fn only_elsewhere(sql: &str) {
    parse_sql::<MySql>(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    assert!(
        parse_sql::<Postgres>(sql).is_err(),
        "Postgres accepted: {sql}"
    );
    assert!(parse_sql::<Core>(sql).is_err(), "Core accepted: {sql}");
}

fn statement(sql: &str) -> Statement<'_, Builtin> {
    parse_sql::<MySql>(sql)
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
        .remove(0)
}

fn expr(sql: &str) -> Expr<'_, Builtin> {
    parse_expr::<MySql>(sql).unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn mysql_expr<'a>(expr: Expr<'a, Builtin>) -> MySqlExpr<'a, Builtin> {
    let ExprKind::Ext(BuiltinExpr::MySql(node)) = expr.kind else {
        panic!("not a MySQL node: {expr:?}");
    };
    node
}

fn mysql_statement<'a>(sql: &'a str) -> MySqlStatement<'a> {
    let StatementKind::Ext(BuiltinStatement::MySql(node)) = statement(sql).kind else {
        panic!("not a MySQL statement: {sql}");
    };
    node
}

fn create_table(sql: &str) -> CreateTable<'_, Builtin> {
    let StatementKind::CreateTable(table) = statement(sql).kind else {
        panic!("not CREATE TABLE: {sql}");
    };
    *table
}

fn table_of<'a>(sql: &'a str) -> TableRef<'a, Builtin> {
    let StatementKind::Query(query) = statement(sql).kind else {
        panic!("not a query: {sql}");
    };
    let SetExpr::Select(mut select) = query.body else {
        panic!("not a SELECT: {sql}");
    };
    select.from.remove(0).relation
}

#[test]
fn cast_integer_targets() {
    parse_expr::<MySql>("CAST(foo AS UNSIGNED)").unwrap();
    parse_expr::<MySql>("CAST(foo AS SIGNED)").unwrap();
    assert_eq!(
        expr("CAST(foo AS UNSIGNED INTEGER)").to_string(),
        "CAST(foo AS UNSIGNED INTEGER)"
    );
    assert_eq!(
        expr("CAST(foo AS SIGNED INTEGER)").to_string(),
        "CAST(foo AS SIGNED INTEGER)"
    );
    let ExprKind::Cast { ty, .. } = expr("CAST(foo AS UNSIGNED INTEGER)").kind else {
        panic!("not a cast");
    };
    assert_eq!(ty.name.text, "UNSIGNED");
    assert_eq!(ty.suffix, DataTypeSuffix::Integer);
    for sql in [
        "CAST(foo AS UNSIGNED(3))",
        "CAST(foo AS UNSIGNED(3) INTEGER)",
        "CAST(foo AS UNSIGNED INTEGER(3))",
        "CAST(foo AS ARRAY)",
    ] {
        assert!(parse_expr::<MySql>(sql).is_err(), "{sql}");
    }
    assert!(parse_expr::<Core>("CAST(foo AS UNSIGNED INTEGER)").is_err());
}

#[test]
fn cast_arrays_of_element_types() {
    for ty in [
        "SIGNED",
        "UNSIGNED",
        "CHAR",
        "CHAR(10)",
        "BINARY",
        "BINARY(5)",
        "DATE",
        "TIME",
        "DATETIME",
        "DECIMAL",
        "DECIMAL(10,2)",
        "DOUBLE",
        "FLOAT",
        "YEAR",
    ] {
        let sql = format!("CAST(foo AS {ty} ARRAY)");
        assert_eq!(expr(&sql).to_string(), sql);
    }
}

#[test]
fn column_data_types_take_sign_and_zerofill_suffixes() {
    parses_to(
        "CREATE TABLE foo (a TINYINT(3) UNSIGNED, b INT(11) SIGNED, c DECIMAL(10,2) UNSIGNED, d DOUBLE PRECISION UNSIGNED)",
        "CREATE TABLE foo (a TINYINT(3) UNSIGNED, b INT(11), c DECIMAL(10,2) UNSIGNED, d DOUBLE PRECISION UNSIGNED)",
    );
    round_trips("CREATE TABLE t (a INT ZEROFILL)");
    rejected("CREATE TABLE t (a VARCHAR(10) ZEROFILL)");
    rejected("CREATE TABLE t (a VARCHAR(10) SIGNED)");
    round_trips("CREATE TABLE t (a INT(10) UNSIGNED ZEROFILL)");
    let table = create_table("CREATE TABLE t (a INT(10) UNSIGNED ZEROFILL)");
    assert_eq!(
        table.columns[0].data_type.suffix,
        DataTypeSuffix::UnsignedZerofill
    );
    assert_eq!(table.columns[0].data_type.args, ["10"]);
}

#[test]
fn enum_and_set_types_take_strings() {
    round_trips("CREATE TABLE foo (bar SET('a', 'b'), baz ENUM('a', 'b'))");
    let table = create_table("CREATE TABLE foo (bar SET('a', 'b'))");
    assert_eq!(table.columns[0].data_type.name.text, "SET");
    assert_eq!(table.columns[0].data_type.args, ["'a'", "'b'"]);
    rejected("CREATE TABLE foo (bar ENUM(1, 2))");
    rejected("CREATE TABLE foo (bar ENUM())");
}

#[test]
fn column_options() {
    round_trips("CREATE TABLE foo (s TEXT CHARACTER SET utf8mb4 COMMENT 'comment')");
    round_trips("CREATE TABLE tb (id TEXT CHARACTER SET utf8mb4 COLLATE utf8mb4_0900_ai_ci)");
    round_trips("CREATE TABLE t (a geometry SRID 4326)");
    round_trips("CREATE TABLE foo (`modification_time` DATETIME ON UPDATE CURRENT_TIMESTAMP())");
    round_trips("CREATE TABLE t1 (a INT, b INT AS (a * 2))");
    round_trips("CREATE TABLE t1 (a INT, b INT AS (a * 2) STORED)");
    round_trips("CREATE TABLE t1 (a INT, b INT AS (a * 2) VIRTUAL)");
    round_trips("CREATE TABLE foo (x INT UNIQUE KEY)");
    parses_to(
        "CREATE TABLE foo (x INT KEY)",
        "CREATE TABLE foo (x INT PRIMARY KEY)",
    );
    round_trips(
        "ALTER TABLE tab ADD COLUMN c VARCHAR(255) NOT NULL DEFAULT 'x' COMMENT 'c' AFTER b",
    );
    round_trips(
        "ALTER TABLE tab MODIFY COLUMN c INT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY FIRST",
    );

    let table = create_table("CREATE TABLE t (a geometry SRID 4326)");
    let ColumnOption::Keyword { keyword, expr } = &table.columns[0].options[0] else {
        panic!("not a keyword option");
    };
    assert_eq!(*keyword, "SRID");
    assert_eq!(expr, &Some(num("4326")));
    let table = create_table("CREATE TABLE t1 (b INT AS (2) STORED)");
    let ColumnOption::Generated(generated) = &table.columns[0].options[0] else {
        panic!("not a generated column");
    };
    assert_eq!(generated.when, Some(GeneratedWhen::Omitted));
    rejected("CREATE TABLE t (a INT COMMENT)");
    rejected("CREATE TABLE t (a INT CHARACTER)");
}

#[test]
fn index_definitions_in_create_table() {
    for sql in [
        "CREATE TABLE foo (bar INT, INDEX index (bar))",
        "CREATE TABLE tb (id INT, INDEX (id))",
        "CREATE TABLE tb (id INT, INDEX USING BTREE (id))",
        "CREATE TABLE tb (id INT, INDEX index USING HASH (id))",
        "CREATE TABLE tb (id INT, KEY USING HASH (id))",
        "CREATE TABLE tb (id INT, KEY index (id))",
        "CREATE TABLE tb (id INT, FULLTEXT (id))",
        "CREATE TABLE tb (id INT, FULLTEXT potato (id))",
        "CREATE TABLE tb (id INT, FULLTEXT INDEX potato (id))",
        "CREATE TABLE tb (id INT, SPATIAL KEY potato (id))",
        "CREATE TABLE tb (c1 INT, c2 INT, FULLTEXT KEY potato (c1, c2))",
        "CREATE TABLE foo (bar INT, UNIQUE INDEX index_name USING BTREE (bar) USING HASH)",
        "CREATE TABLE foo (bar INT, PRIMARY KEY index_name USING BTREE (bar) USING HASH)",
        "CREATE TABLE t (textcol TEXT, INDEX idx_index (textcol(10)))",
        "CREATE TABLE t (a INT, UNIQUE KEY k (a))",
        "CREATE TABLE t (a INT, CONSTRAINT c UNIQUE KEY k (a))",
        "CREATE TABLE customers (id BIGINT, custinfo JSON, INDEX zips ((CAST(custinfo -> '$.zipcode' AS UNSIGNED ARRAY))))",
        "CREATE TABLE t (jsoncol JSON, PRIMARY KEY ((CAST(col ->> '$.id' AS UNSIGNED)) ASC))",
        "CREATE TABLE t (jsoncol JSON, PRIMARY KEY ((CAST(col ->> '$.fields' AS UNSIGNED ARRAY)) ASC))",
    ] {
        round_trips(sql);
    }
    parses_to(
        "CREATE TABLE tb (id INT, index USING BTREE (id))",
        "CREATE TABLE tb (id INT, INDEX USING BTREE (id))",
    );
    parses_to(
        "CREATE TABLE tb (id INT, key index (id))",
        "CREATE TABLE tb (id INT, KEY index (id))",
    );
    parses_to(
        "CREATE TABLE tb (id INT, INDEX (c1, c2, c3,c4))",
        "CREATE TABLE tb (id INT, INDEX (c1, c2, c3, c4))",
    );

    let table = create_table(
        "CREATE TABLE foo (bar INT, UNIQUE INDEX k USING BTREE (bar DESC) USING HASH COMMENT 'c')",
    );
    let TableConstraintKind::Key {
        keyword,
        name,
        using,
        columns,
        options,
    } = &table.constraints[0].kind
    else {
        panic!("not a key: {:?}", table.constraints[0].kind);
    };
    assert_eq!(*keyword, "UNIQUE INDEX");
    assert_eq!(name.as_ref().unwrap().text, "k");
    assert_eq!(using.as_ref().unwrap().text, "BTREE");
    assert_eq!(columns[0].item.expr, col("bar"));
    assert_eq!(columns[0].item.asc, Some(false));
    assert_eq!(
        options,
        &vec![
            KeyOption::Using(ident("HASH")),
            KeyOption::Comment(StringLiteral {
                text: "c",
                quote: '\'',
                backslash_escapes: true,
                national: false,
                prefix: StringPrefix::None,
                triple: false,
            }),
        ]
    );

    // A plain list keeps the core node.
    let table = create_table("CREATE TABLE t (a INT, PRIMARY KEY (a))");
    assert!(matches!(
        table.constraints[0].kind,
        TableConstraintKind::PrimaryKey(_)
    ));
    let table = create_table("CREATE TABLE t (a INT, UNIQUE (a))");
    assert!(matches!(
        table.constraints[0].kind,
        TableConstraintKind::Unique(_)
    ));
    // A prefix length is a key part, not a plain name.
    let table = create_table("CREATE TABLE t (a TEXT, PRIMARY KEY (a(10)))");
    assert!(matches!(
        &table.constraints[0].kind,
        TableConstraintKind::Key {
            keyword: "PRIMARY KEY",
            ..
        }
    ));

    rejected("CREATE TABLE t (a INT, KEY (a) COMMENT)");
    rejected("CREATE TABLE t (a INT, INDEX ())");
    rejected("CREATE TABLE t (a INT, FULLTEXT k ())");
}

#[test]
fn key_definitions_in_alter_table() {
    round_trips("ALTER TABLE tab ADD INDEX idx_index (textcol(10))");
    round_trips("ALTER TABLE tab ADD FULLTEXT INDEX (textcol(10))");
    round_trips("ALTER TABLE tab ADD PRIMARY KEY (textcol(10))");
    round_trips("ALTER TABLE tab ADD UNIQUE KEY (textcol(10))");
    round_trips("ALTER TABLE t ADD FOREIGN KEY i (a) REFERENCES u (b)");
    round_trips(
        "ALTER TABLE customers ADD INDEX zips ((CAST(custinfo -> '$.zipcode' AS UNSIGNED ARRAY)))",
    );
}

#[test]
fn constraint_keyword_without_a_name() {
    parses_to(
        "CREATE TABLE t (x INT, CONSTRAINT CHECK (x > 1))",
        "CREATE TABLE t (x INT, CHECK (x > 1))",
    );
    parses_to(
        "CREATE TABLE t (x INT, CONSTRAINT PRIMARY KEY (x))",
        "CREATE TABLE t (x INT, PRIMARY KEY (x))",
    );
    parses_to(
        "CREATE TABLE t (x INT, CONSTRAINT UNIQUE (x))",
        "CREATE TABLE t (x INT, UNIQUE (x))",
    );
    parses_to(
        "CREATE TABLE t (x INT, CONSTRAINT FOREIGN KEY (x) REFERENCES t2 (id))",
        "CREATE TABLE t (x INT, FOREIGN KEY (x) REFERENCES t2 (id))",
    );
    let table =
        create_table("CREATE TABLE t (x INT, CONSTRAINT c FOREIGN KEY i (x) REFERENCES u (y))");
    assert_eq!(table.constraints[0].name.as_ref().unwrap().text, "c");
    let TableConstraintKind::ForeignKey { index_name, .. } = &table.constraints[0].kind else {
        panic!("not a foreign key");
    };
    assert_eq!(index_name.as_ref().unwrap().text, "i");
}

#[test]
fn table_options() {
    for sql in [
        "CREATE TABLE t (a INT) ENGINE = InnoDB",
        "CREATE TABLE t (a INT) ENGINE = `InnoDB`",
        "CREATE TABLE foo (bar INT) COMMENT 'baz'",
        "CREATE TABLE foo (bar INT) COMMENT = 'baz'",
        "CREATE TABLE foo (id INT(11)) COLLATE = utf8mb4_0900_ai_ci",
        "CREATE TABLE foo (id INT(11)) ENGINE = InnoDB DEFAULT CHARSET = utf8mb3",
        "CREATE TABLE foo (bar INT NOT NULL AUTO_INCREMENT) ENGINE = InnoDB AUTO_INCREMENT = 123",
        "CREATE TABLE foo (id INT(11)) ENGINE = InnoDB DEFAULT CHARSET = utf8mb3 COLLATE = utf8mb4_0900_ai_ci AS SELECT 1",
        "CREATE TABLE t (x INT) DEFAULT CHARSET = utf8mb4 ENGINE = InnoDB AUTO_INCREMENT = 1 DATA DIRECTORY = '/var/lib/mysql/data'",
    ] {
        round_trips(sql);
    }
    parses_to(
        "CREATE TABLE t (x INT) DEFAULT CHARSET = utf8mb4, ENGINE = InnoDB , AUTO_INCREMENT 1 DATA DIRECTORY '/var/lib/mysql/data'",
        "CREATE TABLE t (x INT) DEFAULT CHARSET = utf8mb4 ENGINE = InnoDB AUTO_INCREMENT = 1 DATA DIRECTORY = '/var/lib/mysql/data'",
    );
    parses_to(
        "CREATE TABLE mytable (id INT) ROW_FORMAT=DYNAMIC KEY_BLOCK_SIZE=8 COMMENT='abc' ENGINE=InnoDB",
        "CREATE TABLE mytable (id INT) ROW_FORMAT = DYNAMIC KEY_BLOCK_SIZE = 8 COMMENT = 'abc' ENGINE = InnoDB",
    );
    round_trips("CREATE TABLE t (a INT) UNION = (t1, t2)");
    round_trips("CREATE TABLE t (a INT) TABLESPACE ts STORAGE DISK");
    round_trips("CREATE TABLE t (a INT) INSERT_METHOD = FIRST KEY_BLOCK_SIZE = 8");

    let MySqlStatement::CreateTable(table) =
        mysql_statement("CREATE TABLE t (a INT) ENGINE = InnoDB COMMENT 'c'")
    else {
        panic!("not a MySQL table");
    };
    assert_eq!(table.table.name.parts[0].text, "t");
    assert_eq!(table.options.len(), 2);

    // Without options it stays the core statement.
    assert!(matches!(
        statement("CREATE TABLE t (a INT)").kind,
        StatementKind::CreateTable(_)
    ));
    parses_to("CREATE TABLE t SELECT 1", "CREATE TABLE t AS SELECT 1");
    parses_to(
        "CREATE TABLE t (a INT) SELECT 1",
        "CREATE TABLE t (a INT) AS SELECT 1",
    );
    rejected("CREATE TABLE t (a INT) ENGINE");
    rejected("CREATE TABLE t (a INT) ENGINE =");
    rejected("CREATE TABLE t (a INT) COMMENT 5");
    assert!(parse_sql::<Core>("CREATE TABLE t (a INT) ENGINE = InnoDB").is_err());
}

#[test]
fn create_database_options() {
    round_trips("CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4");
    round_trips("CREATE DATABASE IF NOT EXISTS mydb DEFAULT CHARACTER SET utf16");
    round_trips("CREATE DATABASE mydb DEFAULT COLLATE utf8mb4_unicode_ci");
    round_trips(
        "CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4 DEFAULT COLLATE utf8mb4_unicode_ci",
    );
    parses_to(
        "CREATE DATABASE mydb CHARACTER SET utf8mb4",
        "CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4",
    );
    parses_to(
        "CREATE DATABASE mydb CHARSET utf8mb4",
        "CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4",
    );
    parses_to(
        "CREATE DATABASE mydb DEFAULT CHARSET utf8mb4",
        "CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4",
    );
    parses_to(
        "CREATE DATABASE mydb DEFAULT CHARACTER SET = utf8mb4",
        "CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4",
    );
    parses_to(
        "CREATE DATABASE mydb COLLATE utf8mb4_unicode_ci",
        "CREATE DATABASE mydb DEFAULT COLLATE utf8mb4_unicode_ci",
    );
    parses_to(
        "CREATE DATABASE mydb COLLATE utf8mb4_unicode_ci CHARACTER SET utf8mb4",
        "CREATE DATABASE mydb DEFAULT CHARACTER SET utf8mb4 DEFAULT COLLATE utf8mb4_unicode_ci",
    );
    rejected("CREATE DATABASE mydb CHARACTER SET");
    rejected("CREATE DATABASE mydb DEFAULT");
    let MySqlStatement::CreateDatabase(db) =
        mysql_statement("CREATE DATABASE d DEFAULT CHARACTER SET utf8mb4 COLLATE c")
    else {
        panic!("not a database statement");
    };
    assert_eq!(db.charset.as_ref().unwrap().text, "utf8mb4");
    assert_eq!(db.collation.as_ref().unwrap().text, "c");
    assert!(matches!(
        statement("CREATE DATABASE d").kind,
        StatementKind::CreateDatabase(_)
    ));
}

#[test]
fn create_view_parameters() {
    for sql in [
        "CREATE ALGORITHM = MERGE VIEW foo AS SELECT 1",
        "CREATE ALGORITHM = TEMPTABLE VIEW foo AS SELECT 1",
        "CREATE ALGORITHM = UNDEFINED VIEW foo AS SELECT 1",
        "CREATE SQL SECURITY DEFINER VIEW foo AS SELECT 1",
        "CREATE SQL SECURITY INVOKER VIEW foo AS SELECT 1",
        "CREATE DEFINER = 'jeffrey'@'localhost' VIEW foo AS SELECT 1",
        "CREATE ALGORITHM = UNDEFINED DEFINER = `root`@`%` SQL SECURITY INVOKER VIEW foo AS SELECT 1",
        "CREATE OR REPLACE ALGORITHM = MERGE VIEW foo AS SELECT 1",
        "CREATE DEFINER = jeffrey@localhost VIEW foo AS SELECT 1",
        "CREATE DEFINER = CURRENT_USER VIEW foo AS SELECT 1",
    ] {
        round_trips(sql);
    }
    rejected("CREATE ALGORITHM = BAD VIEW foo AS SELECT 1");
    rejected("CREATE SQL SECURITY BAD VIEW foo AS SELECT 1");
    rejected("CREATE ALGORITHM = MERGE TABLE foo (a int)");
    rejected(
        "CREATE DEFINER = 'a'@'b' SQL SECURITY DEFINER ALGORITHM = MERGE VIEW foo AS SELECT 1",
    );

    let MySqlStatement::CreateView(view) = mysql_statement(
        "CREATE ALGORITHM = MERGE DEFINER = `r`@`%` SQL SECURITY DEFINER VIEW v AS SELECT 1",
    ) else {
        panic!("not a view");
    };
    assert_eq!(view.algorithm, Some("MERGE"));
    assert_eq!(view.security, Some("DEFINER"));
    let definer = view.definer.as_ref().unwrap();
    assert_eq!(
        definer.name.parts[0],
        Ident {
            text: "r",
            quote: Some('`'),
            span: Span::default()
        }
    );
    assert_eq!(definer.host.as_ref().unwrap().text, "%");
    assert_eq!(view.view.name.parts[0].text, "v");

    assert!(matches!(
        statement("CREATE VIEW v AS SELECT 1").kind,
        StatementKind::CreateView(_)
    ));
    assert!(parse_sql::<Postgres>("CREATE ALGORITHM = MERGE VIEW foo AS SELECT 1").is_err());
}

#[test]
fn create_trigger_bodies() {
    round_trips(
        "CREATE TRIGGER mytrigger BEFORE INSERT ON mytable FOR EACH ROW BEGIN SET NEW.a = 1; SET NEW.b = 2; END",
    );
    round_trips(
        "CREATE TRIGGER tr AFTER INSERT ON t1 FOR EACH ROW BEGIN INSERT INTO t2 VALUES (NEW.id); END",
    );
    round_trips("CREATE TRIGGER t BEFORE UPDATE ON x FOR EACH ROW FOLLOWS y SET NEW.a = 1");
    round_trips("CREATE TRIGGER IF NOT EXISTS t AFTER DELETE ON x FOR EACH ROW DELETE FROM y");
    round_trips("CREATE DEFINER = 'a'@'b' TRIGGER t BEFORE INSERT ON x FOR EACH ROW SET NEW.a = 1");
    round_trips("CREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW BEGIN END");
    let MySqlStatement::CreateTrigger(trigger) = mysql_statement(
        "CREATE TRIGGER tr AFTER INSERT ON t1 FOR EACH ROW BEGIN INSERT INTO t2 VALUES (NEW.id); END",
    ) else {
        panic!("not a trigger");
    };
    assert_eq!(trigger.name.parts[0].text, "tr");
    assert_eq!(trigger.period, "AFTER");
    assert_eq!(trigger.event, "INSERT");
    let MySqlTriggerBody::Block(statements) = &trigger.body else {
        panic!("not a block");
    };
    assert_eq!(statements.len(), 1);
    assert!(matches!(statements[0].kind, StatementKind::Insert(_)));

    rejected("CREATE TRIGGER t BEFORE INSERT OR UPDATE ON x FOR EACH ROW SET NEW.a = 1");
    rejected("CREATE TRIGGER t INSTEAD OF INSERT ON x FOR EACH ROW SET NEW.a = 1");
    rejected("CREATE TRIGGER t BEFORE INSERT ON x FOR EACH STATEMENT SET NEW.a = 1");
    rejected("CREATE TRIGGER t BEFORE INSERT ON x SET NEW.a = 1");
    rejected("CREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW BEGIN SET NEW.a = 1 END");
    // Not real MySQL: a body is a statement, not a call to a named function.
    rejected(
        "CREATE TRIGGER emp_stamp BEFORE INSERT ON emp FOR EACH ROW EXECUTE FUNCTION emp_stamp()",
    );
    assert!(
        parse_sql::<Core>("CREATE TRIGGER t BEFORE INSERT ON x FOR EACH ROW SET NEW.a = 1")
            .is_err()
    );
}

#[test]
fn drop_temporary_table() {
    round_trips("DROP TEMPORARY TABLE foo");
    round_trips("DROP TEMPORARY TABLE IF EXISTS foo");
    let StatementKind::Drop(drop) = statement("DROP TEMPORARY TABLE foo").kind else {
        panic!("not a drop");
    };
    assert!(drop.temporary);
    assert!(parse_sql::<Core>("DROP TEMPORARY TABLE foo").is_err());
}

#[test]
fn flush_statements() {
    for sql in [
        "FLUSH BINARY LOGS",
        "FLUSH ENGINE LOGS",
        "FLUSH ERROR LOGS",
        "FLUSH HOSTS",
        "FLUSH LOCAL SLOW LOGS",
        "FLUSH NO_WRITE_TO_BINLOG GENERAL LOGS",
        "FLUSH OPTIMIZER_COSTS",
        "FLUSH PRIVILEGES",
        "FLUSH RELAY LOGS FOR CHANNEL test",
        "FLUSH RELAY LOGS",
        "FLUSH STATUS",
        "FLUSH USER_RESOURCES",
        "FLUSH LOGS",
        "FLUSH TABLES",
        "FLUSH TABLES WITH READ LOCK",
        "FLUSH TABLES `mek`.`table1`, table2",
        "FLUSH TABLES `mek`.`table1`, table2 FOR EXPORT",
        "FLUSH TABLES `mek`.`table1`, table2 WITH READ LOCK",
        "FLUSH TABLES handler WITH READ LOCK",
    ] {
        round_trips(sql);
    }
    let MySqlStatement::Flush(flush) = mysql_statement("FLUSH TABLES a, b WITH READ LOCK") else {
        panic!("not a flush");
    };
    assert_eq!(flush.object, "TABLES");
    assert_eq!(flush.tables.len(), 2);
    assert!(flush.read_lock);
    assert!(!flush.export);
    rejected("FLUSH SOMETHING");
    rejected("FLUSH RELAY");
    assert!(parse_sql::<Core>("FLUSH TABLES").is_err());
}

#[test]
fn lock_and_kill_statements() {
    parses_to(
        "LOCK TABLES trans t READ, customer WRITE",
        "LOCK TABLES trans AS t READ, customer WRITE",
    );
    round_trips("LOCK TABLES trans AS t READ, customer WRITE");
    round_trips("LOCK TABLES trans AS t READ LOCAL, customer WRITE");
    round_trips("LOCK TABLES trans AS t READ, customer LOW_PRIORITY WRITE");
    round_trips("UNLOCK TABLES");
    round_trips("KILL 5");
    round_trips("KILL CONNECTION 5");
    round_trips("KILL QUERY 5");

    let MySqlStatement::LockTables(tables) = mysql_statement("LOCK TABLES a AS b READ LOCAL")
    else {
        panic!("not a lock");
    };
    assert_eq!(tables[0].lock, MySqlLock::Read { local: true });
    assert_eq!(tables[0].alias.as_ref().unwrap().text, "b");
    let MySqlStatement::LockTables(tables) = mysql_statement("LOCK TABLES a LOW_PRIORITY WRITE")
    else {
        panic!("not a lock");
    };
    assert_eq!(tables[0].lock, MySqlLock::Write { low_priority: true });
    assert!(tables[0].alias.is_none());

    rejected("LOCK TABLES a");
    rejected("LOCK TABLES a, b WRITE");
    rejected("KILL a");
    rejected("KILL MUTATION 1");
    assert!(parse_sql::<Postgres>("KILL 5").is_err());
    assert!(parse_sql::<Postgres>("UNLOCK TABLES").is_err());
}

#[test]
fn insert_forms() {
    round_trips("INSERT LOW_PRIORITY INTO tasks (title, priority) VALUES ('Test Some Inserts', 1)");
    round_trips(
        "INSERT HIGH_PRIORITY INTO tasks (title, priority) VALUES ('Test Some Inserts', 1)",
    );
    round_trips("INSERT DELAYED INTO t VALUES (1)");
    round_trips("INSERT LOW_PRIORITY IGNORE INTO t VALUES (1)");
    round_trips("INSERT INTO t SET a = 1, b = 2");
    round_trips("INSERT INTO t VALUES (), ()");
    parses_to(
        "INSERT INTO tb () VALUES (), ()",
        "INSERT INTO tb VALUES (), ()",
    );
    parses_to("INSERT INTO t VALUE (1)", "INSERT INTO t VALUES (1)");
    round_trips("INSERT INTO t VALUES ROW(1), ROW(2)");
    round_trips("INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = VALUES(a), b = new.a");
    round_trips("INSERT INTO t (a) SELECT 1 FROM u ON DUPLICATE KEY UPDATE a = 1");
    round_trips("INSERT INTO `table` (`date`) VALUES ('2024-01-01') AS `alias`");
    round_trips(
        "INSERT INTO `table` (`id`, `date`) VALUES (1, '2024-01-01') AS `alias` (`mek_id`, `mek_date`)",
    );
    round_trips("INSERT INTO t VALUES (1) AS n (x) ON DUPLICATE KEY UPDATE a = x");

    let MySqlStatement::Insert(boxed) =
        mysql_statement("INSERT INTO t VALUES (1) AS n (x) ON DUPLICATE KEY UPDATE a = x")
    else {
        panic!("not an insert");
    };
    let MySqlInsert {
        replace,
        insert,
        alias,
        on_duplicate,
    } = *boxed;
    assert!(!replace);
    assert_eq!(insert.table.parts[0].text, "t");
    let (name, columns) = alias.unwrap();
    assert_eq!(name.text, "n");
    assert_eq!(columns[0].text, "x");
    assert_eq!(on_duplicate[0].value, col("x"));

    // Plain inserts stay core statements, with their modifiers.
    let StatementKind::Insert(insert) = statement("INSERT LOW_PRIORITY INTO t SET a = 1").kind
    else {
        panic!("not an insert");
    };
    assert_eq!(insert.modifiers.priority, Some("LOW_PRIORITY"));
    let InsertSource::Set(assignments) = &insert.source else {
        panic!("not SET");
    };
    assert_eq!(assignments[0].value, num("1"));

    rejected("INSERT LOW_PRIORITY DELAYED INTO t VALUES (1)");
    rejected("INSERT INTO t VALUES (1) ON DUPLICATE UPDATE a = 1");
    assert!(
        parse_sql::<Postgres>("INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = 1").is_err()
    );
    assert!(parse_sql::<Core>("INSERT INTO t SET a = 1").is_err());
}

#[test]
fn replace_statements() {
    round_trips("REPLACE DELAYED INTO tasks (title, priority) VALUES ('Test Some Inserts', 1)");
    round_trips("REPLACE LOW_PRIORITY INTO t VALUES (1)");
    round_trips("REPLACE INTO t SET a = 1");
    round_trips("REPLACE INTO t SELECT 1");
    let MySqlStatement::Insert(insert) = mysql_statement("REPLACE INTO t VALUES (1)") else {
        panic!("not a replace");
    };
    assert!(insert.replace);
    assert!(parse_sql::<Core>("REPLACE INTO t VALUES (1)").is_err());
}

#[test]
fn values_row_queries() {
    round_trips("VALUES ROW(1, true, 'a')");
    round_trips(
        "SELECT a, c FROM (VALUES ROW(1, true, 'a'), ROW(2, false, 'b'), ROW(3, false, 'c')) AS t (a, b, c)",
    );
    assert!(matches!(
        statement("VALUES ROW(1)").kind,
        StatementKind::Query(query) if matches!(query.body, SetExpr::RowValues(_))
    ));
    assert!(parse_sql::<Core>("VALUES ROW(1)").is_err());
}

#[test]
fn update_and_delete_forms() {
    round_trips("UPDATE foo SET bar = false WHERE foo = true ORDER BY foo ASC");
    round_trips("UPDATE foo SET bar = false WHERE foo = true ORDER BY foo ASC LIMIT 10");
    round_trips(
        "UPDATE orders AS o JOIN customers AS c ON o.customer_id = c.id SET o.completed = true WHERE c.firstname = 'Peter'",
    );
    round_trips("UPDATE employees PARTITION (p0) SET salary = 1");
    round_trips("DELETE FROM customers ORDER BY id DESC");
    round_trips("DELETE FROM customers LIMIT 100");
    round_trips("DELETE FROM customers ORDER BY id LIMIT 100");
    round_trips("DELETE FROM employees PARTITION (p0) WHERE id = 1");

    let StatementKind::Update(update) =
        statement("UPDATE a JOIN b ON x SET y = 1 ORDER BY z LIMIT 1").kind
    else {
        panic!("not an update");
    };
    assert_eq!(update.joins.len(), 1);
    assert_eq!(update.order_by.len(), 1);
    assert_eq!(update.limit.unwrap().count, Some(num("1")));

    assert!(parse_sql::<Core>("UPDATE foo SET bar = 1 ORDER BY a").is_err());
    assert!(parse_sql::<Core>("DELETE FROM foo LIMIT 1").is_err());
}

#[test]
fn table_partitions_and_hints() {
    round_trips("SELECT * FROM employees PARTITION (p0, p2)");
    round_trips("SELECT * FROM employees PARTITION (p0) AS e");
    round_trips(
        "SELECT * FROM employees PARTITION (p0) JOIN departments PARTITION (p1) ON employees.dept_id = departments.id",
    );
    round_trips("SELECT * FROM t USE INDEX (i)");
    round_trips("SELECT * FROM t FORCE INDEX FOR JOIN (i, j)");
    round_trips("SELECT * FROM t USE KEY FOR ORDER BY (i)");
    round_trips("SELECT * FROM t IGNORE INDEX FOR GROUP BY (i) USE INDEX ()");
    round_trips("SELECT * FROM t AS a USE INDEX (i) WHERE x");
    round_trips("SELECT * FROM t PARTITION (p0) USE INDEX (i)");
    round_trips("UPDATE t PARTITION (p0) USE INDEX (i) SET a = 1");
    round_trips("DELETE FROM t PARTITION (p0) ORDER BY a LIMIT 1");

    let TableFactor::Ext(BuiltinTableFactor::MySql(factor)) =
        table_of("SELECT * FROM t PARTITION (p0, p2)").factor
    else {
        panic!("not a MySQL table factor");
    };
    let MySqlTableFactor::Partitioned { name, partitions } = *factor else {
        panic!("not partitioned");
    };
    assert_eq!(name.parts[0].text, "t");
    assert_eq!(
        partitions.iter().map(|p| p.text).collect::<Vec<_>>(),
        ["p0", "p2"]
    );

    rejected("SELECT * FROM t USE INDEX");
    rejected("SELECT * FROM t USE INDEX FOR (i)");
    assert!(parse_sql::<Postgres>("SELECT * FROM t PARTITION (p0)").is_err());
}

#[test]
fn straight_join_and_cross_join_on() {
    round_trips("SELECT a.*, b.* FROM table_a AS a STRAIGHT_JOIN table_b AS b ON a.b_id = b.id");
    round_trips("SELECT a.*, b.* FROM table_a STRAIGHT_JOIN table_b AS b ON a.b_id = b.id");
    round_trips("SELECT * FROM a STRAIGHT_JOIN b");
    round_trips("SELECT * FROM a STRAIGHT_JOIN b USING(x)");
    round_trips("SELECT * FROM a CROSS JOIN b ON a.x = b.x");
    let StatementKind::Query(query) = statement("SELECT * FROM a STRAIGHT_JOIN b").kind else {
        panic!("not a query");
    };
    let SetExpr::Select(select) = &query.body else {
        panic!("not a select");
    };
    assert_eq!(
        select.from[0].joins[0].kind,
        JoinKind::Custom("STRAIGHT_JOIN")
    );
    rejected("SELECT * FROM a NATURAL STRAIGHT_JOIN b");
}

#[test]
fn json_table_factor() {
    let sql = r#"SELECT * FROM JSON_TABLE('[1]', '$[*]' COLUMNS(a INT PATH '$.a', b TEXT EXISTS PATH "$.b", NESTED PATH '$.c' COLUMNS (d INT PATH '$'), e FOR ORDINALITY, f INT PATH '$' DEFAULT '0' ON EMPTY ERROR ON ERROR)) AS t"#;
    round_trips(sql);
    round_trips("SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(a INT PATH '$'))");
    round_trips(
        "SELECT jt.* FROM JSON_TABLE('[]', '$[*]' COLUMNS(row_num FOR ORDINALITY, name VARCHAR(50) PATH '$')) AS jt",
    );
    parses_to(
        "SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(NESTED 'x' COLUMNS(a INT PATH 'y'))) AS t",
        "SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(NESTED PATH 'x' COLUMNS (a INT PATH 'y'))) AS t",
    );
    parses_to(
        "SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(a INT PATH 'x' NULL ON ERROR NULL ON EMPTY)) AS t",
        "SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(a INT PATH 'x' NULL ON EMPTY NULL ON ERROR)) AS t",
    );
    round_trips("SELECT * FROM json_table");
    round_trips("SELECT * FROM JSON_TABLE(?, '$[*]' COLUMNS(a VARCHAR(20) PATH '$')) AS t");

    let table = table_of(
        "SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(a INT PATH '$', b INT EXISTS PATH 'x')) AS t",
    );
    assert_eq!(table.alias.as_ref().unwrap().name.text, "t");
    let TableFactor::Ext(BuiltinTableFactor::MySql(factor)) = table.factor else {
        panic!("not a MySQL table factor");
    };
    let MySqlTableFactor::JsonTable(json) = *factor else {
        panic!("not a JSON table");
    };
    assert_eq!(json.path.text, "$");
    assert_eq!(json.columns.len(), 2);

    rejected("SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(a INT)) AS t");
    rejected("SELECT * FROM JSON_TABLE('[]', COLUMNS(a INT PATH '$')) AS t");
    rejected("SELECT * FROM JSON_TABLE('[]', '$' COLUMNS()) AS t");
    assert!(
        parse_sql::<Postgres>("SELECT * FROM JSON_TABLE('[]', '$' COLUMNS(a INT PATH '$')) AS t")
            .is_err()
    );
}

#[test]
fn grants_to_users_at_hosts() {
    round_trips("GRANT ALL ON *.* TO 'jeffrey'@'%'");
    round_trips("GRANT SELECT ON t TO 'a'@'b', 'c'@'d'");
    round_trips("GRANT ALL ON *.* TO jeffrey@localhost");
    round_trips("REVOKE ALL ON db1.* FROM 'jeffrey'@'%'");
    round_trips("GRANT ALL ON *.* TO 'jeffrey'");
    let StatementKind::Grant(grant) = statement("GRANT ALL ON *.* TO 'jeffrey'@'%'").kind else {
        panic!("not a grant");
    };
    let grantee = &grant.grantees[0];
    assert_eq!(grantee.name.parts[0].text, "jeffrey");
    assert_eq!(grantee.name.parts[0].quote, Some('\''));
    assert_eq!(grantee.host.as_ref().unwrap().text, "%");
    assert_eq!(grant.objects.unwrap().names[0].name.parts[1].text, "*");

    let StatementKind::Grant(grant) = statement("GRANT ALL ON t TO u").kind else {
        panic!("not a grant");
    };
    assert!(grant.grantees[0].host.is_none());

    rejected("GRANT ALL ON t TO");
    rejected("GRANT ALL ON t TO 'a'@");
    assert!(parse_sql::<Postgres>("GRANT ALL ON *.* TO 'jeffrey'@'%'").is_err());
}

#[test]
fn convert_forms() {
    round_trips("SELECT CONVERT('x' USING latin1)");
    round_trips("SELECT CONVERT(my_column USING utf8mb4) FROM my_table");
    round_trips("SELECT CONVERT('abc', CHAR(60))");
    round_trips("SELECT CONVERT(123.456, DECIMAL(5,2))");
    round_trips("SELECT CONVERT('test', CHAR CHARACTER SET utf8mb4)");
    let MySqlExpr::Convert(convert) = mysql_expr(expr("CONVERT(a, CHAR(5) CHARACTER SET b)"))
    else {
        panic!("not a convert");
    };
    let MySqlConvert {
        expr: value,
        using,
        data_type,
        charset,
    } = *convert;
    assert_eq!(value, col("a"));
    assert_eq!(using, None);
    assert_eq!(data_type.as_ref().unwrap().args, ["5"]);
    assert_eq!(charset.unwrap().parts[0].text, "b");
    let MySqlExpr::Convert(convert) = mysql_expr(expr("CONVERT(a USING b)")) else {
        panic!("not a convert");
    };
    assert_eq!(convert.using.unwrap().parts[0].text, "b");
    // Anything else stays a plain function call, as in the original.
    assert!(matches!(expr("CONVERT(a)").kind, ExprKind::Function { .. }));
    assert!(matches!(
        expr("CONVERT(INT, 1, 2)").kind,
        ExprKind::Function { .. }
    ));
    assert!(parse_sql::<Postgres>("SELECT CONVERT('x' USING latin1)").is_err());
}

#[test]
fn expressions() {
    round_trips("SELECT 17 MEMBER OF('[23, \"abc\", 17, \"ab\", 10]')");
    parses_to("SELECT 1 MEMBER OF ('[1]')", "SELECT 1 MEMBER OF('[1]')");
    let MySqlExpr::MemberOf { value, array } = mysql_expr(expr("a MEMBER OF('[]')")) else {
        panic!("not MEMBER OF");
    };
    assert_eq!(*value, col("a"));
    assert_eq!(array.to_string(), "'[]'");
    round_trips("SELECT a AS member");
    parses_to("SELECT a MEMBER", "SELECT a AS MEMBER");

    parses_to("SELECT a && b", "SELECT a AND b");
    assert_eq!(expr("a && b"), expr("a AND b"));
    round_trips("SELECT @a := @b := 1");
    round_trips("SELECT @price := price, @tax := price * 0.1 FROM products WHERE id = 1");
    round_trips("UPDATE products SET price = @new_price := price * 1.1 WHERE category = 'Books'");
    let ExprKind::Binary { left, op, right } = expr("@a := @b := 1").kind else {
        panic!("not a binary expression");
    };
    assert_eq!(*left, col("@a"));
    assert_eq!(op, BinaryOp::Named(":="));
    assert!(matches!(
        right.kind,
        ExprKind::Binary {
            op: BinaryOp::Named(":="),
            ..
        }
    ));
    only_elsewhere("SELECT a := 1");
}

#[test]
fn group_concat_separator() {
    let sql = "GROUP_CONCAT(DISTINCT test_score ORDER BY test_score DESC SEPARATOR ' ')";
    assert_eq!(expr(sql).to_string(), sql);
    let ExprKind::Function { options, .. } = expr("GROUP_CONCAT(a SEPARATOR ',')").kind else {
        panic!("not a function");
    };
    assert_eq!(options.unwrap().separator.unwrap().text, ",");
    assert_eq!(expr("F(a SEPARATOR ',')").to_string(), "F(a SEPARATOR ',')");
    for sql in [
        "GROUP_CONCAT(a SEPARATOR b)",
        "GROUP_CONCAT(a SEPARATOR 'x', b)",
        "GROUP_CONCAT(a SEPARATOR ',' ORDER BY a)",
    ] {
        assert!(parse_expr::<MySql>(sql).is_err(), "{sql}");
    }
    assert!(parse_expr::<Core>("GROUP_CONCAT(a SEPARATOR ' ')").is_err());
}

#[test]
fn introducers_and_literals() {
    round_trips("SELECT _binary 'abc'");
    parses_to("SELECT _utf8'abc'", "SELECT _utf8 'abc'");
    parses_to("SELECT _utf8mb4\"abc\"", "SELECT _utf8mb4 \"abc\"");
    round_trips("SELECT _latin1 X'4D7953514C'");
    round_trips("SELECT _foo");
    let MySqlExpr::Introduced(intro) = mysql_expr(expr("_binary 'abc'")) else {
        panic!("not an introducer");
    };
    assert_eq!(intro.charset.text, "_binary");
    let Literal::String(value) = intro.value else {
        panic!("not a string");
    };
    assert_eq!(value.text, "abc");

    round_trips("SELECT N'text'");
    parses_to("SELECT n'text'", "SELECT N'text'");
    let ExprKind::Literal(Literal::String(text)) = expr("N'text'").kind else {
        panic!("not a string");
    };
    assert!(text.national);
    round_trips("SELECT B'111'");
    round_trips("SELECT B'it''s'");
    parses_to("SELECT b'101'", "SELECT B'101'");
    let ExprKind::Literal(Literal::String(bits)) = expr("B'111'").kind else {
        panic!("not a string");
    };
    assert_eq!(bits.prefix, StringPrefix::Byte);
    assert_eq!(bits.text, "111");
    // A double-quoted `B"text"` stays a column with an alias.
    round_trips("SELECT B AS \"x\"");
    assert_eq!(expr("0b101").to_string(), "0b101");
}

#[test]
fn adjacent_string_literals() {
    parses_to(r#"SELECT 'M' "y" 'S' "q" 'l'"#, "SELECT 'MySql'");
    parses_to("SELECT 'a' 'b' FROM t", "SELECT 'ab' FROM t");
    parses_to(r#"SELECT "a" 'b'"#, r#"SELECT "ab""#);
    parses_to(r"SELECT 'a\'b' 'c'", r"SELECT 'a''bc'");
    parses_to("SET @a = 'x' 'y'", "SET @a = 'xy'");
    let MySqlExpr::ConcatenatedStrings(parts) = mysql_expr(expr("'a' \"b\"")) else {
        panic!("not concatenated");
    };
    assert_eq!(parts.iter().map(|p| p.text).collect::<Vec<_>>(), ["a", "b"]);
    assert!(parse_expr::<Core>("'a' 'b'").is_err());
}

#[test]
fn select_item_string_aliases() {
    parses_to("SELECT 1 'x'", "SELECT 1 AS 'x'");
    parses_to("SELECT 1 AS 'x'", "SELECT 1 AS 'x'");
    parses_to("SELECT 1 \"x\"", "SELECT 1 AS \"x\"");
    let StatementKind::Query(query) = statement("SELECT 1 'x'").kind else {
        panic!("not a query");
    };
    let SetExpr::Select(select) = &query.body else {
        panic!("not a select");
    };
    let SelectItemKind::Expr { alias, .. } = &select.projection[0].kind else {
        panic!("not an expression");
    };
    assert_eq!(alias.as_ref().unwrap().quote, Some('\''));
}

#[test]
fn binary_operator_casts() {
    assert_eq!(expr("BINARY 'a'").to_string(), "CAST('a' AS BINARY)");
    assert_eq!(expr("BINARY a").to_string(), "CAST(a AS BINARY)");
    assert_eq!(renders("SELECT BINARY a, b"), "SELECT CAST(a AS BINARY), b");
    assert_eq!(
        expr("BINARY a"),
        cast_node(col("a"), "BINARY", CastStyle::Function)
    );
    assert_eq!(expr("BINARY(5)").to_string(), "BINARY(5)");
    round_trips("SELECT binary FROM t");
}

#[test]
fn numeric_prefixed_identifiers() {
    round_trips("SELECT 123abc");
    round_trips("SELECT 1db.1table.1column");
    round_trips("SELECT t.15to29 FROM my_table AS t");
    round_trips("SELECT t.15e29 FROM my_table AS t");
    assert_eq!(expr("0b101").to_string(), "0b101");
    assert_eq!(expr("123e4").to_string(), "123e4");
    assert_eq!(expr("1").to_string(), "1");
    assert_eq!(expr("1.5").to_string(), "1.5");
    // Dollars can begin and continue a name.
    round_trips("SELECT $a$, a$b FROM t");
    let ExprKind::Ident(name) = expr("$a").kind else {
        panic!("not a name");
    };
    assert_eq!(name.text, "$a");
    // Numbers keep their meaning elsewhere.
    assert!(parse_expr::<Core>("123abc").is_err());
}
