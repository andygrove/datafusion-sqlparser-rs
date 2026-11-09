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

//! Qualified names, function calls, joins, `WITH` and `GROUP BY`.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::UnaryOp::*;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, MySql, Postgres};
use sqlparser::{parse_expr, parse_query};

type E<'a> = Expr<'a, Builtin>;

fn expr(sql: &str) -> E<'_> {
    parse_expr::<Core>(sql).unwrap()
}

fn query(sql: &str) -> Query<'_, Builtin> {
    parse_query::<Core>(sql).unwrap()
}

fn select(sql: &str) -> Select<'_, Builtin> {
    into_select(query(sql))
}

fn only_join(sql: &str) -> Join<'_, Builtin> {
    let mut from = select(sql).from;
    assert_eq!(from.len(), 1);
    let mut joins = from.remove(0).joins;
    assert_eq!(joins.len(), 1);
    joins.remove(0)
}

// Qualified names

#[test]
fn qualified_column_has_each_part() {
    assert_eq!(
        expr("t.a"),
        ExprKind::Compound(object_name(&["t", "a"])).into()
    );
}

#[test]
fn name_may_have_more_than_two_parts() {
    assert_eq!(
        expr("s.t.a"),
        ExprKind::Compound(object_name(&["s", "t", "a"])).into()
    );
}

#[test]
fn qualified_name_parts_may_be_quoted() {
    let ExprKind::Compound(name) = expr("\"T\".\"a b\"").kind else {
        panic!("expected a compound name");
    };
    assert_eq!(name.parts, vec![quoted("T", '"'), quoted("a b", '"')]);
}

#[test]
fn mysql_qualified_name_uses_backticks() {
    let ExprKind::Compound(name) = parse_expr::<MySql>("`t`.`a`").unwrap().kind else {
        panic!("expected a compound name");
    };
    assert_eq!(name.parts, vec![quoted("t", '`'), quoted("a", '`')]);
}

#[test]
fn name_cannot_end_with_a_dot() {
    assert!(parse_expr::<Core>("t.").is_err());
}

#[test]
fn qualified_name_binds_inside_operators() {
    assert_eq!(
        expr("t.a + 1"),
        bin(
            ExprKind::Compound(object_name(&["t", "a"])).into(),
            Add,
            num("1")
        )
    );
}

#[test]
fn postgres_casts_a_qualified_name() {
    let parsed = parse_expr::<Postgres>("t.a::int").unwrap();
    assert_eq!(
        parsed,
        cast(
            ExprKind::Compound(object_name(&["t", "a"])).into(),
            "int",
            CastStyle::DoubleColon
        )
    );
}

#[test]
fn table_name_may_be_qualified() {
    let parsed = select("SELECT 1 FROM s.t x");
    assert_eq!(
        parsed.from[0].relation,
        TableRef {
            factor: named(&["s", "t"]),
            alias: Some(implicit_alias("x")),
            span: Span::default()
        }
    );
}

// Function calls

#[test]
fn function_with_no_arguments() {
    assert_eq!(expr("now()"), call(&["now"], vec![]));
}

#[test]
fn function_with_expression_arguments() {
    assert_eq!(
        expr("f(a, 1 + 2)"),
        call(
            &["f"],
            vec![
                FunctionArg::Expr(col("a")),
                FunctionArg::Expr(bin(num("1"), Add, num("2")))
            ]
        )
    );
}

#[test]
fn function_with_wildcard_argument() {
    assert_eq!(
        expr("COUNT(*)"),
        call(&["COUNT"], vec![FunctionArg::Wildcard])
    );
}

#[test]
fn function_name_may_be_qualified() {
    assert_eq!(
        expr("s.f(a)"),
        call(&["s", "f"], vec![FunctionArg::Expr(col("a"))])
    );
}

#[test]
fn function_calls_nest() {
    assert_eq!(
        expr("f(g(a))"),
        call(
            &["f"],
            vec![FunctionArg::Expr(call(
                &["g"],
                vec![FunctionArg::Expr(col("a"))]
            ))]
        )
    );
}

#[test]
fn function_call_is_an_operand() {
    assert_eq!(expr("f() + 1"), bin(call(&["f"], vec![]), Add, num("1")));
}

