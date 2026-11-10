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

//! `INSERT`, `UPDATE`, `DELETE`, `CREATE TABLE` and statement lists.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::DialectKind;
use sqlparser::dialect::builtin::{Builtin, BuiltinInsert, parse_sql_as};
use sqlparser::dialect::{Core, MySql, Postgres, Redshift};
use sqlparser::parse_sql;

type St<'a> = Statement<'a, Builtin>;

fn one(sql: &str) -> St<'_> {
    let mut statements = parse_sql::<Core>(sql).unwrap();
    assert_eq!(statements.len(), 1, "{sql}");
    statements.remove(0)
}

fn insert(sql: &str) -> Insert<'_, Builtin> {
    match one(sql).kind {
        StatementKind::Insert(insert) => *insert,
        other => panic!("expected an INSERT, found {other:?}"),
    }
}

fn update(sql: &str) -> Update<'_, Builtin> {
    match one(sql).kind {
        StatementKind::Update(update) => *update,
        other => panic!("expected an UPDATE, found {other:?}"),
    }
}

fn delete(sql: &str) -> Delete<'_, Builtin> {
    match one(sql).kind {
        StatementKind::Delete(delete) => *delete,
        other => panic!("expected a DELETE, found {other:?}"),
    }
}

fn create(sql: &str) -> CreateTable<'_, Builtin> {
    match one(sql).kind {
        StatementKind::CreateTable(create) => *create,
        other => panic!("expected a CREATE TABLE, found {other:?}"),
    }
}

fn rejected(sql: &str) -> bool {
    parse_sql::<Core>(sql).is_err()
}

fn wildcard_returning<'a>() -> Vec<SelectItem<'a, Builtin>> {
    vec![SelectItemKind::Wildcard.into()]
}

fn int_column(name: &str) -> ColumnDef<'_, Builtin> {
    ColumnDef {
        name: ident(name),
        data_type: DataType {
            qualifier: Vec::new(),
            angle: None,
            name: ident("int"),
            args: vec![],
            suffix: DataTypeSuffix::None,
            array: ArrayType::None,
        },
        options: vec![],
    }
}

// Statement lists

#[test]
fn semicolons_separate_statements() {
    assert_eq!(parse_sql::<Core>("SELECT 1; SELECT 2").unwrap().len(), 2);
}

#[test]
fn trailing_and_repeated_semicolons_are_ignored() {
    assert_eq!(parse_sql::<Core>(";SELECT 1;; SELECT 2;").unwrap().len(), 2);
}

#[test]
fn empty_input_has_no_statements() {
    assert!(parse_sql::<Core>("").unwrap().is_empty());
}

#[test]
fn statements_require_a_separator() {
    assert!(rejected("SELECT 1 SELECT 2"));
}

#[test]
fn unknown_statement_is_rejected() {
    assert!(rejected("FROBNICATE t"));
}

#[test]
fn query_statement_holds_the_query() {
    let StatementKind::Query(query) = one("SELECT 1 LIMIT 2").kind else {
        panic!("expected a query");
    };
    assert_eq!(query.limit.unwrap().count, Some(num("2")));
}

#[test]
fn parenthesized_query_is_a_statement() {
    assert!(matches!(one("(SELECT 1)").kind, StatementKind::Query(_)));
}

// INSERT

#[test]
fn insert_values_keeps_the_row() {
    let parsed = insert("INSERT INTO t VALUES (1, a)");
    assert_eq!(parsed.table, object_name(&["t"]));
    assert!(parsed.columns.is_empty());
    assert_eq!(
        parsed.source,
        InsertSource::Values(vec![vec![num("1"), col("a")]])
    );
}

#[test]
fn insert_keeps_every_row_in_order() {
    let parsed = insert("INSERT INTO t VALUES (1), (2), (3)");
    assert_eq!(
        parsed.source,
        InsertSource::Values(vec![vec![num("1")], vec![num("2")], vec![num("3")]])
    );
}

#[test]
fn insert_keeps_the_column_list() {
    let parsed = insert("INSERT INTO t (a, b) VALUES (1, 2)");
    assert_eq!(parsed.columns, vec![ident("a"), ident("b")]);
}

