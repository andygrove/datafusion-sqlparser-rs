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

//! Databricks SQL: what it adds to Spark SQL, and what stays out of each.

use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::dialect::{Databricks, Spark};
use sqlparser::parse_sql;

fn round_trips(sql: &str) {
    let statements = parse_sql::<Databricks>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Databricks>(sql).is_err(), "accepted: {sql}");
}

fn spark_rejects(sql: &str) {
    assert!(parse_sql::<Spark>(sql).is_err(), "spark accepted: {sql}");
}

#[test]
fn inherits_spark_syntax() {
    round_trips("SELECT `a` FROM t LATERAL VIEW explode(x) v AS y");
    round_trips("SELECT * EXCEPT (a) FROM t CLUSTER BY b");
    round_trips("SELECT transform(a, x -> x + 1) FROM t |> WHERE b");
    round_trips("CREATE TABLE t (a INT) USING delta");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("Databricks").unwrap();
    assert_eq!(parse_sql_as(kind, "OPTIMIZE t").unwrap().len(), 1);
}

// Time travel

#[test]
fn time_travel_round_trips() {
    round_trips("SELECT 1 FROM t TIMESTAMP AS OF '2018-10-18T22:15:12.013Z'");
    round_trips("SELECT 1 FROM t VERSION AS OF 1");
    round_trips("SELECT 1 FROM t TIMESTAMP AS OF current_timestamp() - INTERVAL 12 HOURS");
    round_trips("SELECT 1 FROM a.b.c VERSION AS OF 1 AS x");
    round_trips("SELECT 1 FROM t VERSION AS OF 1 JOIN u VERSION AS OF 2 ON 1 = 1");
    spark_rejects("SELECT 1 FROM t VERSION AS OF 1");
}

#[test]
fn time_travel_needs_all_its_words() {
    rejected("SELECT 1 FROM t VERSION AS 1");
    rejected("SELECT 1 FROM t VERSION AS OF");
    rejected("SELECT 1 FROM t TIMESTAMP AS OF");
}

#[test]
fn a_version_is_a_literal_but_a_timestamp_can_be_computed() {
    round_trips("SELECT 1 FROM t VERSION AS OF '1'");
    rejected("SELECT 1 FROM t VERSION AS OF 1 - 2");
    rejected("SELECT 1 FROM t VERSION AS OF x");
    round_trips("SELECT 1 FROM t TIMESTAMP AS OF x - 2");
}

#[test]
fn time_travel_words_can_still_be_aliases() {
    round_trips("SELECT 1 FROM t AS version");
    round_trips("SELECT 1 FROM t timestamp");
}

// OPTIMIZE

#[test]
fn optimize_round_trips() {
    round_trips("OPTIMIZE t");
    round_trips("OPTIMIZE catalog.db.t");
    round_trips("OPTIMIZE t WHERE d >= '2023-01-01' AND d < '2023-02-01'");
    round_trips("OPTIMIZE t ZORDER BY (a)");
    round_trips("OPTIMIZE t WHERE d = 1 ZORDER BY (a, b)");
    spark_rejects("OPTIMIZE t");
}

#[test]
fn optimize_keeps_to_its_own_form() {
    rejected("OPTIMIZE");
    rejected("OPTIMIZE t ZORDER BY a");
    rejected("OPTIMIZE t ZORDER (a)");
    // These are ClickHouse's.
    rejected("OPTIMIZE TABLE t");
    rejected("OPTIMIZE t FINAL");
}

// USE

#[test]
fn use_can_name_the_kind_of_object() {
    round_trips("USE CATALOG c");
    round_trips("USE CATALOG `c`");
    round_trips("USE DATABASE d");
    round_trips("USE SCHEMA s");
    round_trips("USE SCHEMA `c`.`s`");
    round_trips("USE SCHEMA 's'");
    round_trips("USE plain");
    spark_rejects("USE CATALOG c");
}

#[test]
fn use_of_a_kind_needs_a_name() {
    rejected("USE CATALOG");
    rejected("USE DATABASE");
    rejected("USE SCHEMA");
}

