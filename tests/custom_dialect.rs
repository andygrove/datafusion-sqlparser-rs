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

//! A dialect defined outside the crate, adding a token, an operator and an
//! AST node using only the public API.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::{AstExt, Core, Dialect, NoExt};
use sqlparser::grammar::{self, prec};
use sqlparser::lexer::{LexResult, Lexeme, TokenKind};
use sqlparser::parser::{InfixRule, Parser};
use sqlparser::{PResult, parse_expr, parse_query};

#[derive(Debug, Clone, PartialEq)]
struct Acme;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "visitor",
    derive(sqlparser::ast::Visit, sqlparser::ast::VisitMut),
    visit(ext = "Acme")
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct Coalesce<'a> {
    left: Box<Expr<'a, Acme>>,
    right: Box<Expr<'a, Acme>>,
}

fn parse_coalesce<'a>(
    p: &mut Parser<'a, Acme>,
    left: Expr<'a, Acme>,
    bp: u8,
) -> PResult<Expr<'a, Acme>> {
    p.advance();
    let right = p.parse_expr_bp(bp)?;
    Ok(ExprKind::Ext(Coalesce {
        left: Box::new(left),
        right: Box::new(right),
    })
    .into())
}

impl AstExt for Acme {
    type Expr<'a> = Coalesce<'a>;
    type Select<'a> = NoExt;
    type Statement<'a> = Ping<'a>;
    type Insert<'a> = NoExt;
    type TableFactor<'a> = Series<'a>;
    type JoinConstraint<'a> = Within<'a>;
    type OrderByItem<'a> = NoExt;
    type QueryTail<'a> = NoExt;
}

/// A table only this dialect has: `SERIES n`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "visitor",
    derive(sqlparser::ast::Visit, sqlparser::ast::VisitMut)
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct Series<'a> {
    count: &'a str,
}

impl std::fmt::Display for Series<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SERIES {}", self.count)
    }
}

/// A join condition only this dialect has: `WITHIN n`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "visitor",
    derive(sqlparser::ast::Visit, sqlparser::ast::VisitMut)
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct Within<'a> {
    distance: &'a str,
}

impl std::fmt::Display for Within<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WITHIN {}", self.distance)
    }
}

/// A statement that only this dialect has: `PING host`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "visitor",
    derive(sqlparser::ast::Visit, sqlparser::ast::VisitMut)
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct Ping<'a> {
    host: Ident<'a>,
}

impl Dialect for Acme {
    type Ext = Self;

    fn parse_statement<'a>(p: &mut Parser<'a, Self>) -> PResult<Statement<'a, Self>> {
        if p.eat_keyword("PING") {
            let host = p.parse_ident()?;
            return Ok(StatementKind::Ext(Ping { host }).into());
        }
        grammar::parse_statement(p)
    }

    fn parse_table_factor<'a>(p: &mut Parser<'a, Self>) -> PResult<TableFactor<'a, Self>> {
        if p.eat_keyword("SERIES") {
            let count = p.peek().text;
            p.advance();
            return Ok(TableFactor::Ext(Series { count }));
        }
        grammar::parse_table_factor(p)
    }

    fn parse_join<'a>(p: &mut Parser<'a, Self>) -> PResult<Option<Join<'a, Self>>> {
        if !p.eat_keyword("NEAR") {
            return grammar::parse_join(p);
        }
        p.expect_keyword("JOIN")?;
        let table = Self::parse_table_ref(p)?;
        p.expect_keyword("WITHIN")?;
        let distance = p.peek().text;
        p.advance();
        Ok(Some(Join {
            kind: JoinKind::Custom("NEAR JOIN"),
            spelled_out: false,
            table,
            constraint: JoinConstraint::Ext(Within { distance }),
        }))
    }

    fn lex_token(rest: &str) -> Option<LexResult> {
        rest.starts_with("??").then_some(Ok(Lexeme {
            kind: TokenKind::Op,
            len: 2,
        }))
    }

    fn infix_rule<'a>(p: &Parser<'a, Self>) -> Option<InfixRule<'a, Self>> {
        if p.at_op("??") {
            Some(InfixRule::custom(prec::ADD, parse_coalesce))
        } else {
            grammar::infix_rule(p)
        }
    }
}

