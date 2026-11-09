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

//! `DROP`, `TRUNCATE`, `ALTER TABLE` and the `CREATE` family.

mod common;

use common::*;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift};
use sqlparser::parse_sql;

type St<'a> = Statement<'a, Builtin>;

fn one(sql: &str) -> St<'_> {
    let mut statements = parse_sql::<Core>(sql).unwrap();
    assert_eq!(statements.len(), 1, "{sql}");
    statements.remove(0)
}

fn rejected(sql: &str) -> bool {
    parse_sql::<Core>(sql).is_err()
}

fn assert_round_trip<D: Dialect>(sql: &str) {
    let statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    assert_eq!(statements[0].to_string(), sql);
}

fn assert_all_round_trip(cases: &[&str]) {
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

fn assert_all_rejected(cases: &[&str]) {
    for sql in cases {
        assert!(rejected(sql), "{sql}");
    }
}

fn alter_ops(sql: &str) -> Vec<AlterTableOp<'_, Builtin>> {
    match one(sql).kind {
        StatementKind::AlterTable(alter) => alter.operations,
        other => panic!("expected ALTER TABLE, found {other:?}"),
    }
}

fn create_table(sql: &str) -> CreateTable<'_, Builtin> {
    match one(sql).kind {
        StatementKind::CreateTable(create) => *create,
        other => panic!("expected CREATE TABLE, found {other:?}"),
    }
}

// DROP

#[test]
fn drop_records_its_parts() {
    let StatementKind::Drop(parsed) = one("DROP TABLE IF EXISTS a, s.b CASCADE").kind else {
        panic!("expected DROP");
    };
    assert_eq!(parsed.object_type, ObjectType::Table);
    assert!(parsed.if_exists);
    assert_eq!(
        parsed.names,
        vec![object_name(&["a"]), object_name(&["s", "b"])]
    );
    assert_eq!(parsed.behavior, Some(DropBehavior::Cascade));
}

#[test]
fn drop_object_types_are_recorded() {
    let cases = [
        ("DROP TABLE x", ObjectType::Table),
        ("DROP VIEW x", ObjectType::View),
        ("DROP MATERIALIZED VIEW x", ObjectType::MaterializedView),
        ("DROP INDEX x", ObjectType::Index),
        ("DROP SCHEMA x", ObjectType::Schema),
        ("DROP DATABASE x", ObjectType::Database),
        ("DROP SEQUENCE x", ObjectType::Sequence),
    ];
    for (sql, object_type) in cases {
        let StatementKind::Drop(parsed) = one(sql).kind else {
            panic!("expected DROP");
        };
        assert_eq!(parsed.object_type, object_type, "{sql}");
    }
}

#[test]
fn drop_round_trips() {
    assert_all_round_trip(&[
        "DROP TABLE t",
        "DROP TABLE IF EXISTS a, s.b CASCADE",
        "DROP VIEW v RESTRICT",
        "DROP MATERIALIZED VIEW IF EXISTS m",
        "DROP INDEX i",
        "DROP SCHEMA s CASCADE",
        "DROP DATABASE d",
        "DROP SEQUENCE q",
    ]);
}

#[test]
fn malformed_drop_is_rejected() {
    assert_all_rejected(&[
        "DROP t",
        "DROP TABLE",
        "DROP TABLE a,",
        "DROP TABLE IF t",
        "DROP MATERIALIZED TABLE t",
    ]);
}

// TRUNCATE

#[test]
fn truncate_records_its_tables() {
    let StatementKind::Truncate(parsed) = one("TRUNCATE TABLE a, s.b").kind else {
        panic!("expected TRUNCATE");
    };
    assert!(parsed.table_keyword);
    assert_eq!(
        parsed.names,
        vec![object_name(&["a"]), object_name(&["s", "b"])]
    );
}

#[test]
fn truncate_round_trips() {
    assert_all_round_trip(&[
        "TRUNCATE t",
        "TRUNCATE TABLE a, b",
        "TRUNCATE TABLE IF EXISTS t",
    ]);
}

#[test]
fn malformed_truncate_is_rejected() {
    assert_all_rejected(&["TRUNCATE", "TRUNCATE TABLE"]);
}

// ALTER TABLE

#[test]
fn alter_table_keeps_every_operation() {
    assert_eq!(
        alter_ops("ALTER TABLE t ADD a int, DROP b, RENAME TO u").len(),
        3
    );
}

