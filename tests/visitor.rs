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

//! AST visitors, ported from the original crate's tests, plus relations in the
//! places this crate's dialects can name a table.

#![cfg(feature = "visitor")]

use sqlparser::ast::ExprKind;
use sqlparser::ast::StatementKind;
use std::ops::ControlFlow;

use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{
    BigQuery, ClickHouse, Dialect, DuckDb, Hive, MySql, Oracle, Postgres, Snowflake, Spark, Sqlite,
};
use sqlparser::parse_sql;

#[derive(Default)]
struct TestVisitor {
    visited: Vec<String>,
}

impl TestVisitor {
    fn log(&mut self, text: String) -> ControlFlow<()> {
        self.visited.push(text);
        ControlFlow::Continue(())
    }
}

impl<'a> Visitor<'a, Builtin> for TestVisitor {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("PRE: QUERY: {query}"))
    }
    fn post_visit_query(&mut self, query: &Query<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("POST: QUERY: {query}"))
    }
    fn pre_visit_select(&mut self, select: &Select<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("PRE: SELECT: {select}"))
    }
    fn post_visit_select(&mut self, select: &Select<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("POST: SELECT: {select}"))
    }
    fn pre_visit_relation(&mut self, relation: &ObjectName<'a>) -> ControlFlow<()> {
        self.log(format!("PRE: RELATION: {relation}"))
    }
    fn post_visit_relation(&mut self, relation: &ObjectName<'a>) -> ControlFlow<()> {
        self.log(format!("POST: RELATION: {relation}"))
    }
    fn pre_visit_table_factor(&mut self, factor: &TableFactor<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("PRE: TABLE FACTOR: {factor}"))
    }
    fn post_visit_table_factor(&mut self, factor: &TableFactor<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("POST: TABLE FACTOR: {factor}"))
    }
    fn pre_visit_expr(&mut self, expr: &Expr<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("PRE: EXPR: {expr}"))
    }
    fn post_visit_expr(&mut self, expr: &Expr<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("POST: EXPR: {expr}"))
    }
    fn pre_visit_statement(&mut self, statement: &Statement<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("PRE: STATEMENT: {statement}"))
    }
    fn post_visit_statement(&mut self, statement: &Statement<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("POST: STATEMENT: {statement}"))
    }
    fn pre_visit_order_by(&mut self, order_by: &[OrderByItem<'a, Builtin>]) -> ControlFlow<()> {
        let items: Vec<String> = order_by.iter().map(ToString::to_string).collect();
        self.log(format!("PRE: ORDER BY: ORDER BY {}", items.join(", ")))
    }
    fn post_visit_order_by(&mut self, order_by: &[OrderByItem<'a, Builtin>]) -> ControlFlow<()> {
        let items: Vec<String> = order_by.iter().map(ToString::to_string).collect();
        self.log(format!("POST: ORDER BY: ORDER BY {}", items.join(", ")))
    }
    fn pre_visit_order_by_expr(&mut self, item: &OrderByItem<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("PRE: ORDER BY EXPR: {item}"))
    }
    fn post_visit_order_by_expr(&mut self, item: &OrderByItem<'a, Builtin>) -> ControlFlow<()> {
        self.log(format!("POST: ORDER BY EXPR: {item}"))
    }
    fn pre_visit_group_by(&mut self, group_by: &[Expr<'a, Builtin>]) -> ControlFlow<()> {
        let items: Vec<String> = group_by.iter().map(ToString::to_string).collect();
        self.log(format!("PRE: GROUP BY: GROUP BY {}", items.join(", ")))
    }
    fn post_visit_group_by(&mut self, group_by: &[Expr<'a, Builtin>]) -> ControlFlow<()> {
        let items: Vec<String> = group_by.iter().map(ToString::to_string).collect();
        self.log(format!("POST: GROUP BY: GROUP BY {}", items.join(", ")))
    }
}

fn trace<D: Dialect<Ext = Builtin>>(sql: &str) -> Vec<String> {
    let statements = parse_sql::<D>(sql).unwrap();
    assert_eq!(statements.len(), 1, "{sql}");
    let mut visitor = TestVisitor::default();
    assert_eq!(statements.visit(&mut visitor), ControlFlow::Continue(()));
    visitor.visited
}

fn relations<D: Dialect<Ext = Builtin>>(sql: &str) -> Vec<String> {
    let statements = parse_sql::<D>(sql).unwrap();
    let mut found = Vec::new();
    let _ = visit_relations(&statements, |relation| {
        found.push(relation.to_string());
        ControlFlow::<()>::Continue(())
    });
    found
}

#[test]
fn trace_of_a_table_and_its_alias() {
    assert_eq!(
        trace::<Postgres>("SELECT * from table_name as my_table"),
        [
            "PRE: STATEMENT: SELECT * FROM table_name AS my_table",
            "PRE: QUERY: SELECT * FROM table_name AS my_table",
            "PRE: SELECT: SELECT * FROM table_name AS my_table",
            // The alias is on `TableRef`, so a factor renders without it.
            "PRE: TABLE FACTOR: table_name",
            "PRE: RELATION: table_name",
            "POST: RELATION: table_name",
            "POST: TABLE FACTOR: table_name",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM table_name AS my_table",
            "POST: QUERY: SELECT * FROM table_name AS my_table",
            "POST: STATEMENT: SELECT * FROM table_name AS my_table",
        ]
    );
}

#[test]
fn trace_of_a_join() {
    assert_eq!(
        trace::<Postgres>("SELECT * from t1 join t2 on t1.id = t2.t1_id"),
        [
            "PRE: STATEMENT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
            "PRE: QUERY: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
            "PRE: SELECT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
            "PRE: TABLE FACTOR: t1",
            "PRE: RELATION: t1",
            "POST: RELATION: t1",
            "POST: TABLE FACTOR: t1",
            "PRE: TABLE FACTOR: t2",
            "PRE: RELATION: t2",
            "POST: RELATION: t2",
            "POST: TABLE FACTOR: t2",
            "PRE: EXPR: t1.id = t2.t1_id",
            "PRE: EXPR: t1.id",
            "POST: EXPR: t1.id",
            "PRE: EXPR: t2.t1_id",
            "POST: EXPR: t2.t1_id",
            "POST: EXPR: t1.id = t2.t1_id",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
            "POST: QUERY: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
            "POST: STATEMENT: SELECT * FROM t1 JOIN t2 ON t1.id = t2.t1_id",
        ]
    );
}

#[test]
fn trace_of_a_subquery() {
    assert_eq!(
        trace::<Postgres>("SELECT * from t1 where EXISTS(SELECT column from t2)"),
        [
            "PRE: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "PRE: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "PRE: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "PRE: TABLE FACTOR: t1",
            "PRE: RELATION: t1",
            "POST: RELATION: t1",
            "POST: TABLE FACTOR: t1",
            "PRE: EXPR: EXISTS (SELECT column FROM t2)",
            "PRE: QUERY: SELECT column FROM t2",
            "PRE: SELECT: SELECT column FROM t2",
            "PRE: EXPR: column",
            "POST: EXPR: column",
            "PRE: TABLE FACTOR: t2",
            "PRE: RELATION: t2",
            "POST: RELATION: t2",
            "POST: TABLE FACTOR: t2",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT column FROM t2",
            "POST: QUERY: SELECT column FROM t2",
            "POST: EXPR: EXISTS (SELECT column FROM t2)",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "POST: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "POST: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
        ]
    );
}

#[test]
fn trace_of_a_union() {
    assert_eq!(
        trace::<Postgres>(
            "SELECT * from t1 where EXISTS(SELECT column from t2) UNION SELECT * from t3"
        ),
        [
            "PRE: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
            "PRE: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
            "PRE: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "PRE: TABLE FACTOR: t1",
            "PRE: RELATION: t1",
            "POST: RELATION: t1",
            "POST: TABLE FACTOR: t1",
            "PRE: EXPR: EXISTS (SELECT column FROM t2)",
            "PRE: QUERY: SELECT column FROM t2",
            "PRE: SELECT: SELECT column FROM t2",
            "PRE: EXPR: column",
            "POST: EXPR: column",
            "PRE: TABLE FACTOR: t2",
            "PRE: RELATION: t2",
            "POST: RELATION: t2",
            "POST: TABLE FACTOR: t2",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT column FROM t2",
            "POST: QUERY: SELECT column FROM t2",
            "POST: EXPR: EXISTS (SELECT column FROM t2)",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2)",
            "PRE: SELECT: SELECT * FROM t3",
            "PRE: TABLE FACTOR: t3",
            "PRE: RELATION: t3",
            "POST: RELATION: t3",
            "POST: TABLE FACTOR: t3",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM t3",
            "POST: QUERY: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
            "POST: STATEMENT: SELECT * FROM t1 WHERE EXISTS (SELECT column FROM t2) UNION SELECT * FROM t3",
        ]
    );
}

#[test]
fn trace_of_a_pivot() {
    let sql = concat!(
        "SELECT * FROM monthly_sales ",
        "PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ",
        "ORDER BY EMPID"
    );
    // The original's `TableFactor` renders with the pivot's alias, this
    // crate's without, since the alias is on `TableRef`.
    assert_eq!(
        trace::<Snowflake>(sql),
        [
            "PRE: STATEMENT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
            "PRE: QUERY: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
            "PRE: SELECT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d)",
            "PRE: TABLE FACTOR: monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR'))",
            "PRE: TABLE FACTOR: monthly_sales",
            "PRE: RELATION: monthly_sales",
            "POST: RELATION: monthly_sales",
            "POST: TABLE FACTOR: monthly_sales",
            "PRE: EXPR: SUM(a.amount)",
            "PRE: EXPR: a.amount",
            "POST: EXPR: a.amount",
            "POST: EXPR: SUM(a.amount)",
            "PRE: EXPR: a.MONTH",
            "POST: EXPR: a.MONTH",
            "PRE: EXPR: 'JAN'",
            "POST: EXPR: 'JAN'",
            "PRE: EXPR: 'FEB'",
            "POST: EXPR: 'FEB'",
            "PRE: EXPR: 'MAR'",
            "POST: EXPR: 'MAR'",
            "PRE: EXPR: 'APR'",
            "POST: EXPR: 'APR'",
            "POST: TABLE FACTOR: monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR'))",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d)",
            "PRE: ORDER BY: ORDER BY EMPID",
            "PRE: ORDER BY EXPR: EMPID",
            "PRE: EXPR: EMPID",
            "POST: EXPR: EMPID",
            "POST: ORDER BY EXPR: EMPID",
            "POST: ORDER BY: ORDER BY EMPID",
            "POST: QUERY: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
            "POST: STATEMENT: SELECT * FROM monthly_sales PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ORDER BY EMPID",
        ]
    );
}