// Names that start with a digit

#[test]
fn names_can_start_with_a_digit() {
    round_trips("SELECT * FROM catalog.schema.1st_table");
    round_trips("SELECT * FROM a.b.1c");
    round_trips("SELECT 1 FROM 5abc.dataField");
    round_trips("SELECT 1db.1table.1column");
    round_trips("SELECT a AS 3_barrr_asdf FROM t");
    round_trips("SELECT t.1two3, t.15to29 FROM t");
    spark_rejects("SELECT * FROM a.b.1c");
}

#[test]
fn numbers_stay_numbers() {
    for number in [
        "1", "1.5", ".5", "1e5", "1E5", "1e+5", "1e-5", "1.5e3", "5.",
    ] {
        let sql = format!("SELECT {number}");
        let parsed = parse_sql::<Databricks>(&sql).unwrap();
        assert_eq!(parsed[0].to_string(), sql);
    }
    round_trips("SELECT 1 + 2");
    round_trips("SELECT a.b, 1.5 FROM t");
    round_trips("SELECT X'1F'");
}

// REPLACE in a wildcard

#[test]
fn wildcard_can_replace_columns() {
    round_trips("SELECT * REPLACE (upper(name) AS name) FROM t");
    round_trips("SELECT * REPLACE (a / 2 AS a, 3 AS b) FROM t");
    round_trips("SELECT t.* REPLACE (1 AS a) FROM t");
    round_trips("SELECT * EXCEPT (a) REPLACE (1 AS b) FROM t");
    spark_rejects("SELECT * REPLACE (1 AS a) FROM t");
}

#[test]
fn replace_needs_a_wildcard_and_an_alias() {
    rejected("SELECT a REPLACE (1 AS b) FROM t");
    rejected("SELECT * REPLACE (1) FROM t");
}

// FROM first

#[test]
fn from_can_come_first() {
    round_trips("FROM t");
    round_trips("FROM t SELECT a, b");
    round_trips("FROM t SELECT *");
    round_trips("FROM a, b");
    round_trips("FROM t SELECT a WHERE b GROUP BY a HAVING c");
    round_trips("FROM t |> WHERE a > 1 |> SELECT a");
    round_trips("CREATE VIEW v AS FROM a.b.c, d");
    round_trips("WITH x AS (FROM t SELECT a) FROM x SELECT 1");
    round_trips("SELECT * FROM (FROM t SELECT a) AS x");
    round_trips("SELECT (FROM t SELECT a)");
    round_trips("FROM a UNION ALL FROM b");
    spark_rejects("FROM t SELECT a");
}

#[test]
fn from_first_still_needs_a_table() {
    rejected("FROM");
    rejected("FROM SELECT a");
}

// exists as a function

#[test]
fn exists_can_be_the_array_function() {
    round_trips("SELECT exists(array(1, 2, 3), x -> x IS NULL)");
    assert_eq!(
        parse_sql::<Databricks>("SELECT EXISTS (NULL)").unwrap()[0].to_string(),
        "SELECT EXISTS(NULL)"
    );
}

#[test]
fn exists_over_a_query_is_still_a_subquery_test() {
    round_trips("SELECT EXISTS (SELECT 1)");
    round_trips("SELECT EXISTS (FROM t)");
    round_trips("SELECT EXISTS (WITH x AS (SELECT 1) SELECT * FROM x)");
}

mod shape {
    use sqlparser::ast::*;
    use sqlparser::dialect::Databricks;
    use sqlparser::dialect::builtin::{Builtin, BuiltinStatement};
    use sqlparser::dialect::databricks::DatabricksStatement;
    use sqlparser::{parse_query, parse_sql};

    type Sel<'a> = Select<'a, Builtin>;