#[test]
fn alter_add_column_records_how_it_was_written() {
    let statements = parse_sql::<Postgres>("ALTER TABLE t ADD COLUMN IF NOT EXISTS a int").unwrap();
    let StatementKind::AlterTable(alter) = &statements[0].kind else {
        panic!("expected ALTER TABLE");
    };
    let ops = &alter.operations;
    let AlterTableOp::AddColumn {
        column_keyword,
        if_not_exists,
        column,
        ..
    } = &ops[0]
    else {
        panic!("expected ADD COLUMN");
    };
    assert!(*column_keyword);
    assert!(*if_not_exists);
    assert_eq!(column.name, ident("a"));
}

#[test]
fn alter_drop_column_records_behavior() {
    let ops = alter_ops("ALTER TABLE t DROP COLUMN IF EXISTS a CASCADE");
    assert_eq!(
        ops[0],
        AlterTableOp::DropColumn {
            column_keyword: true,
            if_exists: true,
            name: ident("a"),
            behavior: Some(DropBehavior::Cascade),
        }
    );
}

#[test]
fn alter_column_actions_are_recorded() {
    let cases = [
        (
            "ALTER TABLE t ALTER COLUMN a SET NOT NULL",
            AlterColumnAction::SetNotNull,
        ),
        (
            "ALTER TABLE t ALTER COLUMN a DROP NOT NULL",
            AlterColumnAction::DropNotNull,
        ),
        (
            "ALTER TABLE t ALTER COLUMN a DROP DEFAULT",
            AlterColumnAction::DropDefault,
        ),
        (
            "ALTER TABLE t ALTER COLUMN a SET DEFAULT 1",
            AlterColumnAction::SetDefault(num("1")),
        ),
    ];
    for (sql, action) in cases {
        let ops = alter_ops(sql);
        assert_eq!(
            ops[0],
            AlterTableOp::AlterColumn {
                column_keyword: true,
                name: ident("a"),
                action,
            },
            "{sql}"
        );
    }
}

#[test]
fn alter_table_round_trips() {
    assert_all_round_trip(&[
        "ALTER TABLE t ADD COLUMN a int",
        "ALTER TABLE t ADD a int NOT NULL",
        "ALTER TABLE t DROP COLUMN a",
        "ALTER TABLE t DROP a",
        "ALTER TABLE t DROP COLUMN IF EXISTS a RESTRICT",
        "ALTER TABLE t RENAME COLUMN a TO b",
        "ALTER TABLE t RENAME TO u",
        "ALTER TABLE t ALTER COLUMN a SET NOT NULL",
        "ALTER TABLE t ALTER a DROP NOT NULL",
        "ALTER TABLE t ALTER COLUMN a SET DEFAULT 1 + 2",
        "ALTER TABLE t ALTER COLUMN a DROP DEFAULT",
        "ALTER TABLE t ALTER COLUMN a TYPE varchar(10)",
        "ALTER TABLE t ALTER COLUMN a SET DATA TYPE int",
        "ALTER TABLE t ADD CONSTRAINT c PRIMARY KEY (a)",
        "ALTER TABLE t ADD UNIQUE (a, b)",
        "ALTER TABLE t ADD CHECK (a > 1)",
        "ALTER TABLE t ADD FOREIGN KEY (a) REFERENCES u (b)",
        "ALTER TABLE t DROP CONSTRAINT c",
        "ALTER TABLE t DROP CONSTRAINT IF EXISTS c CASCADE",
        "ALTER TABLE IF EXISTS s.t ADD a int, DROP b",
    ]);
}

#[test]
fn malformed_alter_table_is_rejected() {
    assert_all_rejected(&[
        "ALTER TABLE t",
        "ALTER TABLE t ADD",
        "ALTER TABLE t ADD COLUMN a",
        "ALTER TABLE t DROP",
        "ALTER TABLE t RENAME",
        "ALTER TABLE t RENAME a",
        "ALTER TABLE t RENAME COLUMN a",
        "ALTER TABLE t ALTER COLUMN a",
        "ALTER TABLE t ALTER COLUMN a SET",
        "ALTER TABLE t ALTER COLUMN a TYPE",
        "ALTER TABLE t ADD a int,",
        "ALTER t ADD a int",
    ]);
}

#[test]
fn only_is_for_the_postgres_family_of_alter_table() {
    assert_round_trip::<Postgres>("ALTER TABLE ONLY t ADD a int");
    assert_round_trip::<Postgres>("ALTER TABLE IF EXISTS ONLY t DROP a");
    assert!(parse_sql::<MySql>("ALTER TABLE ONLY t ADD a int").is_err());
    assert!(parse_sql::<Core>("ALTER TABLE ONLY t ADD a int").is_err());
}

#[test]
fn only_can_still_name_a_table_in_mysql() {
    assert_round_trip::<MySql>("ALTER TABLE only ADD a int");
}

// CREATE TABLE

#[test]
fn create_table_records_or_replace_and_temporary() {
    let parsed = create_table("CREATE OR REPLACE TEMPORARY TABLE t (a int)");
    assert!(parsed.or_replace);
    assert!(parsed.temporary);
}