#[test]
fn trace_of_show_columns() {
    assert_eq!(
        trace::<MySql>("SHOW COLUMNS FROM t1"),
        [
            "PRE: STATEMENT: SHOW COLUMNS FROM t1",
            "PRE: RELATION: t1",
            "POST: RELATION: t1",
            "POST: STATEMENT: SHOW COLUMNS FROM t1",
        ]
    );
}

#[test]
fn trace_of_order_by() {
    assert_eq!(
        trace::<Postgres>("SELECT * FROM t1 ORDER BY a DESC, b"),
        [
            "PRE: STATEMENT: SELECT * FROM t1 ORDER BY a DESC, b",
            "PRE: QUERY: SELECT * FROM t1 ORDER BY a DESC, b",
            "PRE: SELECT: SELECT * FROM t1",
            "PRE: TABLE FACTOR: t1",
            "PRE: RELATION: t1",
            "POST: RELATION: t1",
            "POST: TABLE FACTOR: t1",
            "PRE: GROUP BY: GROUP BY ",
            "POST: GROUP BY: GROUP BY ",
            "POST: SELECT: SELECT * FROM t1",
            "PRE: ORDER BY: ORDER BY a DESC, b",
            "PRE: ORDER BY EXPR: a DESC",
            "PRE: EXPR: a",
            "POST: EXPR: a",
            "POST: ORDER BY EXPR: a DESC",
            "PRE: ORDER BY EXPR: b",
            "PRE: EXPR: b",
            "POST: EXPR: b",
            "POST: ORDER BY EXPR: b",
            "POST: ORDER BY: ORDER BY a DESC, b",
            "POST: QUERY: SELECT * FROM t1 ORDER BY a DESC, b",
            "POST: STATEMENT: SELECT * FROM t1 ORDER BY a DESC, b",
        ]
    );
}