    fn select(sql: &str) -> Sel<'_> {
        match parse_query::<Databricks>(sql).unwrap().body {
            SetExpr::Select(select) => *select,
            other => panic!("not a select: {other:?}"),
        }
    }

    fn statement(sql: &str) -> DatabricksStatement<'_> {
        match parse_sql::<Databricks>(sql).unwrap().remove(0).kind {
            StatementKind::Ext(BuiltinStatement::Databricks(statement)) => statement,
            other => panic!("not a Databricks statement: {other:?}"),
        }
    }

    #[test]
    fn time_travel_keeps_its_table_kind_and_point() {
        let select = select("SELECT 1 FROM a.b VERSION AS OF 7 AS x");
        let table = &select.from[0].relation;
        let TableFactor::TimeTravel(travel) = &table.factor else {
            panic!("not time travel: {:?}", table.factor);
        };
        assert_eq!(travel.name.to_string(), "a.b");
        assert_eq!(travel.kind, "VERSION");
        assert_eq!(travel.at.to_string(), "7");
        assert_eq!(table.alias.as_ref().unwrap().name.text, "x");
    }

    #[test]
    fn optimize_keeps_its_clauses_apart() {
        let DatabricksStatement::Optimize {
            table,
            predicate,
            zorder_by,
        } = statement("OPTIMIZE a.b WHERE c = 1 ZORDER BY (d, e)")
        else {
            panic!("not OPTIMIZE");
        };
        assert_eq!(table.to_string(), "a.b");
        assert_eq!(predicate.unwrap().to_string(), "c = 1");
        assert_eq!(zorder_by.len(), 2);
    }

    #[test]
    fn use_records_the_kind_of_object() {
        let DatabricksStatement::Use { kind, name } = statement("use catalog c") else {
            panic!("not USE");
        };
        assert_eq!(kind, "CATALOG");
        assert_eq!(name.to_string(), "c");
    }

    #[test]
    fn use_without_a_kind_is_the_core_statement() {
        let parsed = parse_sql::<Databricks>("USE c").unwrap();
        assert!(matches!(parsed[0].kind, StatementKind::Use(_)));
    }

    #[test]
    fn digit_name_is_one_part_of_a_name() {
        let select = select("SELECT 1 FROM a.1b");
        let TableFactor::Named(name) = &select.from[0].relation.factor else {
            panic!("not a name");
        };
        assert_eq!(name.parts.len(), 2);
        assert_eq!(name.parts[1].text, "1b");
    }

    #[test]
    fn exponent_is_a_number_not_a_name() {
        let select = select("SELECT 1e5");
        assert!(matches!(
            select.projection[0].kind,
            SelectItemKind::Expr {
                expr: Expr {
                    kind: ExprKind::Literal(Literal::Number("1e5")),
                    ..
                },
                alias: None
            }
        ));
    }

    #[test]
    fn replace_is_kept_with_the_columns_excepted() {
        let select = select("SELECT * EXCEPT (a) REPLACE (1 AS b) FROM t");
        let SelectItemKind::WildcardWithOptions { options, .. } = &select.projection[0].kind else {
            panic!("not a wildcard with options");
        };
        assert_eq!(options.exclude.len(), 1);
        assert_eq!(options.replace.len(), 1);
        assert_eq!(options.replace[0].1.text, "b");
    }

    #[test]
    fn from_first_without_select_has_no_projection() {
        let select = select("FROM t");
        assert!(select.from_first);
        assert!(select.projection.is_empty());
        assert_eq!(select.from.len(), 1);
    }

    #[test]
    fn ordinary_select_is_not_from_first() {
        assert!(!select("SELECT 1 FROM t").from_first);
    }

    #[test]
    fn exists_over_values_is_a_function_and_over_a_query_is_not() {
        let function = select("SELECT exists(a, x -> x)");
        assert!(matches!(
            function.projection[0].kind,
            SelectItemKind::Expr {
                expr: Expr {
                    kind: ExprKind::Function { .. },
                    ..
                },
                ..
            }
        ));
        let subquery = select("SELECT EXISTS (FROM t)");
        assert!(matches!(
            subquery.projection[0].kind,
            SelectItemKind::Expr {
                expr: Expr {
                    kind: ExprKind::Exists(_),
                    ..
                },
                ..
            }
        ));
    }
}