#[test]
fn plain_create_table_has_neither_flag() {
    let parsed = create_table("CREATE TABLE t (a int)");
    assert!(!parsed.or_replace);
    assert!(!parsed.temporary);
}

#[test]
fn temp_is_a_spelling_of_temporary() {
    assert!(create_table("CREATE TEMP TABLE t (a int)").temporary);
}

#[test]
fn named_constraint_keeps_its_name() {
    let parsed = create_table("CREATE TABLE t (a int, CONSTRAINT c UNIQUE (a))");
    assert_eq!(
        parsed.constraints,
        vec![TableConstraint {
            name: Some(ident("c")),
            kind: TableConstraintKind::Unique(vec![ident("a")]),
            include: Vec::new(),
            nulls_distinct: None,
            attributes: Vec::new(),
        }]
    );
}

#[test]
fn check_constraint_keeps_its_condition() {
    let parsed = create_table("CREATE TABLE t (a int, CHECK (a))");
    assert_eq!(
        parsed.constraints[0].kind,
        TableConstraintKind::Check(col("a"))
    );
}

#[test]
fn foreign_key_keeps_its_references_and_actions() {
    let parsed = create_table(
        "CREATE TABLE t (a int, FOREIGN KEY (a) REFERENCES s.u (b) ON DELETE CASCADE ON UPDATE SET NULL)",
    );
    assert_eq!(
        parsed.constraints[0].kind,
        TableConstraintKind::ForeignKey {
            index_name: None,
            columns: vec![ident("a")],
            references: ForeignKeyRef {
                table: object_name(&["s", "u"]),
                columns: vec![ident("b")],
                on_delete: Some(ReferentialAction::Cascade),
                on_update: Some(ReferentialAction::SetNull),
                match_kind: None,
            },
        }
    );
}

#[test]
fn column_check_and_references_are_options() {
    let parsed = create_table("CREATE TABLE t (a int CHECK (a) REFERENCES u)");
    assert_eq!(
        parsed.columns[0].options,
        vec![
            ColumnOption::Check(col("a")),
            ColumnOption::References(ForeignKeyRef {
                table: object_name(&["u"]),
                columns: vec![],
                on_delete: None,
                on_update: None,
                match_kind: None,
            }),
        ]
    );
}

#[test]
fn constraint_words_can_still_name_columns_when_used_as_names() {
    let parsed = create_table("CREATE TABLE t (constraint int, check int, foreign int)");
    assert_eq!(parsed.columns.len(), 3);
    assert!(parsed.constraints.is_empty());
}

#[test]
fn multi_word_types_are_recorded() {
    let cases = [
        ("DOUBLE PRECISION", DataTypeSuffix::Precision),
        ("CHARACTER VARYING", DataTypeSuffix::Varying),
        ("TIMESTAMP WITH TIME ZONE", DataTypeSuffix::WithTimeZone),
        ("TIME WITHOUT TIME ZONE", DataTypeSuffix::WithoutTimeZone),
    ];
    for (ty, suffix) in cases {
        let sql = format!("CREATE TABLE t (a {ty})");
        assert_eq!(
            create_table(&sql).columns[0].data_type.suffix,
            suffix,
            "{ty}"
        );
    }
}

#[test]
fn create_table_round_trips() {
    assert_all_round_trip(&[
        "CREATE OR REPLACE TABLE t (a int)",
        "CREATE TEMPORARY TABLE t (a int)",
        "CREATE TABLE t (a int, CONSTRAINT c PRIMARY KEY (a))",
        "CREATE TABLE t (a int, CHECK (a > 1))",
        "CREATE TABLE t (a int, FOREIGN KEY (a) REFERENCES u (b))",
        "CREATE TABLE t (a int, CONSTRAINT f FOREIGN KEY (a) REFERENCES u (b) ON DELETE NO ACTION ON UPDATE RESTRICT)",
        "CREATE TABLE t (a int REFERENCES u (b) ON DELETE SET DEFAULT)",
        "CREATE TABLE t (a int CHECK (a > 1) NOT NULL)",
        "CREATE TABLE t (a DOUBLE PRECISION)",
        "CREATE TABLE t (a CHARACTER VARYING(10))",
        "CREATE TABLE t (a TIMESTAMP(3) WITH TIME ZONE)",
        "CREATE TABLE t (a TIME WITHOUT TIME ZONE)",
    ]);
}

#[test]
fn temp_renders_as_temporary() {
    let rendered = one("CREATE TEMP TABLE t (a int)").to_string();
    assert_eq!(rendered, "CREATE TEMPORARY TABLE t (a int)");
}