#[test]
fn function_requires_a_closing_parenthesis() {
    assert!(parse_expr::<Core>("f(a").is_err());
}

#[test]
fn function_rejects_a_trailing_comma() {
    assert!(parse_expr::<Core>("f(a,)").is_err());
}

#[test]
fn not_before_parenthesis_is_still_an_operator() {
    assert_eq!(expr("NOT (a)"), unary(Not, parens(col("a"))));
}

// Joins

#[test]
fn join_keywords_select_the_kind() {
    let cases = [
        ("SELECT 1 FROM a JOIN b ON x", JoinKind::Inner),
        ("SELECT 1 FROM a INNER JOIN b ON x", JoinKind::Inner),
        ("SELECT 1 FROM a LEFT JOIN b ON x", JoinKind::Left),
        ("SELECT 1 FROM a LEFT OUTER JOIN b ON x", JoinKind::Left),
        ("SELECT 1 FROM a RIGHT JOIN b ON x", JoinKind::Right),
        ("SELECT 1 FROM a RIGHT OUTER JOIN b ON x", JoinKind::Right),
        ("SELECT 1 FROM a FULL JOIN b ON x", JoinKind::Full),
        ("SELECT 1 FROM a FULL OUTER JOIN b ON x", JoinKind::Full),
        ("SELECT 1 FROM a CROSS JOIN b", JoinKind::Cross),
    ];
    for (sql, kind) in cases {
        assert_eq!(only_join(sql).kind, kind, "{sql}");
    }
}

#[test]
fn join_on_keeps_the_condition() {
    let join = only_join("SELECT 1 FROM a JOIN b ON x = y");
    assert_eq!(
        join.constraint,
        JoinConstraint::On(bin(col("x"), Eq, col("y")))
    );
}

#[test]
fn join_using_keeps_every_column() {
    let join = only_join("SELECT 1 FROM a JOIN b USING (x, y)");
    assert_eq!(
        join.constraint,
        JoinConstraint::Using(vec![ident("x"), ident("y")])
    );
}

#[test]
fn cross_join_has_no_constraint() {
    let join = only_join("SELECT 1 FROM a CROSS JOIN b");
    assert_eq!(join.constraint, JoinConstraint::None);
}

#[test]
fn join_without_a_constraint_is_accepted() {
    let join = only_join("SELECT 1 FROM a JOIN b");
    assert_eq!(join.constraint, JoinConstraint::None);
}

#[test]
fn joined_table_may_have_an_alias() {
    let join = only_join("SELECT 1 FROM a JOIN b AS c ON x");
    assert_eq!(join.table.alias, Some(explicit_alias("c")));
}

#[test]
fn join_keywords_are_not_taken_as_a_table_alias() {
    let parsed = select("SELECT 1 FROM a LEFT JOIN b ON x");
    assert_eq!(parsed.from[0].relation.alias, None);
}

#[test]
fn join_constraint_keywords_are_not_taken_as_a_table_alias() {
    let on = only_join("SELECT 1 FROM a JOIN b ON x");
    assert_eq!(on.table.alias, None);
    let using = only_join("SELECT 1 FROM a JOIN b USING (x)");
    assert_eq!(using.table.alias, None);
}

#[test]
fn joins_stay_in_source_order() {
    let parsed = select("SELECT 1 FROM a JOIN b ON x LEFT JOIN c ON y");
    let joins = &parsed.from[0].joins;
    assert_eq!(joins.len(), 2);
    assert_eq!(joins[0].kind, JoinKind::Inner);
    assert_eq!(joins[0].table.factor, named(&["b"]));
    assert_eq!(joins[1].kind, JoinKind::Left);
    assert_eq!(joins[1].table.factor, named(&["c"]));
}

#[test]
fn comma_separates_from_items_and_joins_attach_to_their_item() {
    let parsed = select("SELECT 1 FROM a, b JOIN c ON x");
    assert_eq!(parsed.from.len(), 2);
    assert!(parsed.from[0].joins.is_empty());
    assert_eq!(parsed.from[1].relation.factor, named(&["b"]));
    assert_eq!(parsed.from[1].joins.len(), 1);
}

#[test]
fn left_without_join_is_rejected() {
    assert!(parse_query::<Core>("SELECT 1 FROM a LEFT b").is_err());
}