fn coalesce<'a>(left: Expr<'a, Acme>, right: Expr<'a, Acme>) -> Expr<'a, Acme> {
    ExprKind::Ext(Coalesce {
        left: Box::new(left),
        right: Box::new(right),
    })
    .into()
}

#[test]
fn custom_operator_builds_a_custom_node() {
    assert_eq!(
        parse_expr::<Acme>("a ?? b").unwrap(),
        coalesce(col("a"), col("b"))
    );
}

#[test]
fn custom_operator_uses_its_declared_precedence() {
    assert_eq!(
        parse_expr::<Acme>("a ?? b * c").unwrap(),
        coalesce(col("a"), bin(col("b"), Mul, col("c")))
    );
}

#[test]
fn custom_operator_works_inside_core_clauses() {
    let select = parse_query::<Acme>("SELECT 1 FROM t LIMIT a ?? 1").unwrap();
    assert_eq!(
        select.limit.unwrap().count,
        Some(coalesce(col("a"), num("1")))
    );
}

#[test]
fn custom_operator_does_not_leak_into_other_dialects() {
    assert!(parse_expr::<Core>("a ?? b").is_err());
}

// New grammar rules must be reachable through their hooks.

use sqlparser::dialect::builtin::Builtin;

#[derive(Debug, Clone, PartialEq)]
struct Restricted;

impl Dialect for Restricted {
    type Ext = Builtin;

    fn parse_cte<'a>(p: &mut Parser<'a, Self>) -> PResult<Cte<'a, Builtin>> {
        Err(p.expected("hook:cte"))
    }

    fn parse_join<'a>(p: &mut Parser<'a, Self>) -> PResult<Option<Join<'a, Builtin>>> {
        Err(p.expected("hook:join"))
    }

    fn parse_function_call<'a>(
        p: &mut Parser<'a, Self>,
        _name: ObjectName<'a>,
    ) -> PResult<Expr<'a, Builtin>> {
        Err(p.expected("hook:function"))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct NoFromItems;

impl Dialect for NoFromItems {
    type Ext = Builtin;

    fn parse_table_with_joins<'a>(
        p: &mut Parser<'a, Self>,
    ) -> PResult<TableWithJoins<'a, Builtin>> {
        Err(p.expected("hook:from"))
    }
}

fn message_of<T: std::fmt::Debug>(result: PResult<T>) -> String {
    result.unwrap_err().message
}

#[test]
fn cte_override_is_used() {
    let err = parse_query::<Restricted>("WITH a AS (SELECT 1) SELECT 2");
    assert!(message_of(err).contains("hook:cte"));
}

#[test]
fn join_override_is_used() {
    let err = parse_query::<Restricted>("SELECT 1 FROM a JOIN b");
    assert!(message_of(err).contains("hook:join"));
}

#[test]
fn function_call_override_is_used_inside_nested_expressions() {
    let err = parse_expr::<Restricted>("1 + (f(a))");
    assert!(message_of(err).contains("hook:function"));
}

#[test]
fn from_item_override_is_used_for_every_comma_separated_item() {
    let err = parse_query::<NoFromItems>("SELECT 1 FROM a");
    assert!(message_of(err).contains("hook:from"));
}

// Query-level hooks.

use sqlparser::ast::{SetExpr, SetOperator, TableFactor};
use sqlparser::parser::SetOperatorRule;

/// Rejects any query starting with `SELECT 999`, wherever it appears.
#[derive(Debug, Clone, PartialEq)]
struct QueryHook;

impl Dialect for QueryHook {
    type Ext = Builtin;

    fn parse_query<'a>(p: &mut Parser<'a, Self>) -> PResult<Query<'a, Builtin>> {
        if p.peek_nth(1).text == "999" {
            return Err(p.expected("hook:query"));
        }
        grammar::parse_query(p)
    }
}

#[test]
fn query_override_is_used_at_every_nesting_point() {
    let cases = [
        "SELECT (SELECT 999)",
        "SELECT EXISTS (SELECT 999)",
        "SELECT a IN (SELECT 999)",
        "SELECT 1 FROM (SELECT 999) x",
        "WITH c AS (SELECT 999) SELECT 1",
        "(SELECT 999) UNION SELECT 1",
    ];
    for sql in cases {
        let message = message_of(parse_query::<QueryHook>(sql));
        assert!(message.contains("hook:query"), "{sql}: {message}");
    }
}