// CREATE VIEW, INDEX, SCHEMA, DATABASE

#[test]
fn create_view_records_its_parts() {
    let StatementKind::CreateView(parsed) =
        one("CREATE OR REPLACE VIEW s.v (a, b) AS SELECT 1, 2").kind
    else {
        panic!("expected CREATE VIEW");
    };
    assert!(parsed.or_replace);
    assert!(!parsed.materialized);
    assert_eq!(parsed.name, object_name(&["s", "v"]));
    assert_eq!(parsed.columns, vec![ident("a"), ident("b")]);
}

#[test]
fn create_index_records_its_parts() {
    let StatementKind::CreateIndex(parsed) =
        one("CREATE UNIQUE INDEX IF NOT EXISTS i ON t USING btree (a DESC, b)").kind
    else {
        panic!("expected CREATE INDEX");
    };
    assert!(parsed.unique);
    assert!(parsed.if_not_exists);
    assert_eq!(parsed.name, Some(object_name(&["i"])));
    assert_eq!(parsed.table, object_name(&["t"]));
    assert_eq!(parsed.using, Some(ident("btree")));
    assert_eq!(parsed.columns.len(), 2);
    assert_eq!(parsed.columns[0].item.asc, Some(false));
}

#[test]
fn create_index_name_is_optional() {
    let StatementKind::CreateIndex(parsed) = one("CREATE INDEX ON t (a)").kind else {
        panic!("expected CREATE INDEX");
    };
    assert_eq!(parsed.name, None);
}

#[test]
fn create_family_round_trips() {
    assert_all_round_trip(&[
        "CREATE VIEW v AS SELECT 1",
        "CREATE OR REPLACE VIEW v (a, b) AS SELECT 1, 2",
        "CREATE TEMPORARY VIEW v AS SELECT 1",
        "CREATE MATERIALIZED VIEW IF NOT EXISTS s.m AS SELECT 1",
        "CREATE INDEX i ON t (a)",
        "CREATE INDEX ON t (a)",
        "CREATE UNIQUE INDEX IF NOT EXISTS i ON t USING btree (a DESC, b)",
        "CREATE INDEX i ON t (lower(a))",
        "CREATE SCHEMA s",
        "CREATE SCHEMA IF NOT EXISTS a.b",
        "CREATE DATABASE d",
        "CREATE DATABASE IF NOT EXISTS d",
    ]);
}

#[test]
fn malformed_create_is_rejected() {
    assert_all_rejected(&[
        "CREATE",
        "CREATE FROBNICATOR x",
        "CREATE OR TABLE t (a int)",
        "CREATE UNIQUE TABLE t (a int)",
        "CREATE MATERIALIZED TABLE t (a int)",
        "CREATE TEMPORARY INDEX i ON t (a)",
        "CREATE VIEW v",
        "CREATE VIEW v AS",
        "CREATE INDEX i t (a)",
        "CREATE INDEX i ON t",
        "CREATE INDEX i ON t ()",
        "CREATE SCHEMA",
    ]);
}

#[test]
fn dialect_rules_apply_inside_new_statements() {
    assert_round_trip::<MySql>("CREATE VIEW `v` AS SELECT `a` FROM t LIMIT 1, 2");
    assert_round_trip::<Postgres>("CREATE VIEW v AS SELECT x::int");
    assert_round_trip::<Redshift>("CREATE VIEW v AS SELECT TOP 1 a FROM t");
    assert!(parse_sql::<Core>("CREATE VIEW v AS SELECT x::int").is_err());
}

#[test]
fn referential_actions_cannot_repeat() {
    assert_all_rejected(&[
        "CREATE TABLE t (a int REFERENCES u ON DELETE CASCADE ON DELETE SET NULL)",
        "CREATE TABLE t (a int REFERENCES u ON UPDATE CASCADE ON UPDATE SET NULL)",
    ]);
}

#[test]
fn referential_actions_can_come_in_either_order() {
    let parsed =
        create_table("CREATE TABLE t (a int REFERENCES u ON UPDATE CASCADE ON DELETE RESTRICT)");
    let ColumnOption::References(references) = &parsed.columns[0].options[0] else {
        panic!("expected REFERENCES");
    };
    assert_eq!(references.on_delete, Some(ReferentialAction::Restrict));
    assert_eq!(references.on_update, Some(ReferentialAction::Cascade));
}

#[test]
fn add_column_if_not_exists_is_for_postgres() {
    let sql = "ALTER TABLE t ADD COLUMN IF NOT EXISTS a int";
    assert_round_trip::<Postgres>(sql);
    assert!(parse_sql::<Core>(sql).is_err());
    assert!(parse_sql::<MySql>(sql).is_err());
    assert!(parse_sql::<Redshift>(sql).is_err());
}