#[test]
fn insert_table_may_be_qualified() {
    let parsed = insert("INSERT INTO s.t VALUES (1)");
    assert_eq!(parsed.table, object_name(&["s", "t"]));
}

#[test]
fn insert_values_accept_expressions() {
    let parsed = insert("INSERT INTO t VALUES (a + 1)");
    assert_eq!(
        parsed.source,
        InsertSource::Values(vec![vec![bin(col("a"), Add, num("1"))]])
    );
}

#[test]
fn insert_from_query_holds_the_query() {
    let parsed = insert("INSERT INTO t SELECT 1");
    assert!(matches!(parsed.source, InsertSource::Query(_)));
}

#[test]
fn insert_from_query_keeps_the_column_list() {
    let parsed = insert("INSERT INTO t (a) SELECT 1");
    assert_eq!(parsed.columns, vec![ident("a")]);
    assert!(matches!(parsed.source, InsertSource::Query(_)));
}

#[test]
fn parenthesized_query_is_not_a_column_list() {
    let parsed = insert("INSERT INTO t (SELECT 1)");
    assert!(parsed.columns.is_empty());
    assert!(matches!(parsed.source, InsertSource::Query(_)));
}

#[test]
fn insert_requires_into() {
    assert!(rejected("INSERT t VALUES (1)"));
}

#[test]
fn insert_requires_a_source() {
    assert!(rejected("INSERT INTO t"));
}

#[test]
fn insert_rejects_an_empty_row() {
    assert!(rejected("INSERT INTO t VALUES ()"));
}

// UPDATE

#[test]
fn update_keeps_its_assignments() {
    let parsed = update("UPDATE t SET a = 1, b = c + 2");
    assert_eq!(
        parsed.assignments,
        vec![
            Assignment {
                target: object_name(&["a"]),
                value: num("1"),
            },
            Assignment {
                target: object_name(&["b"]),
                value: bin(col("c"), Add, num("2")),
            },
        ]
    );
}

#[test]
fn update_target_may_be_qualified() {
    let parsed = update("UPDATE t SET t.a = 1");
    assert_eq!(parsed.assignments[0].target, object_name(&["t", "a"]));
}

#[test]
fn update_keeps_where() {
    let parsed = update("UPDATE t SET a = 1 WHERE b = 2");
    assert_eq!(parsed.selection, Some(bin(col("b"), Eq, num("2"))));
}

#[test]
fn update_table_may_have_an_alias() {
    let parsed = update("UPDATE t AS x SET a = 1");
    assert_eq!(parsed.table.alias, Some(explicit_alias("x")));
}

#[test]
fn set_is_not_taken_as_a_table_alias() {
    let parsed = update("UPDATE t SET a = 1");
    assert_eq!(parsed.table.alias, None);
}

#[test]
fn update_requires_set() {
    assert!(rejected("UPDATE t WHERE a = 1"));
}

#[test]
fn assignment_requires_a_value() {
    assert!(rejected("UPDATE t SET a ="));
}

// DELETE

#[test]
fn delete_keeps_its_table() {
    let parsed = delete("DELETE FROM s.t");
    assert_eq!(parsed.table.factor, named(&["s", "t"]));
    assert_eq!(parsed.selection, None);
}

#[test]
fn delete_keeps_where() {
    let parsed = delete("DELETE FROM t WHERE a = 1");
    assert_eq!(parsed.selection, Some(bin(col("a"), Eq, num("1"))));
}

#[test]
fn delete_table_may_have_an_alias() {
    let parsed = delete("DELETE FROM t x WHERE a");
    assert_eq!(parsed.table.alias, Some(implicit_alias("x")));
}

#[test]
fn delete_requires_from() {
    assert!(rejected("DELETE t"));
}

// CREATE TABLE

#[test]
fn create_table_keeps_its_columns() {
    let parsed = create("CREATE TABLE t (a int, b int)");
    assert_eq!(parsed.name, object_name(&["t"]));
    assert_eq!(parsed.columns, vec![int_column("a"), int_column("b")]);
    assert!(!parsed.if_not_exists);
}

#[test]
fn create_table_records_if_not_exists() {
    assert!(create("CREATE TABLE IF NOT EXISTS t (a int)").if_not_exists);
}