/// Adds Oracle-style `MINUS` as a new set operator.
#[derive(Debug, Clone, PartialEq)]
struct WithMinus;

impl Dialect for WithMinus {
    type Ext = Builtin;

    fn is_reserved(word: &str) -> bool {
        word.eq_ignore_ascii_case("MINUS") || grammar::is_reserved(word)
    }

    fn set_operator_rule<'a>(p: &Parser<'a, Self>) -> Option<SetOperatorRule> {
        if p.at_keyword("MINUS") {
            Some(SetOperatorRule {
                bp: 10,
                op: SetOperator::Except,
            })
        } else {
            grammar::set_operator_rule(p)
        }
    }
}

#[test]
fn set_operator_override_adds_a_new_operator() {
    let parsed = parse_query::<WithMinus>("SELECT 1 MINUS SELECT 2").unwrap();
    let SetExpr::SetOperation { op, .. } = parsed.body else {
        panic!("expected a set operation");
    };
    assert_eq!(op, SetOperator::Except);
}

#[test]
fn set_operator_override_keeps_the_core_operators() {
    assert!(parse_query::<WithMinus>("SELECT 1 UNION SELECT 2").is_ok());
}

#[test]
fn new_set_operator_does_not_leak_into_other_dialects() {
    assert!(parse_query::<Core>("SELECT 1 MINUS SELECT 2").is_err());
}

#[derive(Debug, Clone, PartialEq)]
struct NoDerivedTables;

impl Dialect for NoDerivedTables {
    type Ext = Builtin;

    fn parse_table_factor<'a>(p: &mut Parser<'a, Self>) -> PResult<TableFactor<'a, Builtin>> {
        if p.at_op("(") {
            return Err(p.expected("hook:factor"));
        }
        grammar::parse_table_factor(p)
    }
}

#[test]
fn table_factor_override_is_used_for_joined_tables() {
    let message = message_of(parse_query::<NoDerivedTables>(
        "SELECT 1 FROM a JOIN (SELECT 2) b ON x",
    ));
    assert!(message.contains("hook:factor"));
}

#[test]
fn huge_chain_of_custom_operators_is_rejected() {
    let sql = format!("a{}", " ?? a".repeat(1_000_000));
    assert!(parse_expr::<Acme>(&sql).is_err());
}

// Statement-level hooks.

#[test]
fn custom_statement_builds_a_custom_node() {
    let mut statements = sqlparser::parse_sql::<Acme>("PING db1").unwrap();
    assert_eq!(
        statements.remove(0),
        StatementKind::Ext(Ping { host: ident("db1") }).into()
    );
}

#[test]
fn custom_statement_mixes_with_core_statements() {
    let statements = sqlparser::parse_sql::<Acme>("PING a; SELECT 1; PING b").unwrap();
    assert_eq!(statements.len(), 3);
    assert!(matches!(statements[1].kind, StatementKind::Query(_)));
}

#[test]
fn custom_statement_does_not_leak_into_other_dialects() {
    assert!(sqlparser::parse_sql::<Core>("PING a").is_err());
}

/// Every statement hook fails with its own name.
#[derive(Debug, Clone, PartialEq)]
struct StatementHooks;

impl Dialect for StatementHooks {
    type Ext = Builtin;

    fn parse_insert<'a>(p: &mut Parser<'a, Self>) -> PResult<Insert<'a, Builtin>> {
        Err(p.expected("hook:insert"))
    }

    fn parse_update<'a>(p: &mut Parser<'a, Self>) -> PResult<Update<'a, Builtin>> {
        Err(p.expected("hook:update"))
    }

    fn parse_delete<'a>(p: &mut Parser<'a, Self>) -> PResult<Delete<'a, Builtin>> {
        Err(p.expected("hook:delete"))
    }

    fn parse_create_table<'a>(
        p: &mut Parser<'a, Self>,
        _flags: CreateFlags,
    ) -> PResult<CreateTable<'a, Builtin>> {
        Err(p.expected("hook:create"))
    }
}