#[test]
fn using_requires_a_column() {
    assert!(parse_query::<Core>("SELECT 1 FROM a JOIN b USING ()").is_err());
}

// GROUP BY

#[test]
fn group_by_keeps_every_expression() {
    let parsed = select("SELECT 1 FROM t GROUP BY a, b + 1");
    assert_eq!(
        parsed.group_by,
        vec![col("a"), bin(col("b"), Add, num("1"))]
    );
}

#[test]
fn group_by_comes_between_where_and_order_by() {
    let parsed = query("SELECT 1 FROM t WHERE w GROUP BY g ORDER BY o");
    assert_eq!(body(&parsed).selection, Some(col("w")));
    assert_eq!(body(&parsed).group_by, vec![col("g")]);
    assert_eq!(parsed.order_by[0].expr, col("o"));
}

#[test]
fn group_requires_by() {
    assert!(parse_query::<Core>("SELECT 1 FROM t GROUP a").is_err());
}

// WITH

#[test]
fn with_attaches_the_common_table_expression() {
    let parsed = query("WITH c AS (SELECT 1) SELECT 2 FROM c");
    assert_eq!(parsed.with.len(), 1);
    assert_eq!(parsed.with[0].name, ident("c"));
    assert_eq!(parsed.with[0].query, query("SELECT 1"));
    assert_eq!(body(&parsed).from[0].relation.factor, named(&["c"]));
}

#[test]
fn with_accepts_several_common_table_expressions() {
    let parsed = query("WITH a AS (SELECT 1), b AS (SELECT 2) SELECT 3");
    let names: Vec<_> = parsed.with.iter().map(|cte| cte.name).collect();
    assert_eq!(names, vec![ident("a"), ident("b")]);
}

#[test]
fn with_nests_inside_a_common_table_expression() {
    let parsed = query("WITH a AS (WITH b AS (SELECT 1) SELECT 2) SELECT 3");
    assert_eq!(parsed.with[0].query.with[0].name, ident("b"));
}

#[test]
fn with_requires_a_following_select() {
    assert!(parse_query::<Core>("WITH a AS (SELECT 1)").is_err());
}

#[test]
fn deeply_nested_with_errors_instead_of_overflowing_the_stack() {
    let sql = format!(
        "{}SELECT 1{}",
        "WITH a AS (".repeat(100_000),
        ") SELECT 1".repeat(100_000)
    );
    assert!(parse_query::<Core>(&sql).is_err());
}

#[test]
fn moderately_nested_with_parses() {
    let sql = format!(
        "{}SELECT 1{}",
        "WITH a AS (".repeat(20),
        ") SELECT 1".repeat(20)
    );
    assert!(parse_query::<Core>(&sql).is_ok());
}

// The query from the original crate's benchmark

#[test]
fn original_with_select_benchmark_query_has_the_expected_shape() {
    let parsed = query(
        "WITH derived AS (
            SELECT MAX(a) AS max_a, COUNT(b) AS b_num, user_id
            FROM MY_TABLE
            GROUP BY user_id
        )
        SELECT * FROM my_table
        LEFT JOIN derived USING (user_id)",
    );

    let cte = &parsed.with[0];
    assert_eq!(cte.name, ident("derived"));
    assert_eq!(
        body(&cte.query).projection[0],
        SelectItemKind::Expr {
            expr: call(&["MAX"], vec![FunctionArg::Expr(col("a"))]),
            alias: Some(ident("max_a")),
        }
        .into()
    );
    assert_eq!(body(&cte.query).group_by, vec![col("user_id")]);

    assert_eq!(
        body(&parsed).projection,
        vec![SelectItemKind::Wildcard.into()]
    );
    let join = &body(&parsed).from[0].joins[0];
    assert_eq!(join.kind, JoinKind::Left);
    assert_eq!(join.table.factor, named(&["derived"]));
    assert_eq!(
        join.constraint,
        JoinConstraint::Using(vec![ident("user_id")])
    );
}

#[test]
fn cross_join_rejects_a_constraint() {
    assert!(parse_query::<Core>("SELECT 1 FROM a CROSS JOIN b ON x").is_err());
    assert!(parse_query::<Core>("SELECT 1 FROM a CROSS JOIN b USING (x)").is_err());
}