#[test]
fn data_type_keeps_its_arguments() {
    let parsed = create("CREATE TABLE t (a decimal(10, 2), b varchar(255))");
    assert_eq!(
        parsed.columns[0].data_type,
        DataType {
            qualifier: Vec::new(),
            angle: None,
            name: ident("decimal"),
            args: vec!["10".into(), "2".into()],
            suffix: DataTypeSuffix::None,
            array: ArrayType::None,
        }
    );
    assert_eq!(parsed.columns[1].data_type.args, vec!["255"]);
}

#[test]
fn column_options_are_kept_in_order() {
    let parsed = create("CREATE TABLE t (a int NOT NULL UNIQUE PRIMARY KEY, b int NULL)");
    assert_eq!(
        parsed.columns[0].options,
        vec![
            ColumnOption::NotNull,
            ColumnOption::Unique,
            ColumnOption::PrimaryKey,
        ]
    );
    assert_eq!(parsed.columns[1].options, vec![ColumnOption::Null]);
}

#[test]
fn default_option_keeps_its_expression() {
    let parsed = create("CREATE TABLE t (a int DEFAULT 1 + 2 NOT NULL)");
    assert_eq!(
        parsed.columns[0].options,
        vec![
            ColumnOption::Default(bin(num("1"), Add, num("2"))),
            ColumnOption::NotNull,
        ]
    );
}

#[test]
fn table_constraints_keep_their_columns() {
    let parsed = create("CREATE TABLE t (a int, b int, PRIMARY KEY (a, b), UNIQUE (b))");
    assert_eq!(parsed.columns.len(), 2);
    assert_eq!(
        parsed.constraints,
        vec![
            TableConstraint {
                name: None,
                kind: TableConstraintKind::PrimaryKey(vec![ident("a"), ident("b")]),
                include: Vec::new(),
                nulls_distinct: None,
                attributes: Vec::new(),
            },
            TableConstraint {
                name: None,
                kind: TableConstraintKind::Unique(vec![ident("b")]),
                include: Vec::new(),
                nulls_distinct: None,
                attributes: Vec::new(),
            },
        ]
    );
}

#[test]
fn constraint_words_can_still_name_columns() {
    let parsed = create("CREATE TABLE t (primary int, unique int)");
    assert_eq!(
        parsed.columns,
        vec![int_column("primary"), int_column("unique")]
    );
    assert!(parsed.constraints.is_empty());
}

#[test]
fn create_table_as_holds_the_query() {
    let parsed = create("CREATE TABLE t AS SELECT 1");
    assert!(parsed.columns.is_empty());
    assert!(parsed.query.is_some());
}

#[test]
fn create_table_requires_columns_or_a_query() {
    assert!(rejected("CREATE TABLE t"));
}

#[test]
fn create_table_rejects_an_empty_column_list() {
    assert!(rejected("CREATE TABLE t ()"));
}

#[test]
fn create_table_rejects_a_trailing_comma() {
    assert!(rejected("CREATE TABLE t (a int,)"));
}

#[test]
fn create_table_requires_a_closing_parenthesis() {
    assert!(rejected("CREATE TABLE t (a int"));
}

#[test]
fn column_requires_a_type() {
    assert!(rejected("CREATE TABLE t (a)"));
}

// Dialect differences

fn insert_as<D>(sql: &str) -> Insert<'_, Builtin>
where
    D: sqlparser::dialect::Dialect<Ext = Builtin>,
{
    let mut statements = parse_sql::<D>(sql).unwrap();
    match statements.remove(0).kind {
        StatementKind::Insert(insert) => *insert,
        other => panic!("expected an INSERT, found {other:?}"),
    }
}

#[test]
fn mysql_insert_ignore_is_recorded() {
    let parsed = insert_as::<MySql>("INSERT IGNORE INTO t VALUES (1)");
    assert_eq!(
        parsed.modifiers,
        BuiltinInsert {
            ignore: true,
            ..Default::default()
        }
    );
}

#[test]
fn mysql_plain_insert_is_not_ignore() {
    let parsed = insert_as::<MySql>("INSERT INTO t VALUES (1)");
    assert_eq!(
        parsed.modifiers,
        BuiltinInsert {
            ignore: false,
            ..Default::default()
        }
    );
}