#[test]
fn trace_of_group_by() {
    assert_eq!(
        trace::<Postgres>("SELECT a FROM t GROUP BY a, b"),
        [
            "PRE: STATEMENT: SELECT a FROM t GROUP BY a, b",
            "PRE: QUERY: SELECT a FROM t GROUP BY a, b",
            "PRE: SELECT: SELECT a FROM t GROUP BY a, b",
            "PRE: EXPR: a",
            "POST: EXPR: a",
            "PRE: TABLE FACTOR: t",
            "PRE: RELATION: t",
            "POST: RELATION: t",
            "POST: TABLE FACTOR: t",
            "PRE: GROUP BY: GROUP BY a, b",
            "PRE: EXPR: a",
            "POST: EXPR: a",
            "PRE: EXPR: b",
            "POST: EXPR: b",
            "POST: GROUP BY: GROUP BY a, b",
            "POST: SELECT: SELECT a FROM t GROUP BY a, b",
            "POST: QUERY: SELECT a FROM t GROUP BY a, b",
            "POST: STATEMENT: SELECT a FROM t GROUP BY a, b",
        ]
    );
}

struct Noop;

impl<'a> Visitor<'a, Builtin> for Noop {
    type Break = ();
}

#[test]
fn walking_a_long_chain_of_or() {
    let cond = (0..1000)
        .map(|n| format!("x = {n}"))
        .collect::<Vec<_>>()
        .join(" OR ");
    let sql = format!("SELECT x WHERE {cond}");
    let statements = parse_sql::<Postgres>(&sql).unwrap();
    assert_eq!(statements.visit(&mut Noop), ControlFlow::Continue(()));
}

