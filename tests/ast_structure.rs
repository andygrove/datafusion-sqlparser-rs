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

//! Checks the shape of the tree for statements whose tests elsewhere only look
//! at the rendered SQL, which cannot show a value stored in the wrong place.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinStatement};
use sqlparser::dialect::snowflake::{CopyLocation, CopySource, SnowflakeStatement};
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Snowflake};
use sqlparser::parse_sql;

fn one<D: Dialect<Ext = Builtin>>(sql: &'static str) -> Statement<'static, Builtin> {
    let mut statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements.remove(0)
}

fn c(name: &'static str) -> Expr<'static, Builtin> {
    col(name)
}

fn n(text: &'static str) -> Expr<'static, Builtin> {
    num(text)
}

fn special(expr: SpecialExpr<'static, Builtin>) -> Expr<'static, Builtin> {
    ExprKind::Special(Box::new(expr)).into()
}

fn names<'q>(grantees: &'q [Grantee<'static>]) -> Vec<&'q ObjectName<'static>> {
    grantees.iter().map(|grantee| &grantee.name).collect()
}

// Access control

#[test]
fn grant_separates_objects_grantees_and_options() {
    let StatementKind::Grant(grant) =
        one::<Core>("GRANT SELECT ON ALL TABLES IN SCHEMA s TO a, b WITH GRANT OPTION").kind
    else {
        panic!("expected GRANT");
    };
    let objects = grant.objects.as_ref().unwrap();
    assert_eq!(objects.kind, ["ALL", "TABLES", "IN", "SCHEMA"]);
    assert_eq!(objects.names.len(), 1);
    assert_eq!(objects.names[0].name, object_name(&["s"]));
    assert_eq!(
        names(&grant.grantees),
        [&object_name(&["a"]), &object_name(&["b"])]
    );
    assert!(grant.with_grant_option);
}

#[test]
fn granting_a_role_has_no_objects() {
    let StatementKind::Grant(grant) = one::<Core>("GRANT ROLE r TO u").kind else {
        panic!("expected GRANT");
    };
    assert_eq!(grant.objects, None);
    assert_eq!(names(&grant.grantees), [&object_name(&["u"])]);
}

#[test]
fn revoke_keeps_grant_option_and_behavior() {
    let StatementKind::Revoke(revoke) =
        one::<Core>("REVOKE GRANT OPTION FOR SELECT ON t FROM u CASCADE").kind
    else {
        panic!("expected REVOKE");
    };
    assert!(revoke.grant_option_for);
    assert_eq!(revoke.objects.names[0].name, object_name(&["t"]));
    assert_eq!(names(&revoke.grantees), [&object_name(&["u"])]);
    assert_eq!(revoke.behavior, Some(DropBehavior::Cascade));
}

#[test]
fn drop_function_keeps_each_signature() {
    let StatementKind::DropRoutine(drop) =
        one::<Postgres>("DROP FUNCTION f(a INT, OUT TEXT), g CASCADE").kind
    else {
        panic!("expected DROP FUNCTION");
    };
    assert!(!drop.procedure);
    assert_eq!(drop.routines.len(), 2);
    let args = drop.routines[0].args.as_ref().unwrap();
    assert_eq!(args[0].mode, None);
    assert_eq!(args[0].name, Some(ident("a")));
    assert_eq!(args[1].mode.as_deref(), Some("OUT"));
    assert_eq!(args[1].name, None);
    assert_eq!(drop.routines[1].name, object_name(&["g"]));
    assert_eq!(drop.routines[1].args, None);
    assert_eq!(drop.behavior, Some(DropBehavior::Cascade));
}

// MERGE

#[test]
fn merge_keeps_clauses_with_their_conditions() {
    let StatementKind::Merge(merge) = one::<Core>(
        "MERGE INTO t USING s ON a WHEN MATCHED AND b THEN DELETE \
         WHEN NOT MATCHED THEN INSERT (c) VALUES (1)",
    )
    .kind
    else {
        panic!("expected MERGE");
    };
    assert_eq!(merge.on, c("a"));
    assert_eq!(table_name(&merge.table), &object_name(&["t"]));
    assert_eq!(table_name(&merge.source), &object_name(&["s"]));
    assert_eq!(merge.clauses.len(), 2);
    assert_eq!(merge.clauses[0].kind, MergeClauseKind::Matched);
    assert_eq!(merge.clauses[0].predicate, Some(c("b")));
    assert_eq!(merge.clauses[0].action, MergeAction::Delete);
    assert_eq!(merge.clauses[1].kind, MergeClauseKind::NotMatched);
    assert_eq!(merge.clauses[1].predicate, None);
    let MergeAction::Insert { columns, rows, .. } = &merge.clauses[1].action else {
        panic!("expected INSERT");
    };
    assert_eq!(columns, &[object_name(&["c"])]);
    assert_eq!(rows, &[vec![n("1")]]);
}

// Session statements

#[test]
fn show_separates_words_filters_and_locations() {
    let StatementKind::Show(show) = one::<Snowflake>("SHOW TERSE TABLES LIKE 'a' IN SCHEMA s").kind
    else {
        panic!("expected SHOW");
    };
    assert_eq!(show.modifiers, ["TERSE"]);
    assert_eq!(show.object, ["TABLES"]);
    assert_eq!(show.name, None);
    let [
        ShowClause::Like(pattern),
        ShowClause::Location { from, scope, name },
    ] = show.clauses.as_slice()
    else {
        panic!("unexpected clauses: {:?}", show.clauses);
    };
    assert_eq!(pattern.text, "a");
    assert!(!from);
    assert_eq!(scope.as_deref(), Some("SCHEMA"));
    assert_eq!(name.as_ref(), Some(&object_name(&["s"])));
}

#[test]
fn transaction_statements_keep_their_details() {
    let finish = |sql| match one::<Core>(sql).kind {
        StatementKind::Transaction(transaction) => *transaction,
        other => panic!("expected a transaction statement: {other:?}"),
    };
    assert_eq!(
        finish("BEGIN TRANSACTION"),
        Transaction::Begin(Some("TRANSACTION".into()))
    );
    assert_eq!(
        finish("COMMIT AND CHAIN"),
        Transaction::Finish {
            kind: FinishKind::Commit,
            chain: true,
            savepoint: None
        }
    );
    assert_eq!(
        finish("ROLLBACK TO SAVEPOINT s"),
        Transaction::Finish {
            kind: FinishKind::Rollback,
            chain: false,
            savepoint: Some(ident("s"))
        }
    );
    assert_eq!(finish("SAVEPOINT a"), Transaction::Savepoint(ident("a")));
    assert_eq!(
        finish("RELEASE SAVEPOINT a"),
        Transaction::Release(ident("a"))
    );
}

fn set_of<D: Dialect<Ext = Builtin>>(sql: &'static str) -> SetStatement<'static, Builtin> {
    match one::<D>(sql).kind {
        StatementKind::Set(set) => *set,
        other => panic!("expected SET: {other:?}"),
    }
}

#[test]
fn set_tells_its_forms_apart() {
    assert_eq!(
        set_of::<Postgres>("SET LOCAL ROLE admin"),
        SetStatement::Role {
            scope: Some("LOCAL".into()),
            role: c("admin")
        }
    );
    assert_eq!(
        set_of::<Core>("SET a.b = 1, 2"),
        SetStatement::Variable {
            scope: None,
            name: object_name(&["a", "b"]),
            values: vec![n("1"), n("2")]
        }
    );
}

#[test]
fn comment_on_null_has_no_text() {
    let StatementKind::Comment(comment) = one::<Postgres>("COMMENT ON TABLE a.b IS NULL").kind
    else {
        panic!("expected COMMENT");
    };
    assert_eq!(comment.kind, ["TABLE"]);
    assert_eq!(comment.name, object_name(&["a", "b"]));
    assert_eq!(comment.comment, None);
}

// Columns and constraints

fn create_table<D: Dialect<Ext = Builtin>>(sql: &'static str) -> CreateTable<'static, Builtin> {
    match one::<D>(sql).kind {
        StatementKind::CreateTable(table) => *table,
        other => panic!("expected CREATE TABLE: {other:?}"),
    }
}

#[test]
fn generated_columns_keep_their_kind() {
    let table = create_table::<Core>(
        "CREATE TABLE t (a INT GENERATED BY DEFAULT AS IDENTITY, \
         b INT GENERATED ALWAYS AS (a + 1) STORED)",
    );
    let [ColumnOption::Generated(identity)] = table.columns[0].options.as_slice() else {
        panic!("unexpected options: {:?}", table.columns[0].options);
    };
    assert_eq!(identity.when, Some(GeneratedWhen::ByDefault));
    assert_eq!(identity.kind, GeneratedKind::Identity(None));
    let [ColumnOption::Generated(computed)] = table.columns[1].options.as_slice() else {
        panic!("unexpected options: {:?}", table.columns[1].options);
    };
    assert_eq!(computed.when, Some(GeneratedWhen::Always));
    assert_eq!(
        computed.kind,
        GeneratedKind::Expr {
            expr: bin(c("a"), Add, n("1")),
            storage: Some(GeneratedStorage::Stored)
        }
    );
}

#[test]
fn table_constraints_hold_their_own_attributes() {
    let table = create_table::<Core>(
        "CREATE TABLE t (a INT, CONSTRAINT c UNIQUE (a) DEFERRABLE INITIALLY DEFERRED, CHECK (a))",
    );
    assert_eq!(table.constraints.len(), 2);
    assert_eq!(table.constraints[0].name, Some(ident("c")));
    assert_eq!(
        table.constraints[0].kind,
        TableConstraintKind::Unique(vec![ident("a")])
    );
    assert_eq!(
        table.constraints[0].attributes,
        [
            ConstraintAttribute::Deferrable(true),
            ConstraintAttribute::InitiallyDeferred(true)
        ]
    );
    assert_eq!(table.constraints[1].attributes, []);
}

#[test]
fn column_attributes_follow_their_constraint() {
    let table =
        create_table::<Core>("CREATE TABLE t (a INT UNIQUE NOT DEFERRABLE CHECK (a) NOT ENFORCED)");
    let positions: Vec<_> = table.columns[0]
        .options
        .iter()
        .filter_map(|option| match option {
            ColumnOption::Attribute(attribute) => Some(*attribute),
            _ => None,
        })
        .collect();
    assert_eq!(
        positions,
        [
            ConstraintAttribute::Deferrable(false),
            ConstraintAttribute::Enforced(false)
        ]
    );
    assert!(matches!(
        table.columns[0].options[1],
        ColumnOption::Attribute(ConstraintAttribute::Deferrable(false))
    ));
    assert!(matches!(
        table.columns[0].options[3],
        ColumnOption::Attribute(ConstraintAttribute::Enforced(false))
    ));
}

// ALTER TABLE

fn first_alter<D: Dialect<Ext = Builtin>>(sql: &'static str) -> AlterTableOp<'static, Builtin> {
    match one::<D>(sql).kind {
        StatementKind::AlterTable(mut alter) => alter.operations.remove(0),
        other => panic!("expected ALTER TABLE: {other:?}"),
    }
}

#[test]
fn mysql_change_separates_old_and_new_names() {
    let AlterTableOp::ChangeColumn {
        old,
        column,
        position,
    } = first_alter::<MySql>("ALTER TABLE t CHANGE a b INT FIRST")
    else {
        panic!("expected CHANGE");
    };
    assert_eq!(old, ident("a"));
    assert_eq!(column.name, ident("b"));
    assert_eq!(position, Some(ColumnPosition::First));
}

#[test]
fn mysql_modify_keeps_its_position() {
    let AlterTableOp::ModifyColumn { column, position } =
        first_alter::<MySql>("ALTER TABLE t MODIFY a INT AFTER b")
    else {
        panic!("expected MODIFY");
    };
    assert_eq!(column.name, ident("a"));
    assert_eq!(position, Some(ColumnPosition::After(ident("b"))));
}

#[test]
fn alter_column_type_keeps_its_conversion() {
    let AlterTableOp::AlterColumn {
        column_keyword,
        name,
        action,
    } = first_alter::<Postgres>("ALTER TABLE t ALTER COLUMN a TYPE INT USING a + 1")
    else {
        panic!("expected ALTER COLUMN");
    };
    assert!(column_keyword);
    assert_eq!(name, ident("a"));
    let AlterColumnAction::SetType {
        set_data, using, ..
    } = action
    else {
        panic!("expected TYPE");
    };
    assert!(!set_data);
    assert_eq!(using, Some(bin(c("a"), Add, n("1"))));
}

#[test]
fn storage_parameters_keep_qualified_names_and_missing_values() {
    let AlterTableOp::SetOptions(options) =
        first_alter::<Postgres>("ALTER TABLE t SET (toast.x = 1, fillfactor)")
    else {
        panic!("expected SET");
    };
    assert_eq!(
        options,
        [
            (object_name(&["toast", "x"]), Some(n("1"))),
            (object_name(&["fillfactor"]), None)
        ]
    );
}

// Indexes

fn create_index<D: Dialect<Ext = Builtin>>(sql: &'static str) -> CreateIndex<'static, Builtin> {
    match one::<D>(sql).kind {
        StatementKind::CreateIndex(index) => *index,
        other => panic!("expected CREATE INDEX: {other:?}"),
    }
}

#[test]
fn each_using_position_has_its_own_field() {
    let index = create_index::<Core>("CREATE INDEX i USING BTREE ON t (a) USING HASH");
    assert_eq!(index.using_before_on, Some(ident("BTREE")));
    assert_eq!(index.using, None);
    assert_eq!(index.trailing_using, Some(ident("HASH")));

    let index = create_index::<Core>("CREATE INDEX i ON t USING BTREE (a)");
    assert_eq!(index.using_before_on, None);
    assert_eq!(index.using, Some(ident("BTREE")));
    assert_eq!(index.trailing_using, None);
}

#[test]
fn index_columns_keep_their_class_and_ordering() {
    let index = create_index::<Core>("CREATE INDEX i ON t (a int4_ops DESC NULLS FIRST)");
    let column = &index.columns[0];
    assert_eq!(column.item.expr, c("a"));
    assert_eq!(column.operator_class, Some(object_name(&["int4_ops"])));
    assert_eq!(column.item.asc, Some(false));
    assert_eq!(column.item.nulls_first, Some(true));
}

#[test]
fn index_flags_are_separate() {
    let index = create_index::<Postgres>("CREATE INDEX CONCURRENTLY i ON t (a)");
    assert!(index.concurrently);
    assert!(!index.asynchronous);
    let index = create_index::<Postgres>("CREATE INDEX ASYNC i ON t (a)");
    assert!(!index.concurrently);
    assert!(index.asynchronous);
    assert_eq!(index.name, Some(object_name(&["i"])));
}

// Expressions

#[test]
fn at_time_zone_binds_tighter_than_arithmetic() {
    let StatementKind::Query(query) = one::<Core>("SELECT a + b AT TIME ZONE 'z'").kind else {
        panic!("expected a query");
    };
    let SelectItemKind::Expr { expr, .. } = &body(&query).projection[0].kind else {
        panic!("expected an expression");
    };
    let ExprKind::Binary { left, op, right } = &expr.kind else {
        panic!("expected addition: {expr:?}");
    };
    assert_eq!(**left, c("a"));
    assert_eq!(*op, Add);
    let ExprKind::Special(special_expr) = &right.kind else {
        panic!("expected AT TIME ZONE: {right:?}");
    };
    let SpecialExpr::AtTimeZone { expr, .. } = &**special_expr else {
        panic!("expected AT TIME ZONE: {right:?}");
    };
    assert_eq!(*expr, c("b"));
}

#[test]
fn connect_by_keeps_its_clauses_in_order_and_prior_binds_to_one_side() {
    let StatementKind::Query(query) =
        one::<Snowflake>("SELECT a FROM t START WITH b CONNECT BY NOCYCLE PRIOR c = d").kind
    else {
        panic!("expected a query");
    };
    assert_eq!(
        body(&query).connect_by,
        [
            ConnectBy::StartWith(c("b")),
            ConnectBy::ConnectBy {
                nocycle: true,
                relationships: vec![bin(special(SpecialExpr::Prior(c("c"))), Eq, c("d"))]
            }
        ]
    );
}

// Snowflake loading

#[test]
fn copy_into_separates_target_columns_and_source() {
    let StatementKind::Ext(BuiltinStatement::Snowflake(SnowflakeStatement::CopyInto(copy))) =
        one::<Snowflake>("COPY INTO t (a, b) FROM @stage/path").kind
    else {
        panic!("expected COPY INTO");
    };
    assert_eq!(copy.into, CopyLocation::Name(object_name(&["t"])));
    assert_eq!(copy.columns, [ident("a"), ident("b")]);
    assert_eq!(
        copy.from,
        CopySource::Location(CopyLocation::Stage(ident("@stage/path")))
    );
}
