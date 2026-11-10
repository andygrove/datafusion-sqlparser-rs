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

//! Parsed trees must be safe to use: untrusted SQL cannot be allowed to build
//! a tree so deep that dropping, cloning or comparing it overflows the stack.
//!
//! Rust runs each test on a thread with a 2 MiB stack, and debug builds use
//! the most stack, so passing here is the strictest check of the default limit.

mod common;

use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, MySql, Postgres, Snowflake, Spark};
use sqlparser::parser::Parser;
use sqlparser::{parse_expr, parse_query};

const HUGE: usize = 1_000_000;

fn chain(terms: usize) -> String {
    format!("{}1", "1+".repeat(terms))
}

fn exercise(tree: &Expr<'_, Builtin>) {
    let copy = tree.clone();
    assert!(copy == *tree);
    assert!(!format!("{tree:?}").is_empty());
}

// Rejected instead of aborting the process

#[test]
fn huge_operator_chain_is_rejected() {
    assert!(parse_expr::<Core>(&chain(HUGE)).is_err());
}

#[test]
fn huge_union_chain_is_rejected() {
    let sql = format!("{}SELECT 1", "SELECT 1 UNION ALL ".repeat(HUGE));
    assert!(parse_query::<Core>(&sql).is_err());
}

#[test]
fn huge_intersect_chain_on_the_right_of_a_union_is_rejected() {
    let sql = format!(
        "SELECT 1 UNION {}SELECT 1",
        "SELECT 1 INTERSECT ".repeat(HUGE)
    );
    assert!(parse_query::<Core>(&sql).is_err());
}

#[test]
fn huge_chain_of_dialect_operators_is_rejected() {
    let sql = format!("1{}", " DIV 1".repeat(HUGE));
    assert!(parse_expr::<MySql>(&sql).is_err());
}

#[test]
fn huge_cast_chain_is_rejected() {
    let sql = format!("x{}", "::int".repeat(HUGE));
    assert!(parse_expr::<Postgres>(&sql).is_err());
}

#[test]
fn chains_nested_in_parentheses_add_up() {
    // Each level is shallow, but the tree is 100 * 100 levels tall.
    let sql = format!(
        "{}1{}",
        "(".repeat(100),
        format!("{})", "+1".repeat(100)).repeat(100)
    );
    assert!(parse_expr::<Core>(&sql).is_err());
}

// Legitimate wide queries are unaffected

#[test]
fn separate_expressions_do_not_add_up() {
    let item = chain(100);
    let items = vec![item; 100].join(", ");
    assert!(parse_query::<Core>(&format!("SELECT {items}")).is_ok());
}

#[test]
fn many_columns_do_not_count_as_height() {
    let items = vec!["a + b"; 10_000].join(", ");
    assert!(parse_query::<Core>(&format!("SELECT {items} FROM t")).is_ok());
}

#[test]
fn many_joins_do_not_count_as_height() {
    let sql = format!("SELECT 1 FROM t{}", " CROSS JOIN u".repeat(10_000));
    assert!(parse_query::<Core>(&sql).is_ok());
}

#[test]
fn separate_intersect_runs_do_not_add_up() {
    let run = format!("{}SELECT 1", "SELECT 1 INTERSECT ".repeat(1000));
    let sql = vec![run; 5].join(" UNION ");
    assert!(parse_query::<Core>(&sql).is_ok());
}

// The limit itself

#[test]
fn tall_chain_within_the_limit_is_safe_to_use() {
    let sql = chain(1190);
    exercise(&parse_expr::<Core>(&sql).unwrap());
}

#[test]
fn chain_past_the_limit_is_rejected() {
    assert!(parse_expr::<Core>(&chain(1300)).is_err());
}

#[test]
fn tall_tree_of_nested_queries_and_chains_is_safe_to_use() {
    let sql = format!("{}{}{}", "(SELECT ".repeat(30), chain(960), ")".repeat(30));
    exercise(&parse_expr::<Core>(&sql).unwrap());
}

#[test]
fn limit_can_be_raised() {
    let sql = chain(3000);
    let mut parser = Parser::<Core>::new(&sql)
        .unwrap()
        .with_max_tree_height(5000);
    assert!(parser.parse_expr().is_ok());
}

#[test]
fn limit_can_be_lowered() {
    let sql = chain(20);
    let mut parser = Parser::<Core>::new(&sql).unwrap().with_max_tree_height(10);
    assert!(parser.parse_expr().is_err());
}

#[test]
fn rejected_chain_reports_where_the_limit_was_reached() {
    let err = parse_expr::<Core>(&chain(HUGE)).unwrap_err();
    assert!(err.span.start > 0);
}

#[test]
fn tallest_chains_of_every_shape_are_safe_to_use() {
    let cast = format!("x{}", "::int".repeat(590));
    exercise(&parse_expr::<Postgres>(&cast).unwrap());
    let div = format!("1{}", " DIV 1".repeat(590));
    exercise(&parse_expr::<MySql>(&div).unwrap());
    let in_chain = format!("1{}", " IN (1)".repeat(590));
    exercise(&parse_expr::<Core>(&in_chain).unwrap());
}

#[test]
fn tallest_union_chain_is_safe_to_use() {
    let sql = format!("{}SELECT 1", "SELECT 1 UNION ALL ".repeat(1190));
    let query = parse_query::<Core>(&sql).unwrap();
    let copy = query.clone();
    assert!(copy == query);
    assert!(!format!("{query:?}").is_empty());
}

#[test]
fn custom_operators_count_as_two_levels() {
    let within = format!("1{}", " DIV 1".repeat(590));
    assert!(parse_expr::<MySql>(&within).is_ok());
    let past = format!("1{}", " DIV 1".repeat(640));
    assert!(parse_expr::<MySql>(&past).is_err());
}

// Clauses that wrap the table before them, so a long run builds a tall tree

const TABLE_CLAUSE_CHAINS: [&str; 3] = [
    " TABLESAMPLE (1)",
    " PIVOT(SUM(a) FOR b IN ('x'))",
    " UNPIVOT(a FOR b IN (c))",
];

#[test]
fn huge_chains_of_table_clauses_are_rejected_in_snowflake() {
    for clause in TABLE_CLAUSE_CHAINS {
        let sql = format!("SELECT * FROM t{}", clause.repeat(200_000));
        assert!(parse_query::<Snowflake>(&sql).is_err(), "{clause}");
    }
}

#[test]
fn huge_chains_of_table_clauses_are_rejected_in_spark() {
    for clause in TABLE_CLAUSE_CHAINS {
        let sql = format!("SELECT * FROM t{}", clause.repeat(200_000));
        assert!(parse_query::<Spark>(&sql).is_err(), "{clause}");
    }
}

#[test]
fn nesting_limit_can_be_changed() {
    let parse = |depth: usize, limit: usize| {
        let sql = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
        Parser::<Core>::new(&sql)
            .unwrap()
            .with_max_nesting_depth(limit)
            .parse_expr()
            .map(|_| ())
    };
    let error = parse(5, 4).unwrap_err();
    assert_eq!(error.message, "SQL is nested too deeply (the limit is 4)");
    assert!(parse(5, 6).is_ok());
    assert!(parse(100, 64).is_err());
    assert!(parse(100, 110).is_ok());
}