#[derive(Default)]
struct IdentVisitor {
    idents: Vec<String>,
}

impl<'a> Visitor<'a, Builtin> for IdentVisitor {
    type Break = ();

    fn pre_visit_ident(&mut self, ident: &Ident<'a>) -> ControlFlow<()> {
        self.idents.push(ident.text.to_string());
        ControlFlow::Continue(())
    }
}

#[test]
fn identifiers_are_visited_in_order() {
    let statements = parse_sql::<Postgres>("SELECT a, b FROM t").unwrap();
    let mut visitor = IdentVisitor::default();
    let _ = statements.visit(&mut visitor);
    assert_eq!(visitor.idents, ["a", "b", "t"]);
}

fn expressions_of<D: Dialect<Ext = Builtin>>(sql: &str) -> Vec<String> {
    let statements = parse_sql::<D>(sql).unwrap();
    let mut visited = Vec::new();
    let _ = visit_expressions(&statements, |expr| {
        visited.push(expr.to_string());
        ControlFlow::<()>::Continue(())
    });
    visited
}

#[test]
fn function_clauses_are_visited_in_source_order() {
    let statements = parse_sql::<Postgres>(
        "SELECT LISTAGG(value) WITHIN GROUP (ORDER BY order_key) FILTER (WHERE filter_key)",
    )
    .unwrap();
    let mut idents = Vec::new();
    let _ = visit_expressions(&statements, |expr| {
        if let ExprKind::Ident(ident) = &expr.kind {
            idents.push(ident.text.to_string());
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(idents, ["value", "order_key", "filter_key"]);
}

#[test]
fn case_branches_are_visited_in_order() {
    assert_eq!(
        expressions_of::<Postgres>("SELECT CASE a WHEN 1 THEN 2 WHEN 3 THEN 4 ELSE 5 END"),
        [
            "CASE a WHEN 1 THEN 2 WHEN 3 THEN 4 ELSE 5 END",
            "a",
            "1",
            "2",
            "3",
            "4",
            "5"
        ]
    );
}

#[test]
fn a_call_nested_in_connect_by_is_visited_once() {
    // The original reparses `PRIOR` as a name here; Oracle keeps it an operator
    // inside `CONNECT BY`, so this names the column plainly.
    let sql = "SELECT * FROM t CONNECT BY x IN (((SELECT POSITION(a IN (z)))), 1)";
    let mut positions = 0;
    let statements = parse_sql::<Oracle>(sql).unwrap();
    let _ = visit_expressions(&statements, |expr| {
        if let ExprKind::Special(special) = &expr.kind
            && matches!(**special, SpecialExpr::Position { .. })
        {
            positions += 1;
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(positions, 1);
}

#[test]
fn create_view_name_is_a_relation() {
    assert_eq!(
        relations::<Postgres>("CREATE VIEW db1.v AS SELECT * FROM t"),
        ["db1.v", "t"]
    );
}

#[test]
fn in_table_target_is_a_relation() {
    assert_eq!(
        relations::<Sqlite>("SELECT a IN lookup FROM source"),
        ["lookup", "source"]
    );
}

#[test]
fn in_table_target_can_be_renamed() {
    let renamed = String::from("renamed");
    let mut statements = parse_sql::<Sqlite>("SELECT a IN lookup FROM source").unwrap();
    let _ = visit_relations_mut(&mut statements, |relation| {
        if relation.to_string() == "lookup" {
            relation.parts[0].text = renamed.as_str();
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(statements[0].to_string(), "SELECT a IN renamed FROM source");
}

#[test]
fn foreign_table_partition_of_is_a_relation() {
    assert_eq!(
        relations::<Postgres>(
            "CREATE FOREIGN TABLE part PARTITION OF public.root DEFAULT SERVER s"
        ),
        ["part", "public.root"]
    );
}

#[test]
fn value_visits_can_redact_literals() {
    let secrets = (1..=4).map(|i| format!("REDACTED_{i}")).collect::<Vec<_>>();
    let sql = concat!(
        "SELECT * FROM monthly_sales ",
        "PIVOT(SUM(a.amount) FOR a.MONTH IN ('JAN', 'FEB', 'MAR', 'APR')) AS p (c, d) ",
        "ORDER BY EMPID"
    );
    let mut statements = parse_sql::<Snowflake>(sql).unwrap();

    struct Redactor<'r> {
        secrets: &'r [String],
        index: usize,
    }

    impl<'a> VisitorMut<'a, Builtin> for Redactor<'a> {
        type Break = ();

        fn pre_visit_value(&mut self, value: &mut Literal<'a>) -> ControlFlow<()> {
            if let Literal::String(string) = value {
                string.text = &self.secrets[self.index];
                self.index += 1;
            }
            ControlFlow::Continue(())
        }
    }

    let mut redactor = Redactor {
        secrets: &secrets,
        index: 0,
    };
    let _ = VisitMut::visit(&mut statements, &mut redactor);
    assert_eq!(
        statements[0].to_string(),
        concat!(
            "SELECT * FROM monthly_sales ",
            "PIVOT(SUM(a.amount) FOR a.MONTH IN ('REDACTED_1', 'REDACTED_2', 'REDACTED_3', 'REDACTED_4')) AS p (c, d) ",
            "ORDER BY EMPID"
        )
    );
}

#[test]
fn identifiers_can_be_rewritten() {
    // `to_uppercase` allocates inside the hook in the original; here the text
    // must come from data that outlives the tree.
    let upper = ["A".to_string(), "B".to_string(), "T".to_string()];
    let mut statements = parse_sql::<Postgres>("SELECT a, b FROM t").unwrap();

    struct Upper<'r>(&'r [String]);

    impl<'a> VisitorMut<'a, Builtin> for Upper<'a> {
        type Break = ();

        fn pre_visit_ident(&mut self, ident: &mut Ident<'a>) -> ControlFlow<()> {
            if let Some(found) = self.0.iter().find(|s| s.eq_ignore_ascii_case(ident.text)) {
                ident.text = found;
            }
            ControlFlow::Continue(())
        }
    }

    let _ = VisitMut::visit(&mut statements, &mut Upper(&upper));
    assert_eq!(statements[0].to_string(), "SELECT A, B FROM T");
}

#[test]
fn a_replacement_lives_in_data_that_outlives_the_tree() {
    let replacement = String::from("bar");
    let mut statements = parse_sql::<Postgres>("SELECT a FROM foo").unwrap();
    let _ = visit_relations_mut(&mut statements, |table| {
        table.parts[0].text = replacement.as_str();
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(statements[0].to_string(), "SELECT a FROM bar");
}

#[test]
fn expressions_can_be_replaced() {
    let mut statements =
        parse_sql::<Postgres>("SELECT (SELECT y FROM z LIMIT 9) FROM t LIMIT 3").unwrap();
    let _ = visit_expressions_mut(&mut statements, |expr| {
        if let ExprKind::Subquery(query) = &mut expr.kind {
            query.limit = None;
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        statements[0].to_string(),
        "SELECT (SELECT y FROM z) FROM t LIMIT 3"
    );
}

#[test]
fn statements_are_visited_and_mutated() {
    let sql = "SELECT a FROM foo WHERE x IN (SELECT y FROM bar); CREATE TABLE baz (q INT)";
    let mut statements = parse_sql::<Postgres>(sql).unwrap();
    let mut visited = Vec::new();
    let _ = visit_statements(&statements, |statement| {
        visited.push(statement.to_string());
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        visited,
        [
            "SELECT a FROM foo WHERE x IN (SELECT y FROM bar)",
            "CREATE TABLE baz (q INT)"
        ]
    );
    let _ = visit_statements_mut(&mut statements, |statement| {
        if let StatementKind::Query(query) = &mut statement.kind {
            query.limit = Some(Box::new(Limit {
                count: Some(ExprKind::Literal(Literal::Number("1")).into()),
                offset: None,
                offset_rows: OffsetRows::None,
                offset_comma: false,
                all: false,
                by: Vec::new(),
            }));
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        statements[0].to_string(),
        "SELECT a FROM foo WHERE x IN (SELECT y FROM bar) LIMIT 1"
    );
}

#[test]
fn a_break_stops_the_walk() {
    let statements = parse_sql::<Postgres>("SELECT 1, 2, 3").unwrap();
    let mut visited = Vec::new();
    let flow = visit_expressions(&statements, |expr| {
        visited.push(expr.to_string());
        if visited.len() == 2 {
            ControlFlow::Break("stop")
        } else {
            ControlFlow::Continue(())
        }
    });
    assert_eq!(flow, ControlFlow::Break("stop"));
    assert_eq!(visited, ["1", "2"]);
}

// Relations in the places dialects name a table

#[test]
fn time_travel_and_versioned_tables() {
    assert_eq!(
        relations::<sqlparser::dialect::Databricks>("SELECT 1 FROM t VERSION AS OF 2"),
        ["t"]
    );
    assert_eq!(
        relations::<Snowflake>("SELECT 1 FROM t AT(TIMESTAMP => '2020-01-01')"),
        ["t"]
    );
}

#[test]
fn dml_targets() {
    assert_eq!(
        relations::<Postgres>("INSERT INTO a SELECT * FROM b"),
        ["a", "b"]
    );
    assert_eq!(
        relations::<Postgres>("UPDATE a SET x = 1 FROM b WHERE a.id = b.id"),
        ["a", "b"]
    );
    assert_eq!(
        relations::<Postgres>("DELETE FROM a USING b WHERE a.id = b.id"),
        ["a", "b"]
    );
    assert_eq!(
        relations::<Snowflake>("MERGE INTO a USING b ON a.id = b.id WHEN MATCHED THEN DELETE"),
        ["a", "b"]
    );
}

#[test]
fn multi_table_delete_reports_targets_and_sources_once() {
    assert_eq!(
        relations::<MySql>("DELETE a, b FROM a JOIN b ON a.x = b.x WHERE a.y = 1"),
        ["a", "b", "a", "b"]
    );
}

#[test]
fn table_statements_report_their_targets() {
    assert_eq!(relations::<Postgres>("CREATE TABLE t (a INT)"), ["t"]);
    assert_eq!(
        relations::<Postgres>("CREATE TABLE t (a INT REFERENCES u)"),
        ["t", "u"]
    );
    assert_eq!(relations::<Postgres>("CREATE INDEX i ON t (a)"), ["t"]);
    assert_eq!(relations::<Postgres>("DROP TABLE a, b"), ["a", "b"]);
    assert_eq!(relations::<Postgres>("DROP VIEW v"), ["v"]);
    assert_eq!(relations::<Postgres>("DROP INDEX i"), Vec::<String>::new());
    assert_eq!(relations::<Postgres>("DROP SCHEMA s"), Vec::<String>::new());
    assert_eq!(
        relations::<Postgres>("ALTER TABLE t ADD COLUMN c INT"),
        ["t"]
    );
    assert_eq!(
        relations::<Postgres>("ALTER TABLE t RENAME TO u"),
        ["t", "u"]
    );
    assert_eq!(relations::<Postgres>("TRUNCATE TABLE a, b"), ["a", "b"]);
}

#[test]
fn explain_of_a_table() {
    assert_eq!(relations::<MySql>("DESCRIBE t"), ["t"]);
}

#[test]
fn show_tables_in_a_schema_names_no_table() {
    assert_eq!(
        relations::<MySql>("SHOW TABLES FROM db"),
        Vec::<String>::new()
    );
    assert_eq!(relations::<MySql>("SHOW CREATE TABLE t"), ["t"]);
}

#[test]
fn dialect_statements_report_tables() {
    assert_eq!(
        relations::<MySql>("RENAME TABLE a TO b, c TO d"),
        ["a", "b", "c", "d"]
    );
    assert_eq!(relations::<MySql>("LOCK TABLES t READ"), ["t"]);
    assert_eq!(relations::<MySql>("TRUNCATE t"), ["t"]);
    assert_eq!(relations::<Snowflake>("COPY INTO t FROM @s"), ["t"]);
    assert_eq!(relations::<Snowflake>("CREATE TABLE a CLONE b"), ["a", "b"]);
    assert_eq!(relations::<Spark>("CACHE TABLE t"), ["t"]);
    assert_eq!(relations::<Spark>("UNCACHE TABLE t"), ["t"]);
    assert_eq!(
        relations::<Hive>("ANALYZE TABLE t COMPUTE STATISTICS"),
        ["t"]
    );
    assert_eq!(
        relations::<Sqlite>("CREATE TRIGGER tr INSERT ON x BEGIN SELECT 1; END"),
        ["x"]
    );
    assert_eq!(
        relations::<BigQuery>("CREATE SNAPSHOT TABLE a CLONE b"),
        ["a", "b"]
    );
    assert_eq!(relations::<ClickHouse>("OPTIMIZE TABLE t"), ["t"]);
    assert_eq!(relations::<Postgres>("COPY t FROM 'f'"), ["t"]);
    assert_eq!(
        relations::<Postgres>("CREATE RULE r AS ON UPDATE TO t DO SELECT 1"),
        ["t"]
    );
    assert_eq!(
        relations::<Postgres>(
            "CREATE TRIGGER tr BEFORE INSERT ON t FOR EACH ROW EXECUTE FUNCTION f()"
        ),
        ["t"]
    );
    assert_eq!(relations::<Postgres>("CREATE POLICY p ON t FOR ALL"), ["t"]);
    assert_eq!(relations::<DuckDb>("SELECT * FROM t"), ["t"]);
}

#[test]
fn every_dialect_is_walked_without_panicking() {
    let statements = parse_sql::<Oracle>("SELECT * FROM t").unwrap();
    assert_eq!(statements.visit(&mut Noop), ControlFlow::Continue(()));
}

// Third-party dialects are covered in tests/custom_dialect.rs.