/// Every hook used inside a statement fails with its own name.
#[derive(Debug, Clone, PartialEq)]
struct InnerHooks;

impl Dialect for InnerHooks {
    type Ext = Builtin;

    fn parse_assignment<'a>(p: &mut Parser<'a, Self>) -> PResult<Assignment<'a, Builtin>> {
        Err(p.expected("hook:assignment"))
    }

    fn parse_column_def<'a>(p: &mut Parser<'a, Self>) -> PResult<ColumnDef<'a, Builtin>> {
        Err(p.expected("hook:column"))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct TypeHooks;

impl Dialect for TypeHooks {
    type Ext = Builtin;

    fn parse_data_type<'a>(p: &mut Parser<'a, Self>) -> PResult<DataType<'a>> {
        Err(p.expected("hook:type"))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct OptionHooks;

impl Dialect for OptionHooks {
    type Ext = Builtin;

    fn parse_column_option<'a>(
        p: &mut Parser<'a, Self>,
    ) -> PResult<Option<ColumnOption<'a, Builtin>>> {
        Err(p.expected("hook:option"))
    }
}

fn assert_hook<T: std::fmt::Debug>(result: PResult<T>, hook: &str) {
    let message = message_of(result);
    assert!(message.contains(hook), "{hook}: {message}");
}

#[test]
fn statement_overrides_are_used() {
    use sqlparser::parse_sql as sql;
    assert_hook(
        sql::<StatementHooks>("INSERT INTO t VALUES (1)"),
        "hook:insert",
    );
    assert_hook(sql::<StatementHooks>("UPDATE t SET a = 1"), "hook:update");
    assert_hook(sql::<StatementHooks>("DELETE FROM t"), "hook:delete");
    assert_hook(
        sql::<StatementHooks>("CREATE TABLE t (a int)"),
        "hook:create",
    );
}

#[test]
fn inner_statement_overrides_are_used() {
    use sqlparser::parse_sql as sql;
    assert_hook(sql::<InnerHooks>("UPDATE t SET a = 1"), "hook:assignment");
    assert_hook(sql::<InnerHooks>("CREATE TABLE t (a int)"), "hook:column");
    assert_hook(sql::<TypeHooks>("CREATE TABLE t (a int)"), "hook:type");
    assert_hook(sql::<OptionHooks>("CREATE TABLE t (a int)"), "hook:option");
}

impl std::fmt::Display for Coalesce<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ?? {}", self.left, self.right)
    }
}

impl std::fmt::Display for Ping<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PING {}", self.host)
    }
}

#[test]
fn custom_nodes_render_through_their_own_display() {
    for sql in ["PING db1", "SELECT a ?? b", "SELECT 1 FROM t LIMIT a ?? 1"] {
        let statements = sqlparser::parse_sql::<Acme>(sql).unwrap();
        assert_eq!(statements[0].to_string(), sql);
    }
}

/// Treats `%%` as a line comment.
#[derive(Debug, Clone, PartialEq)]
struct PercentComments;

impl Dialect for PercentComments {
    type Ext = Builtin;

    fn lex_comment(rest: &str) -> Option<Result<usize, &'static str>> {
        rest.starts_with("%%")
            .then(|| Ok(rest.find('\n').unwrap_or(rest.len())))
    }
}

#[test]
fn comment_override_is_used() {
    let parsed = parse_expr::<PercentComments>("a %% note\n + b").unwrap();
    assert_eq!(parsed.to_string(), "a + b");
}

/// Each DDL hook fails with its own name.
#[derive(Debug, Clone, PartialEq)]
struct DdlHooks;

impl Dialect for DdlHooks {
    type Ext = Builtin;

    fn parse_drop<'a>(p: &mut Parser<'a, Self>) -> PResult<DropObject<'a>> {
        Err(p.expected("hook:drop"))
    }

    fn parse_truncate<'a>(p: &mut Parser<'a, Self>) -> PResult<Truncate<'a>> {
        Err(p.expected("hook:truncate"))
    }

    fn parse_alter_table<'a>(p: &mut Parser<'a, Self>) -> PResult<AlterTable<'a, Builtin>> {
        Err(p.expected("hook:alter"))
    }

    fn parse_create_view<'a>(
        p: &mut Parser<'a, Self>,
        _flags: CreateFlags,
    ) -> PResult<CreateView<'a, Builtin>> {
        Err(p.expected("hook:view"))
    }

    fn parse_create_index<'a>(p: &mut Parser<'a, Self>) -> PResult<CreateIndex<'a, Builtin>> {
        Err(p.expected("hook:index"))
    }
}

