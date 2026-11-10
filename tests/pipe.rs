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

//! Pipe syntax: `SELECT * FROM t |> WHERE x |> SELECT y`.

use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Snowflake, Spark};
use sqlparser::parse_sql;

fn renders(sql: &str) -> String {
    let statements = parse_sql::<Spark>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips(sql: &str) {
    assert_eq!(renders(sql), sql);
}

fn rejected<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

#[test]
fn projection_operators_round_trip() {
    round_trips("SELECT * FROM t |> SELECT a, b AS c");
    round_trips("SELECT * FROM t |> EXTEND a + 1 AS b, c");
    round_trips("SELECT * FROM t |> SET a = a + 1, b = 2");
    round_trips("SELECT * FROM t |> DROP a, b");
    round_trips("SELECT * FROM t |> AS u");
    round_trips("SELECT * FROM t |> RENAME a AS b, c AS d");
}

#[test]
fn bare_names_render_with_as() {
    assert_eq!(
        renders("SELECT * FROM t |> SELECT a b"),
        "SELECT * FROM t |> SELECT a AS b"
    );
    assert_eq!(
        renders("SELECT * FROM t |> RENAME a b"),
        "SELECT * FROM t |> RENAME a AS b"
    );
    assert_eq!(
        renders("SELECT * FROM t |> CALL f() x"),
        "SELECT * FROM t |> CALL f() AS x"
    );
}

#[test]
fn rename_needs_a_new_name() {
    assert!(rejected::<Spark>("SELECT * FROM t |> RENAME a"));
    assert!(rejected::<Spark>("SELECT * FROM t |> RENAME a AS"));
}

#[test]
fn filter_sort_and_limit_round_trip() {
    round_trips("SELECT * FROM t |> WHERE a = 1 OR b = 2");
    round_trips("SELECT * FROM t |> ORDER BY a DESC, b ASC");
    round_trips("SELECT * FROM t |> LIMIT 10");
    round_trips("SELECT * FROM t |> LIMIT 10 OFFSET 5");
}

#[test]
fn order_needs_by() {
    assert!(rejected::<Spark>("SELECT * FROM t |> ORDER a"));
}

#[test]
fn aggregate_round_trips() {
    round_trips("SELECT * FROM t |> AGGREGATE COUNT(*)");
    round_trips("SELECT * FROM t |> AGGREGATE COUNT(*) AS c, MIN(a)");
    round_trips("SELECT * FROM t |> AGGREGATE GROUP BY a, b");
    round_trips("SELECT * FROM t |> AGGREGATE SUM(c) GROUP BY a, b");
    round_trips("SELECT * FROM t |> AGGREGATE SUM(c) AS s GROUP BY a AS x DESC");
}

#[test]
fn aggregate_sort_words_are_not_aliases() {
    round_trips("SELECT * FROM t |> AGGREGATE SUM(c) ASC");
    round_trips("SELECT * FROM t |> AGGREGATE SUM(c) DESC GROUP BY a ASC");
    assert_eq!(
        renders("SELECT * FROM t |> AGGREGATE COUNT(*) c"),
        "SELECT * FROM t |> AGGREGATE COUNT(*) AS c"
    );
}

#[test]
fn aggregate_needs_something_to_compute() {
    assert!(rejected::<Spark>("SELECT * FROM t |> AGGREGATE"));
    assert!(rejected::<Spark>("SELECT * FROM t |> AGGREGATE GROUP a"));
}

#[test]
fn union_quantifiers_round_trip() {
    round_trips("SELECT * FROM t |> UNION (SELECT 1)");
    round_trips("SELECT * FROM t |> UNION ALL (SELECT 1), (SELECT 2)");
    round_trips("SELECT * FROM t |> UNION DISTINCT (SELECT 1)");
    round_trips("SELECT * FROM t |> UNION BY NAME (SELECT 1)");
    round_trips("SELECT * FROM t |> UNION ALL BY NAME (SELECT 1)");
    round_trips("SELECT * FROM t |> UNION DISTINCT BY NAME (SELECT 1)");
}

#[test]
fn intersect_and_except_need_distinct() {
    round_trips("SELECT * FROM t |> INTERSECT DISTINCT (SELECT 1), (SELECT 2)");
    round_trips("SELECT * FROM t |> EXCEPT DISTINCT BY NAME (SELECT 1)");
    assert!(rejected::<Spark>("SELECT * FROM t |> INTERSECT (SELECT 1)"));
    assert!(rejected::<Spark>("SELECT * FROM t |> EXCEPT (SELECT 1)"));
    assert!(rejected::<Spark>(
        "SELECT * FROM t |> EXCEPT ALL (SELECT 1)"
    ));
    assert!(rejected::<Spark>(
        "SELECT * FROM t |> INTERSECT BY NAME (SELECT 1)"
    ));
}

#[test]
fn set_operands_need_parentheses() {
    assert!(rejected::<Spark>("SELECT * FROM t |> UNION SELECT 1"));
    assert!(rejected::<Spark>("SELECT * FROM t |> UNION BY (SELECT 1)"));
}

#[test]
fn call_round_trips() {
    round_trips("SELECT * FROM t |> CALL f()");
    round_trips("SELECT * FROM t |> CALL s.f(a + b, 'x') AS r");
    assert!(rejected::<Spark>("SELECT * FROM t |> CALL f"));
}

#[test]
fn pivot_round_trips() {
    round_trips("SELECT * FROM t |> PIVOT(SUM(a) FOR b IN ('x', 'y'))");
    round_trips("SELECT * FROM t |> PIVOT(SUM(a) AS s, COUNT(*) AS c FOR d.e IN ('x')) AS p");
    assert_eq!(
        renders("SELECT * FROM t |> PIVOT(SUM(a) FOR b IN ('x')) p"),
        "SELECT * FROM t |> PIVOT(SUM(a) FOR b IN ('x')) AS p"
    );
}

#[test]
fn unpivot_round_trips() {
    round_trips("SELECT * FROM t |> UNPIVOT(a FOR b IN (c, d))");
    round_trips("SELECT * FROM t |> UNPIVOT(a FOR b IN (c, d)) AS u");
    assert_eq!(
        renders("SELECT * FROM t |> UNPIVOT(a FOR b IN (c)) u"),
        "SELECT * FROM t |> UNPIVOT(a FOR b IN (c)) AS u"
    );
}

#[test]
fn pivot_needs_its_closing_parenthesis() {
    assert!(rejected::<Spark>(
        "SELECT * FROM t |> PIVOT(SUM(a) FOR b IN ('x')"
    ));
    assert!(rejected::<Spark>(
        "SELECT * FROM t |> UNPIVOT(a FOR b IN (c)"
    ));
}

#[test]
fn table_sample_round_trips() {
    round_trips("SELECT * FROM t |> TABLESAMPLE (50)");
    round_trips("SELECT * FROM t |> TABLESAMPLE (50 PERCENT)");
}

#[test]
fn joins_round_trip() {
    round_trips("SELECT * FROM t |> JOIN u ON t.a = u.a");
    round_trips("SELECT * FROM t |> LEFT JOIN u AS v USING(a, b)");
    round_trips("SELECT * FROM t |> INNER JOIN u ON a = b");
    round_trips("SELECT * FROM t |> RIGHT JOIN u ON a = b");
    round_trips("SELECT * FROM t |> FULL JOIN u ON a = b");
    round_trips("SELECT * FROM t |> CROSS JOIN u");
}

#[test]
fn a_join_operator_holds_one_join() {
    assert!(rejected::<Spark>(
        "SELECT * FROM t |> JOIN u ON a = b JOIN v ON c = d"
    ));
}

#[test]
fn operators_chain() {
    round_trips("SELECT * FROM t |> WHERE a > 1 |> SELECT a AS b |> ORDER BY b");
    round_trips("SELECT * FROM t |> LIMIT 10 |> LIMIT 5");
    round_trips("SELECT * FROM t |> JOIN u ON a = b |> JOIN v ON c = d");
}

#[test]
fn pipes_follow_the_whole_query() {
    round_trips("SELECT a FROM t WHERE a > 1 ORDER BY a LIMIT 3 |> WHERE a < 2");
    round_trips("WITH c AS (SELECT 1) SELECT * FROM c |> SELECT a");
    round_trips("SELECT 1 UNION SELECT 2 |> WHERE a");
    round_trips("SELECT * FROM (SELECT * FROM t |> WHERE a) AS s |> LIMIT 1");
    round_trips("SELECT a IN (SELECT b FROM u |> LIMIT 1) FROM t");
}

#[test]
fn an_unknown_operator_is_rejected() {
    assert!(rejected::<Spark>("SELECT * FROM t |> FROB a"));
    assert!(rejected::<Spark>("SELECT * FROM t |>"));
    assert!(rejected::<Spark>("SELECT * FROM t |> "));
}

#[test]
fn a_pipe_is_not_an_expression_operator() {
    assert!(rejected::<Spark>("SELECT a |> b"));
}

#[test]
fn only_dialects_with_pipes_accept_them() {
    let sql = "SELECT * FROM t |> WHERE a";
    assert!(rejected::<Core>(sql));
    assert!(rejected::<MySql>(sql));
    assert!(rejected::<Postgres>(sql));
    assert!(rejected::<Snowflake>(sql));
    assert!(!rejected::<Spark>(sql));
}

#[test]
fn spark_keeps_bitwise_or() {
    assert_eq!(renders("SELECT a | b"), "SELECT a | b");
}

#[test]
fn snowflake_tables_still_pivot_and_sample() {
    for sql in [
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x') DEFAULT ON NULL (0))",
        "SELECT * FROM t UNPIVOT INCLUDE NULLS (a FOR b IN (c, d))",
        "SELECT * FROM t TABLESAMPLE (10 PERCENT)",
        "SELECT * FROM t SAMPLE (10 ROWS)",
    ] {
        let statements = parse_sql::<Snowflake>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
        assert_eq!(statements[0].to_string(), sql);
    }
}

mod shape {
    use sqlparser::ast::*;
    use sqlparser::dialect::Spark;
    use sqlparser::dialect::builtin::Builtin;
    use sqlparser::parse_query;

    fn pipes(sql: &'static str) -> Vec<PipeOperator<'static, Builtin>> {
        parse_query::<Spark>(sql)
            .unwrap_or_else(|e| panic!("{sql}: {e:?}"))
            .pipes
    }

    fn only(sql: &'static str) -> PipeOperator<'static, Builtin> {
        let mut pipes = pipes(sql);
        assert_eq!(pipes.len(), 1, "{sql}");
        pipes.remove(0)
    }

    fn text(expr: &Expr<'_, Builtin>) -> String {
        expr.to_string()
    }

    #[test]
    fn a_query_without_pipes_has_none() {
        assert!(pipes("SELECT * FROM t").is_empty());
    }

    #[test]
    fn steps_keep_their_order() {
        let pipes = pipes("SELECT * FROM t |> WHERE a |> AS u |> DROP b, c");
        assert_eq!(pipes.len(), 3);
        assert!(matches!(&pipes[0], PipeOperator::Where(e) if text(e) == "a"));
        assert!(matches!(&pipes[1], PipeOperator::As(name) if name.text == "u"));
        assert!(
            matches!(&pipes[2], PipeOperator::Drop(names) if names.len() == 2 && names[1].text == "c")
        );
    }

    #[test]
    fn pipes_belong_to_the_innermost_query() {
        let query = parse_query::<Spark>("SELECT * FROM (SELECT 1 |> WHERE a) AS s").unwrap();
        assert!(query.pipes.is_empty());
    }

    #[test]
    fn select_and_extend_are_distinct() {
        assert!(matches!(
            only("SELECT * FROM t |> SELECT a"),
            PipeOperator::Select(_)
        ));
        assert!(matches!(
            only("SELECT * FROM t |> EXTEND a"),
            PipeOperator::Extend(_)
        ));
    }

    #[test]
    fn limit_keeps_count_and_offset_apart() {
        match only("SELECT * FROM t |> LIMIT 10 OFFSET 5") {
            PipeOperator::Limit { count, offset } => {
                assert_eq!(text(&count), "10");
                assert_eq!(text(&offset.unwrap()), "5");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            only("SELECT * FROM t |> LIMIT 10"),
            PipeOperator::Limit { offset: None, .. }
        ));
    }

    #[test]
    fn aggregate_keeps_items_and_groups_apart() {
        match only("SELECT * FROM t |> AGGREGATE SUM(c) AS s ASC, d GROUP BY a AS x DESC") {
            PipeOperator::Aggregate {
                aggregates,
                group_by,
            } => {
                assert_eq!(aggregates.len(), 2);
                assert_eq!(text(&aggregates[0].expr.expr), "SUM(c)");
                assert_eq!(aggregates[0].expr.alias.as_ref().unwrap().text, "s");
                assert_eq!(aggregates[0].asc, Some(true));
                assert_eq!(aggregates[1].expr.alias, None);
                assert_eq!(aggregates[1].asc, None);
                assert_eq!(group_by.len(), 1);
                assert_eq!(text(&group_by[0].expr.expr), "a");
                assert_eq!(group_by[0].expr.alias.as_ref().unwrap().text, "x");
                assert_eq!(group_by[0].asc, Some(false));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn group_only_aggregate_has_no_items() {
        match only("SELECT * FROM t |> AGGREGATE GROUP BY a") {
            PipeOperator::Aggregate {
                aggregates,
                group_by,
            } => {
                assert!(aggregates.is_empty());
                assert_eq!(group_by.len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rename_keeps_old_and_new_apart() {
        match only("SELECT * FROM t |> RENAME a b") {
            PipeOperator::Rename(renames) => {
                assert_eq!(renames[0].name.text, "a");
                assert_eq!(renames[0].new_name.text, "b");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn set_operations_record_their_form() {
        match only("SELECT * FROM t |> EXCEPT DISTINCT BY NAME (SELECT 1), (SELECT 2)") {
            PipeOperator::SetOperation {
                op,
                quantifier,
                by_name,
                queries,
            } => {
                assert_eq!(op, SetOperator::Except);
                assert_eq!(quantifier, SetQuantifier::Distinct);
                assert!(by_name);
                assert_eq!(queries.len(), 2);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            only("SELECT * FROM t |> UNION ALL (SELECT 1)"),
            PipeOperator::SetOperation {
                op: SetOperator::Union,
                quantifier: SetQuantifier::All,
                by_name: false,
                ..
            }
        ));
        assert!(matches!(
            only("SELECT * FROM t |> INTERSECT DISTINCT (SELECT 1)"),
            PipeOperator::SetOperation {
                op: SetOperator::Intersect,
                ..
            }
        ));
    }

    #[test]
    fn call_keeps_function_and_alias() {
        match only("SELECT * FROM t |> CALL s.f(a) AS r") {
            PipeOperator::Call { function, alias } => {
                assert_eq!(text(&function), "s.f(a)");
                assert_eq!(alias.unwrap().text, "r");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn pivot_and_unpivot_keep_their_parts() {
        match only("SELECT * FROM t |> PIVOT(SUM(a) AS s FOR b IN ('x', 'y')) p") {
            PipeOperator::Pivot { clause, alias } => {
                assert_eq!(clause.aggregates.len(), 1);
                assert_eq!(text(&clause.columns[0]), "b");
                assert!(matches!(&clause.source, PivotSource::List(v) if v.len() == 2));
                assert_eq!(clause.default_on_null, None);
                assert_eq!(alias.unwrap().text, "p");
            }
            other => panic!("{other:?}"),
        }
        match only("SELECT * FROM t |> UNPIVOT(a FOR b IN (c, d)) AS u") {
            PipeOperator::Unpivot { clause, alias } => {
                assert_eq!(text(&clause.value), "a");
                assert_eq!(clause.name.text, "b");
                assert_eq!(clause.columns.len(), 2);
                assert_eq!(alias.unwrap().text, "u");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn sample_is_the_long_form() {
        match only("SELECT * FROM t |> TABLESAMPLE (50 PERCENT)") {
            PipeOperator::TableSample(sample) => {
                assert!(sample.long_form);
                assert_eq!(sample.method, None);
                assert!(matches!(
                    sample.amount,
                    SampleAmount::Quantity {
                        unit: Some(SampleUnit::Percent),
                        ..
                    }
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn join_is_a_real_join() {
        match only("SELECT * FROM t |> LEFT JOIN u ON a = b") {
            PipeOperator::Join(join) => assert_eq!(join.kind, JoinKind::Left),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn deeply_nested_pipe_queries_are_rejected() {
    let depth = 5_000;
    let sql = format!(
        "SELECT 1{}{}",
        " |> UNION (SELECT 1".repeat(depth),
        ")".repeat(depth)
    );
    assert!(rejected::<Spark>(&sql));
}

#[test]
fn a_long_chain_of_steps_is_fine() {
    let sql = format!("SELECT 1{}", " |> LIMIT 1".repeat(20_000));
    assert_eq!(renders(&sql), sql);
    let copy = parse_sql::<Spark>(&sql).unwrap();
    assert_eq!(copy.clone(), copy);
}

/// Every way a step can hold a query, as text around the nested query.
const NESTING_PATHS: [(&str, &str); 12] = [
    (" |> UNION (SELECT 1", ")"),
    (" |> WHERE (SELECT 1", ")"),
    (" |> SELECT (SELECT 1", ")"),
    (" |> SET a = (SELECT 1", ")"),
    (" |> JOIN (SELECT 1", ") AS x ON 1"),
    (" |> AGGREGATE (SELECT 1", ")"),
    (" |> CALL f((SELECT 1", "))"),
    (" |> PIVOT(a FOR b IN ((SELECT 1", ")))"),
    (" |> TABLESAMPLE ((SELECT 1", "))"),
    (" |> LIMIT (SELECT 1", ")"),
    (" |> ORDER BY (SELECT 1", ")"),
    (" |> UNPIVOT((SELECT 1", ") FOR b IN (c))"),
];

fn nested_sql(open: &str, close: &str, depth: usize) -> String {
    format!("SELECT 1{}{}", open.repeat(depth), close.repeat(depth))
}

#[test]
fn every_nesting_path_is_rejected_when_too_deep() {
    for (open, close) in NESTING_PATHS {
        assert!(rejected::<Spark>(&nested_sql(open, close, 5_000)), "{open}");
    }
}

#[test]
fn tallest_nesting_of_every_path_is_safe_to_use() {
    for (open, close) in NESTING_PATHS {
        let parsed = (1..=128).rev().find_map(|depth| {
            let sql = nested_sql(open, close, depth);
            parse_sql::<Spark>(&sql).ok().map(|tree| {
                let rendered = tree[0].to_string();
                let copy = tree.clone();
                assert!(copy == tree);
                assert!(!format!("{tree:?}").is_empty());
                (depth, rendered)
            })
        });
        let (depth, rendered) = parsed.unwrap_or_else(|| panic!("nothing parsed: {open}"));
        assert!(depth >= 30, "{open} only reached {depth}");
        assert!(rendered.starts_with("SELECT 1 |> "), "{open}");
    }
}

#[test]
fn pipe_joins_use_the_dialects_join_rule() {
    for sql in [
        "SELECT 1 |> SEMI JOIN u ON 1",
        "SELECT 1 |> ANTI JOIN u ON 1",
        "SELECT 1 |> LEFT SEMI JOIN u ON 1",
        "SELECT 1 |> NATURAL JOIN u",
    ] {
        assert_eq!(parse_sql::<Spark>(sql).unwrap()[0].to_string(), sql);
    }
    assert!(rejected::<Spark>("SELECT 1 |> NONSENSE"));
    assert!(rejected::<Spark>("SELECT 1 |> NATURAL WHERE a"));
}