#[test]
fn only_mysql_accepts_insert_ignore() {
    assert!(parse_sql::<Postgres>("INSERT IGNORE INTO t VALUES (1)").is_err());
    assert!(parse_sql::<Core>("INSERT IGNORE INTO t VALUES (1)").is_err());
}

#[test]
fn mysql_auto_increment_is_a_column_option() {
    let StatementKind::CreateTable(parsed) =
        parse_sql::<MySql>("CREATE TABLE t (`a` int AUTO_INCREMENT NOT NULL)")
            .unwrap()
            .remove(0)
            .kind
    else {
        panic!("expected a CREATE TABLE");
    };
    assert_eq!(parsed.columns[0].name, quoted("a", '`'));
    assert_eq!(
        parsed.columns[0].options,
        vec![ColumnOption::AutoIncrement, ColumnOption::NotNull]
    );
}

#[test]
fn only_mysql_accepts_auto_increment() {
    assert!(parse_sql::<Postgres>("CREATE TABLE t (a int AUTO_INCREMENT)").is_err());
}

#[test]
fn postgres_insert_returning_keeps_its_items() {
    let parsed = insert_as::<Postgres>("INSERT INTO t VALUES (1) RETURNING *");
    assert_eq!(parsed.returning, wildcard_returning());
}

#[test]
fn postgres_update_returning_keeps_its_items() {
    let StatementKind::Update(parsed) = parse_sql::<Postgres>("UPDATE t SET a = 1 RETURNING a")
        .unwrap()
        .remove(0)
        .kind
    else {
        panic!("expected an UPDATE");
    };
    assert_eq!(
        parsed.returning,
        vec![
            SelectItemKind::Expr {
                expr: col("a"),
                alias: None,
            }
            .into()
        ]
    );
}

#[test]
fn postgres_delete_returning_keeps_its_items() {
    let StatementKind::Delete(parsed) = parse_sql::<Postgres>("DELETE FROM t RETURNING *")
        .unwrap()
        .remove(0)
        .kind
    else {
        panic!("expected a DELETE");
    };
    assert_eq!(parsed.returning, wildcard_returning());
    assert_eq!(parsed.table.alias, None);
}

#[test]
fn returning_is_postgres_only() {
    let sql = "INSERT INTO t VALUES (1) RETURNING *";
    assert!(parse_sql::<Core>(sql).is_err());
    assert!(parse_sql::<MySql>(sql).is_err());
    assert!(parse_sql::<Redshift>(sql).is_err());
}

#[test]
fn returning_is_an_ordinary_alias_outside_postgres() {
    let parsed = delete("DELETE FROM t returning");
    assert_eq!(parsed.table.alias, Some(implicit_alias("returning")));
}

#[test]
fn postgres_cast_works_inside_a_statement() {
    assert!(parse_sql::<Postgres>("UPDATE t SET a = b::int").is_ok());
    assert!(parse_sql::<MySql>("UPDATE t SET a = b::int").is_err());
}

#[test]
fn dialect_chosen_at_runtime_parses_statements() {
    let sql = "INSERT IGNORE INTO t VALUES (1)";
    assert_eq!(parse_sql_as(DialectKind::MySql, sql).unwrap().len(), 1);
    assert!(parse_sql_as(DialectKind::Postgres, sql).is_err());
}

// Stack safety

#[test]
fn deep_expressions_in_values_are_rejected() {
    let sql = format!(
        "INSERT INTO t VALUES ({}1{})",
        "(".repeat(100_000),
        ")".repeat(100_000)
    );
    assert!(rejected(&sql));
}

#[test]
fn huge_chain_in_an_assignment_is_rejected() {
    let sql = format!("UPDATE t SET a = {}1", "1+".repeat(1_000_000));
    assert!(rejected(&sql));
}

#[test]
fn many_rows_do_not_count_as_height() {
    let rows = vec!["(1, 2)"; 10_000].join(", ");
    assert!(parse_sql::<Core>(&format!("INSERT INTO t VALUES {rows}")).is_ok());
}

#[test]
fn doubly_parenthesized_query_is_not_a_column_list() {
    let parsed = insert("INSERT INTO t ((SELECT 1))");
    assert!(parsed.columns.is_empty());
    assert!(matches!(parsed.source, InsertSource::Query(_)));
}