#[derive(Debug, Clone, PartialEq)]
struct AlterOpHooks;

impl Dialect for AlterOpHooks {
    type Ext = Builtin;

    fn parse_alter_table_op<'a>(p: &mut Parser<'a, Self>) -> PResult<AlterTableOp<'a, Builtin>> {
        Err(p.expected("hook:op"))
    }

    fn parse_table_constraint<'a>(
        p: &mut Parser<'a, Self>,
    ) -> PResult<Option<TableConstraint<'a, Builtin>>> {
        Err(p.expected("hook:constraint"))
    }
}

#[test]
fn ddl_overrides_are_used() {
    use sqlparser::parse_sql as sql;
    assert_hook(sql::<DdlHooks>("DROP TABLE t"), "hook:drop");
    assert_hook(sql::<DdlHooks>("TRUNCATE t"), "hook:truncate");
    assert_hook(sql::<DdlHooks>("ALTER TABLE t DROP a"), "hook:alter");
    assert_hook(sql::<DdlHooks>("CREATE VIEW v AS SELECT 1"), "hook:view");
    assert_hook(sql::<DdlHooks>("CREATE INDEX i ON t (a)"), "hook:index");
    assert_hook(sql::<AlterOpHooks>("ALTER TABLE t DROP a"), "hook:op");
    assert_hook(
        sql::<AlterOpHooks>("CREATE TABLE t (a int)"),
        "hook:constraint",
    );
}

#[test]
fn custom_table_factor_builds_a_custom_node_and_renders() {
    let query = parse_query::<Acme>("SELECT 1 FROM SERIES 5 AS s").unwrap();
    assert!(query.to_string().ends_with("FROM SERIES 5 AS s"));
    let SetExpr::Select(select) = &query.body else {
        panic!("not a select");
    };
    assert_eq!(
        select.from[0].relation.factor,
        TableFactor::Ext(Series { count: "5" })
    );
}

#[test]
fn custom_join_builds_custom_nodes_and_renders() {
    let sql = "SELECT 1 FROM a AS x NEAR JOIN b AS y WITHIN 3";
    let query = parse_query::<Acme>(sql).unwrap();
    assert_eq!(query.to_string(), sql);
    let SetExpr::Select(select) = &query.body else {
        panic!("not a select");
    };
    assert_eq!(select.from[0].joins[0].kind, JoinKind::Custom("NEAR JOIN"));
}

#[cfg(feature = "visitor")]
mod visitor {
    use std::ops::ControlFlow;

    use sqlparser::ast::{Visit, Visitor, visit_expressions, visit_relations};
    use sqlparser::parse_sql;

    use super::*;

    #[test]
    fn a_visitor_walks_a_dialect_node() {
        let statements = parse_sql::<Acme>("SELECT a ?? b FROM t JOIN SERIES 3").unwrap();
        let mut expressions = Vec::new();
        let _ = visit_expressions(&statements, |expr| {
            expressions.push(expr.to_string());
            ControlFlow::<()>::Continue(())
        });
        assert_eq!(expressions, ["a ?? b", "a", "b"]);
        let mut relations = Vec::new();
        let _ = visit_relations(&statements, |relation| {
            relations.push(relation.to_string());
            ControlFlow::<()>::Continue(())
        });
        assert_eq!(relations, ["t"]);
    }

    #[test]
    fn a_visitor_can_implement_hooks_for_a_third_party_ast() {
        struct Counter(usize);
        impl<'a> Visitor<'a, Acme> for Counter {
            type Break = ();
            fn pre_visit_expr(&mut self, _: &Expr<'a, Acme>) -> ControlFlow<()> {
                self.0 += 1;
                ControlFlow::Continue(())
            }
        }
        let statements = parse_sql::<Acme>("SELECT a ?? b").unwrap();
        let mut counter = Counter(0);
        let _ = statements.visit(&mut counter);
        assert_eq!(counter.0, 3);
    }
}
