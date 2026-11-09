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

//! Apache Spark SQL.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinStatement};
use sqlparser::dialect::spark::{SparkStatement, SparkTableClause};
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Spark};
use sqlparser::{parse_expr, parse_sql};

fn renders<D: Dialect>(sql: &str) -> String {
    let statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips(sql: &str) {
    assert_eq!(renders::<Spark>(sql), sql);
}

fn rejected<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

fn expr(sql: &'static str) -> Expr<'static, Builtin> {
    parse_expr::<Spark>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"))
}

fn c(name: &'static str) -> Expr<'static, Builtin> {
    col(name)
}

fn n(text: &'static str) -> Expr<'static, Builtin> {
    num(text)
}

fn one(sql: &'static str) -> Statement<'static, Builtin> {
    parse_sql::<Spark>(sql).unwrap().remove(0)
}

fn select_of(sql: &'static str) -> Select<'static, Builtin> {
    match one(sql).kind {
        StatementKind::Query(query) => into_select(*query),
        other => panic!("expected a query: {other:?}"),
    }
}

fn lambda(
    params: Vec<Ident<'static>>,
    parenthesized: bool,
    body: Expr<'static, Builtin>,
) -> Expr<'static, Builtin> {
    ExprKind::Special(Box::new(SpecialExpr::Lambda {
        params,
        parenthesized,
        body,
    }))
    .into()
}

// Lexing

#[test]
fn backticks_quote_identifiers() {
    assert_eq!(expr("`a b`"), ExprKind::Ident(quoted("a b", '`')).into());
}

#[test]
fn double_quotes_are_strings() {
    let ExprKind::Literal(Literal::String(text)) = expr("\"a\"").kind else {
        panic!("expected a string");
    };
    assert_eq!(text.text, "a");
    assert_eq!(text.quote, '"');
}

#[test]
fn backslashes_escape_in_strings() {
    let ExprKind::Literal(Literal::String(text)) = expr(r"'a\'b'").kind else {
        panic!("expected a string");
    };
    assert_eq!(text.value(), "a'b");
}

#[test]
fn colons_still_mark_parameters() {
    assert_eq!(expr(":name"), ExprKind::Placeholder(":name").into());
}

// Operators

#[test]
fn bang_negates_like_not() {
    assert_eq!(
        expr("!a AND b"),
        bin(unary(UnaryOp::BangNot, c("a")), And, c("b"))
    );
    assert_eq!(
        expr("!a = b"),
        unary(UnaryOp::BangNot, bin(c("a"), Eq, c("b")))
    );
    round_trips("SELECT !a FROM t");
}

#[test]
fn bang_is_not_valid_elsewhere() {
    assert!(rejected::<Core>("SELECT !a"));
    assert!(rejected::<Postgres>("SELECT !a"));
}

#[test]
fn not_equal_still_lexes_as_one_operator() {
    assert_eq!(expr("a != b"), bin(c("a"), NotEq, c("b")));
}

#[test]
fn div_binds_like_multiplication() {
    let div = |left, right| renders::<Spark>(&format!("SELECT {left} DIV {right}"));
    assert_eq!(div("1", "2"), "SELECT 1 DIV 2");
    round_trips("SELECT a + b DIV c * d FROM t");
    assert!(rejected::<Postgres>("SELECT 5 DIV 2 FROM t"));
}

#[test]
fn div_is_not_an_alias() {
    assert!(rejected::<Spark>("SELECT 1 DIV FROM t"));
}

// Lambdas

#[test]
fn single_parameter_lambda() {
    assert_eq!(
        expr("x -> x + 1"),
        lambda(vec![ident("x")], false, bin(c("x"), Add, n("1")))
    );
}

#[test]
fn parenthesized_lambda() {
    assert_eq!(
        expr("(x, i) -> i > 0"),
        lambda(vec![ident("x"), ident("i")], true, bin(c("i"), Gt, n("0")))
    );
}

#[test]
fn lambdas_round_trip_as_arguments() {
    round_trips("SELECT filter(arr, x -> x > 2) FROM t");
    round_trips("SELECT filter(arr, (x, i) -> i > 0) FROM t");
}

#[test]
fn lambda_body_extends_as_far_as_possible() {
    assert_eq!(
        expr("x -> x OR y"),
        lambda(vec![ident("x")], false, bin(c("x"), Or, c("y")))
    );
}

#[test]
fn parentheses_without_an_arrow_are_not_a_lambda() {
    assert_eq!(expr("(x)"), parens(c("x")));
    round_trips("SELECT (a, b) FROM t");
}

#[test]
fn a_lambda_needs_a_body() {
    assert!(parse_expr::<Spark>("x ->").is_err());
    assert!(parse_expr::<Spark>("(x, 1) -> x").is_err());
}

// Wildcards

#[test]
fn except_after_a_wildcard_holds_its_columns() {
    let select = select_of("SELECT * EXCEPT (a, b) FROM t");
    let SelectItemKind::WildcardWithOptions { qualifier, options } = &select.projection[0].kind
    else {
        panic!("expected options: {:?}", select.projection);
    };
    assert_eq!(*qualifier, None);
    assert_eq!(options.exclude, [ident("a"), ident("b")]);
    round_trips("SELECT * EXCEPT (a, b) FROM t");
    round_trips("SELECT t.* EXCEPT (a) FROM t");
}

#[test]
fn except_before_a_parenthesized_query_is_a_set_operator() {
    let StatementKind::Query(query) = one("SELECT * EXCEPT (SELECT 1)").kind else {
        panic!("expected a query");
    };
    assert!(
        matches!(query.body, SetExpr::SetOperation { .. }),
        "{:?}",
        query.body
    );
}

#[test]
fn except_after_a_wildcard_is_still_a_set_operator_without_parens() {
    let StatementKind::Query(query) = one("SELECT * FROM t EXCEPT SELECT * FROM u").kind else {
        panic!("expected a query");
    };
    assert!(
        matches!(query.body, SetExpr::SetOperation { .. }),
        "{:?}",
        query.body
    );
}

// Types

fn type_of(sql: &'static str) -> DataType<'static> {
    let ExprKind::Cast { ty, .. } = expr(sql).kind else {
        panic!("expected a cast: {sql}");
    };
    *ty
}

#[test]
fn angle_brackets_hold_element_types() {
    let ty = type_of("CAST(a AS ARRAY<INT>)");
    assert_eq!(ty.name, ident("ARRAY"));
    let Some(arguments) = ty.angle else {
        panic!("expected type arguments");
    };
    let AngleArguments::Types(types) = *arguments else {
        panic!("expected types");
    };
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].name, ident("INT"));
}

#[test]
fn nested_angle_brackets_close_separately() {
    round_trips("SELECT CAST(a AS ARRAY<ARRAY<INT>>) FROM t");
    round_trips("SELECT CAST(a AS MAP<STRING, ARRAY<INT>>) FROM t");
}

#[test]
fn struct_fields_hold_names_and_comments() {
    let ty = type_of("CAST(a AS STRUCT<x: INT, y STRING COMMENT 'c'>)");
    let AngleArguments::Fields(fields) = *ty.angle.unwrap() else {
        panic!("expected fields");
    };
    assert_eq!(fields[0].name, Some(ident("x")));
    assert!(fields[0].colon);
    assert_eq!(fields[0].data_type.name, ident("INT"));
    assert_eq!(fields[1].name, Some(ident("y")));
    assert!(!fields[1].colon);
    assert_eq!(fields[1].comment.as_ref().unwrap().text, "c");
}

#[test]
fn struct_fields_allow_a_colon_with_no_space() {
    assert_eq!(
        renders::<Spark>("SELECT CAST(a AS STRUCT<x:DECIMAL(10,2),y:ARRAY<INT>>)"),
        "SELECT CAST(a AS STRUCT<x: DECIMAL(10,2), y: ARRAY<INT>>)"
    );
}

#[test]
fn angle_brackets_need_the_right_number_of_types() {
    assert!(rejected::<Spark>("SELECT CAST(a AS ARRAY<INT, INT>)"));
    assert!(rejected::<Spark>("SELECT CAST(a AS MAP<INT>)"));
    assert!(rejected::<Spark>("SELECT CAST(a AS ARRAY<INT)"));
}

#[test]
fn only_complex_types_take_angle_brackets() {
    assert!(rejected::<Spark>("SELECT CAST(a AS INT<INT>)"));
    assert!(rejected::<Postgres>("SELECT CAST(a AS ARRAY<INT>)"));
}

#[test]
fn deep_angle_brackets_error_instead_of_overflowing_the_stack() {
    let sql = format!("SELECT CAST(a AS {}INT", "ARRAY<".repeat(100_000));
    assert!(rejected::<Spark>(&sql));
}

// Queries

#[test]
fn group_by_modifiers() {
    let select = select_of("SELECT a FROM t GROUP BY a WITH ROLLUP");
    assert_eq!(select.group_by_modifier, Some(GroupByModifier::Rollup));
    let select = select_of("SELECT a FROM t GROUP BY a WITH CUBE");
    assert_eq!(select.group_by_modifier, Some(GroupByModifier::Cube));
    round_trips("SELECT a FROM t GROUP BY a WITH ROLLUP");
    round_trips("SELECT a FROM t GROUP BY a WITH CUBE");
}

#[test]
fn mysql_has_with_rollup_but_not_with_cube() {
    assert_eq!(
        renders::<MySql>("SELECT a FROM t GROUP BY a WITH ROLLUP"),
        "SELECT a FROM t GROUP BY a WITH ROLLUP"
    );
    assert!(rejected::<MySql>("SELECT a FROM t GROUP BY a WITH CUBE"));
    assert!(rejected::<Core>("SELECT a FROM t GROUP BY a WITH ROLLUP"));
}

#[test]
fn grouping_functions_work_in_group_by() {
    round_trips("SELECT a FROM t GROUP BY ROLLUP (a, b)");
    round_trips("SELECT a FROM t GROUP BY GROUPING SETS ((a), (b))");
}

#[test]
fn values_can_be_a_table() {
    let select = select_of("SELECT * FROM VALUES (1), (2) AS t");
    let [from] = select.from.as_slice() else {
        panic!("expected one table");
    };
    assert_eq!(
        from.relation.factor,
        TableFactor::Values(vec![vec![n("1")], vec![n("2")]])
    );
    round_trips("SELECT * FROM VALUES (1, 2), (3, 4) AS t");
}

#[test]
fn values_as_a_table_stops_at_the_next_table() {
    let select = select_of("SELECT * FROM VALUES (1), u");
    assert_eq!(select.from.len(), 2);
    assert_eq!(table_name(&select.from[1].relation), &object_name(&["u"]));
}

#[test]
fn common_table_expressions_can_omit_as() {
    assert_eq!(
        renders::<Spark>("WITH t (SELECT 1) SELECT * FROM t"),
        "WITH t AS (SELECT 1) SELECT * FROM t"
    );
    assert!(rejected::<Core>("WITH t (SELECT 1) SELECT * FROM t"));
}

// CREATE TABLE

#[test]
fn plain_create_table_is_the_core_node() {
    assert!(matches!(
        one("CREATE TABLE t (a INT)").kind,
        StatementKind::CreateTable(_)
    ));
}

#[test]
fn create_table_using_keeps_its_clauses_in_order() {
    let StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::CreateTable(create))) =
        one("CREATE EXTERNAL TABLE t (a INT) USING parquet LOCATION '/d' COMMENT 'c'").kind
    else {
        panic!("expected a Spark table");
    };
    assert!(create.external);
    assert_eq!(create.table.name, object_name(&["t"]));
    assert_eq!(create.table.columns.len(), 1);
    let [
        SparkTableClause::Using(format),
        SparkTableClause::Location(location),
        SparkTableClause::Comment(comment),
    ] = create.clauses.as_slice()
    else {
        panic!("unexpected clauses: {:?}", create.clauses);
    };
    assert_eq!(*format, ident("parquet"));
    assert_eq!(location.text, "/d");
    assert_eq!(comment.text, "c");
}

#[test]
fn create_table_forms_round_trip() {
    for sql in [
        "CREATE TABLE t (i INT, s STRING) USING parquet",
        "CREATE TABLE IF NOT EXISTS t (i INT) USING delta",
        "CREATE TABLE t USING parquet AS SELECT 1",
        "CREATE OR REPLACE TABLE t (i INT) USING parquet",
        "CREATE TABLE t (a INT, b STRING) USING parquet PARTITIONED BY (b)",
        "CREATE TABLE t (a INT) PARTITIONED BY (b STRING, c INT)",
        "CREATE TABLE t (a INT) USING csv OPTIONS (header = 'true', 'a.b' = 1)",
        "CREATE TABLE t (a INT) TBLPROPERTIES (k.v = 'x')",
        "CREATE TABLE t (a INT COMMENT 'x') USING parquet",
        "CREATE TABLE t (arr ARRAY<INT>, m MAP<STRING, INT>) USING parquet",
    ] {
        round_trips(sql);
    }
}

#[test]
fn options_can_omit_the_equals_sign_and_the_value() {
    round_trips("CREATE TABLE t (a INT) USING csv OPTIONS (a 'b', c)");
}

#[test]
fn partition_columns_can_have_types() {
    let StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::CreateTable(create))) =
        one("CREATE TABLE t (a INT) PARTITIONED BY (b STRING, c)").kind
    else {
        panic!("expected a Spark table");
    };
    let [SparkTableClause::PartitionedBy(columns)] = create.clauses.as_slice() else {
        panic!("unexpected clauses");
    };
    assert_eq!(columns[0].name, ident("b"));
    assert_eq!(columns[0].data_type.as_ref().unwrap().name, ident("STRING"));
    assert_eq!(columns[1].name, ident("c"));
    assert_eq!(columns[1].data_type, None);
}

#[test]
fn table_clauses_cannot_be_repeated() {
    assert!(rejected::<Spark>("CREATE TABLE t (a INT) USING a USING b"));
    assert!(rejected::<Spark>(
        "CREATE TABLE t (a INT) LOCATION 'a' LOCATION 'b'"
    ));
}

#[test]
fn a_table_needs_columns_clauses_or_a_query() {
    assert!(rejected::<Spark>("CREATE TABLE t"));
}

#[test]
fn table_clauses_are_spark_syntax() {
    assert!(rejected::<Core>("CREATE TABLE t (a INT) USING parquet"));
    assert!(rejected::<Core>("CREATE EXTERNAL TABLE t (a INT)"));
}

// Limits

#[test]
fn deep_bangs_error_instead_of_overflowing_the_stack() {
    let sql = format!("SELECT {}a", "!".repeat(100_000));
    assert!(rejected::<Spark>(&sql));
}

#[test]
fn deep_lambdas_error_instead_of_overflowing_the_stack() {
    let sql = format!("SELECT {}a", "x -> ".repeat(100_000));
    assert!(rejected::<Spark>(&sql));
}

#[test]
fn lateral_views_round_trip() {
    round_trips("SELECT a FROM t LATERAL VIEW explode(a) x AS j, p WHERE a = 1");
    round_trips("SELECT a FROM t LATERAL VIEW explode(a) x AS j LATERAL VIEW OUTER f(b) y AS k");
    round_trips("SELECT a FROM t AS u LATERAL VIEW explode(a) x");
    assert_eq!(
        renders::<Spark>("SELECT a FROM t LATERAL VIEW explode(a) x j"),
        "SELECT a FROM t LATERAL VIEW explode(a) x AS j"
    );
}

#[test]
fn lateral_views_are_spark_syntax() {
    assert!(rejected::<Core>("SELECT a FROM t LATERAL VIEW f(a) x AS j"));
    assert!(rejected::<Spark>("SELECT a FROM t LATERAL VIEW f(a)"));
}

#[test]
fn distribution_clauses_round_trip() {
    round_trips("SELECT * FROM t SORT BY a");
    round_trips("SELECT * FROM t SORT BY a, b DESC");
    round_trips("SELECT * FROM t CLUSTER BY a, b");
    round_trips("SELECT * FROM t DISTRIBUTE BY a, b");
    round_trips("SELECT * FROM t DISTRIBUTE BY a SORT BY b");
    round_trips("SELECT a FROM t GROUP BY a HAVING a > 1 SORT BY a LIMIT 3");
}

#[test]
fn distribution_clauses_keep_their_order() {
    assert!(rejected::<Spark>(
        "SELECT * FROM t SORT BY a DISTRIBUTE BY b"
    ));
    assert!(rejected::<Spark>("SELECT * FROM t SORT BY a HAVING b"));
    assert!(rejected::<Spark>("SELECT * FROM t SORT a"));
}

#[test]
fn distribution_words_are_still_aliases() {
    round_trips("SELECT 1 FROM t AS sort");
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t sort"),
        "SELECT 1 FROM t sort"
    );
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t cluster SORT BY a"),
        "SELECT 1 FROM t cluster SORT BY a"
    );
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t distribute"),
        "SELECT 1 FROM t distribute"
    );
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t lateral"),
        "SELECT 1 FROM t lateral"
    );
}

#[test]
fn distribution_clauses_are_spark_syntax() {
    assert!(rejected::<Core>("SELECT * FROM t DISTRIBUTE BY a"));
    assert!(rejected::<Postgres>("SELECT * FROM t SORT BY a"));
}

#[test]
fn spark_inserts_round_trip() {
    round_trips("INSERT OVERWRITE TABLE db.t PARTITION (a = '1', b) SELECT a FROM c");
    round_trips("INSERT OVERWRITE INTO s.t SELECT 1");
    round_trips("INSERT OVERWRITE t SELECT 1");
    round_trips("INSERT INTO TABLE t SELECT 1");
    round_trips("INSERT INTO t PARTITION (a) VALUES (1)");
    round_trips("INSERT INTO t (a, b) PARTITION (c) SELECT 1");
    round_trips("INSERT INTO t PARTITION (c) (a, b) SELECT 1");
}

#[test]
fn directories_round_trip() {
    round_trips("INSERT OVERWRITE LOCAL DIRECTORY '/x' STORED AS TEXTFILE SELECT 1");
    round_trips("INSERT OVERWRITE DIRECTORY '/x' SELECT * FROM t");
}

#[test]
fn malformed_spark_inserts_are_rejected() {
    assert!(rejected::<Spark>("INSERT DIRECTORY '/x' SELECT 1"));
    assert!(rejected::<Spark>("INSERT t SELECT 1"));
    assert!(rejected::<Spark>("INSERT INTO t PARTITION () SELECT 1"));
    assert!(rejected::<Spark>("INSERT INTO t PARTITION a SELECT 1"));
    assert!(rejected::<Spark>("INSERT OVERWRITE DIRECTORY x SELECT 1"));
    assert!(rejected::<Spark>("INSERT OVERWRITE LOCAL t SELECT 1"));
    assert!(rejected::<Spark>(
        "INSERT OVERWRITE DIRECTORY '/x' STORED SELECT 1"
    ));
    assert!(rejected::<Spark>(
        "INSERT INTO t (a) PARTITION (b) (c) SELECT 1"
    ));
}

#[test]
fn overwrite_is_spark_syntax() {
    assert!(rejected::<Core>("INSERT OVERWRITE TABLE t SELECT 1"));
    assert!(rejected::<MySql>("INSERT INTO t PARTITION (a) SELECT 1"));
}

#[test]
fn multi_column_aliases_round_trip() {
    round_trips("SELECT stack(2, 'a', 'b') AS (c, d) FROM t");
    round_trips("SELECT f(a) AS (c), b AS e FROM t");
    assert!(rejected::<Spark>("SELECT a AS () FROM t"));
    assert!(rejected::<Core>("SELECT a AS (b, c) FROM t"));
}

#[test]
fn between_low_bound_can_be_a_comparison() {
    round_trips("SELECT a BETWEEN b = c AND d");
    round_trips("SELECT a NOT BETWEEN b < c AND d");
    round_trips("SELECT a BETWEEN b >= c AND d");
    assert!(rejected::<Core>("SELECT a BETWEEN b = c AND d"));
    assert!(rejected::<Spark>("SELECT a BETWEEN b = c = d AND e"));
}

#[test]
fn table_samples_round_trip() {
    round_trips("SELECT * FROM t TABLESAMPLE (10 ROWS)");
    round_trips("SELECT * FROM t TABLESAMPLE (10 PERCENT) AS s");
    round_trips("SELECT * FROM t AS u TABLESAMPLE (50)");
    round_trips("SELECT * FROM t TABLESAMPLE (BUCKET 3 OUT OF 16 ON id)");
    round_trips("SELECT * FROM t TABLESAMPLE (BUCKET 3 OUT OF 16)");
}

#[test]
fn bucket_samples_need_both_numbers() {
    assert!(rejected::<Spark>("SELECT * FROM t TABLESAMPLE (BUCKET 3)"));
    assert!(rejected::<Spark>(
        "SELECT * FROM t TABLESAMPLE (BUCKET 3 OUT 16)"
    ));
}

#[test]
fn table_sample_is_spark_syntax_here() {
    assert!(rejected::<Core>("SELECT * FROM t TABLESAMPLE (10 ROWS)"));
}

#[test]
fn table_pivots_round_trip() {
    round_trips("SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x', 'y')) AS p (c, d) ORDER BY e");
    round_trips("SELECT * FROM (SELECT 1) PIVOT(max(a) FOR b IN (0, 1))");
    round_trips(
        "SELECT * FROM t AS c UNPIVOT(a FOR b IN (d, e)) AS u PIVOT(SUM(a) FOR b IN ('d')) AS p",
    );
    round_trips("SELECT * FROM t PIVOT(SUM(a) FOR (b, c) IN (('x', 1) AS g))");
}

#[test]
fn pivot_is_still_an_alias_without_its_clause() {
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t pivot"),
        "SELECT 1 FROM t pivot"
    );
}

#[test]
fn snowflake_pivot_options_stay_in_snowflake() {
    assert!(rejected::<Spark>(
        "SELECT * FROM t PIVOT(SUM(a) FOR b IN ('x') DEFAULT ON NULL (0))"
    ));
    assert!(rejected::<Spark>(
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN (a) DEFINE a AS TRUE) AS x y"
    ));
}

#[test]
fn semi_and_anti_joins_round_trip() {
    round_trips("SELECT * FROM a LEFT SEMI JOIN b ON x = y");
    round_trips("SELECT * FROM a LEFT ANTI JOIN b USING(x)");
    round_trips("SELECT * FROM a SEMI JOIN b ON x = y");
    round_trips("SELECT * FROM a ANTI JOIN b USING(x)");
}

#[test]
fn semi_and_anti_stay_ordinary_names() {
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t semi"),
        "SELECT 1 FROM t semi"
    );
    assert_eq!(
        renders::<Spark>("SELECT 1 FROM t anti"),
        "SELECT 1 FROM t anti"
    );
    assert!(rejected::<Spark>(
        "SELECT * FROM a RIGHT SEMI JOIN b ON x = y"
    ));
    assert!(rejected::<Spark>("SELECT * FROM a LEFT SEMI b ON x = y"));
    assert!(rejected::<Core>(
        "SELECT * FROM a LEFT SEMI JOIN b ON x = y"
    ));
}

mod shape {
    use super::*;
    use sqlparser::dialect::spark::{SparkInsert, SparkInsertDirectory};
    use sqlparser::parse_query;

    fn select(sql: &'static str) -> Select<'static, Builtin> {
        common::into_select(parse_query::<Spark>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}")))
    }

    fn join_kind(sql: &'static str) -> JoinKind<'static> {
        select(sql).from[0].joins[0].kind
    }

    fn spark_insert(sql: &'static str) -> SparkInsert<'static> {
        match one(sql).kind {
            StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::Insert(insert))) => *insert,
            other => panic!("{other:?}"),
        }
    }

    fn directory(sql: &'static str) -> SparkInsertDirectory<'static> {
        match one(sql).kind {
            StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::InsertDirectory(
                insert,
            ))) => *insert,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lateral_view_keeps_its_parts() {
        let select =
            select("SELECT 1 FROM t LATERAL VIEW OUTER f(a) x AS j, k LATERAL VIEW g(b) y");
        assert_eq!(select.lateral_views.len(), 2);
        let view = &select.lateral_views[0];
        assert!(view.outer);
        assert_eq!(view.function.to_string(), "f(a)");
        assert_eq!(view.name.to_string(), "x");
        assert_eq!(view.columns.len(), 2);
        assert_eq!(view.columns[1].text, "k");
        assert!(!select.lateral_views[1].outer);
        assert!(select.lateral_views[1].columns.is_empty());
    }

    #[test]
    fn a_clause_word_does_not_become_the_table_alias() {
        for sql in [
            "SELECT 1 FROM t SORT BY a",
            "SELECT 1 FROM t CLUSTER BY a",
            "SELECT 1 FROM t DISTRIBUTE BY a",
            "SELECT 1 FROM t LATERAL VIEW f(a) x",
        ] {
            assert_eq!(select(sql).from[0].relation.alias, None, "{sql}");
        }
    }

    #[test]
    fn distribution_keeps_its_lists_apart() {
        let distribution = sqlparser::parse_query::<Spark>(
            "SELECT 1 FROM t CLUSTER BY a DISTRIBUTE BY b, c SORT BY d DESC",
        )
        .unwrap()
        .distribution
        .unwrap();
        assert_eq!(distribution.cluster_by, vec![c("a")]);
        assert_eq!(distribution.distribute_by, vec![c("b"), c("c")]);
        assert_eq!(distribution.sort_by.len(), 1);
        assert_eq!(distribution.sort_by[0].expr, c("d"));
        assert_eq!(distribution.sort_by[0].asc, Some(false));
    }

    #[test]
    fn a_select_without_distribution_has_none() {
        let query = sqlparser::parse_query::<Spark>("SELECT 1 FROM t").unwrap();
        assert!(query.distribution.is_none());
        assert!(select("SELECT 1 FROM t").lateral_views.is_empty());
    }

    #[test]
    fn multi_column_alias_keeps_its_names() {
        match select("SELECT f(a) AS (b, c)").projection.remove(0).kind {
            SelectItemKind::ExprWithAliases { expr, aliases } => {
                assert_eq!(expr.to_string(), "f(a)");
                assert_eq!(aliases, vec![common::ident("b"), common::ident("c")]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn between_low_bound_comparison_stays_in_the_bound() {
        match expr("a BETWEEN b = c AND d").kind {
            ExprKind::Between {
                expr, low, high, ..
            } => {
                assert_eq!(*expr, c("a"));
                assert_eq!(*low, bin(c("b"), Eq, c("c")));
                assert_eq!(*high, c("d"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn joins_record_their_kind() {
        assert_eq!(
            join_kind("SELECT 1 FROM a LEFT SEMI JOIN b"),
            JoinKind::LeftSemi
        );
        assert_eq!(
            join_kind("SELECT 1 FROM a LEFT ANTI JOIN b"),
            JoinKind::LeftAnti
        );
        assert_eq!(join_kind("SELECT 1 FROM a SEMI JOIN b"), JoinKind::Semi);
        assert_eq!(join_kind("SELECT 1 FROM a ANTI JOIN b"), JoinKind::Anti);
        assert_eq!(join_kind("SELECT 1 FROM a LEFT JOIN b"), JoinKind::Left);
    }

    #[test]
    fn a_sample_before_the_alias_wraps_the_bare_table() {
        let table = select("SELECT 1 FROM t TABLESAMPLE (5 ROWS) AS s")
            .from
            .remove(0)
            .relation;
        assert_eq!(table.alias.unwrap().name.text, "s");
        match table.factor {
            TableFactor::Sampled(sample) => {
                assert_eq!(sample.table.alias, None);
                assert!(sample.sample.long_form);
                assert!(matches!(
                    sample.sample.amount,
                    SampleAmount::Quantity {
                        unit: Some(SampleUnit::Rows),
                        ..
                    }
                ));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_sample_after_the_alias_wraps_the_aliased_table() {
        let table = select("SELECT 1 FROM t AS s TABLESAMPLE (5)")
            .from
            .remove(0)
            .relation;
        assert_eq!(table.alias, None);
        match table.factor {
            TableFactor::Sampled(sample) => {
                assert_eq!(sample.table.alias.unwrap().name.text, "s")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bucket_sample_keeps_its_numbers() {
        let table = select("SELECT 1 FROM t TABLESAMPLE (BUCKET 3 OUT OF 16 ON id)")
            .from
            .remove(0)
            .relation;
        match table.factor {
            TableFactor::Sampled(sample) => match sample.sample.amount {
                SampleAmount::Bucket { bucket, total, on } => {
                    assert_eq!(bucket, "3");
                    assert_eq!(total, "16");
                    assert_eq!(*on.unwrap(), c("id"));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn plain_insert_is_the_core_statement() {
        assert!(matches!(
            one("INSERT INTO t SELECT 1").kind,
            StatementKind::Insert(_)
        ));
    }

    #[test]
    fn spark_insert_keeps_its_parts() {
        let insert = spark_insert("INSERT OVERWRITE TABLE s.t PARTITION (a = 1, b) (c) SELECT 1");
        assert!(insert.overwrite);
        assert!(!insert.into);
        assert!(insert.table_keyword);
        assert_eq!(insert.table.to_string(), "s.t");
        assert_eq!(insert.partition.len(), 2);
        assert_eq!(insert.partition[1], c("b"));
        assert_eq!(insert.columns, vec![common::ident("c")]);
        assert!(insert.columns_after_partition);
    }

    #[test]
    fn columns_before_the_partition_are_recorded() {
        let insert = spark_insert("INSERT INTO t (c) PARTITION (a) SELECT 1");
        assert!(!insert.overwrite);
        assert!(insert.into);
        assert!(!insert.table_keyword);
        assert!(!insert.columns_after_partition);
    }

    #[test]
    fn directory_keeps_its_parts() {
        let insert = directory("INSERT OVERWRITE LOCAL DIRECTORY '/x' STORED AS ORC SELECT 1");
        assert!(insert.local);
        assert_eq!(insert.path.text, "/x");
        assert_eq!(insert.stored_as.unwrap().text, "ORC");
        assert!(!directory("INSERT OVERWRITE DIRECTORY '/x' SELECT 1").local);
    }

    #[test]
    fn cache_keeps_its_parts() {
        match one("CACHE LAZY TABLE 'n' OPTIONS('a' = 1, 'b' = 2) AS SELECT 1").kind {
            StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::Cache(cache))) => {
                assert_eq!(cache.flag.unwrap().text, "LAZY");
                assert_eq!(cache.table.parts[0].text, "n");
                assert_eq!(cache.table.parts[0].quote, Some('\''));
                assert_eq!(cache.options.len(), 2);
                assert!(cache.has_as);
                assert!(cache.query.is_some());
            }
            other => panic!("{other:?}"),
        }
        match one("CACHE TABLE t").kind {
            StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::Cache(cache))) => {
                assert_eq!(cache.flag, None);
                assert!(!cache.has_as);
                assert!(cache.query.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn uncache_keeps_its_parts() {
        match one("UNCACHE TABLE IF EXISTS db.t").kind {
            StatementKind::Ext(BuiltinStatement::Spark(SparkStatement::Uncache {
                table,
                if_exists,
            })) => {
                assert_eq!(table.to_string(), "db.t");
                assert!(if_exists);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_table_named_directory_is_still_a_table() {
        assert_eq!(
            spark_insert("INSERT OVERWRITE directory SELECT 1")
                .table
                .to_string(),
            "directory"
        );
    }
}

#[test]
fn double_colon_casts_round_trip() {
    round_trips("SELECT a::INT");
    round_trips("SELECT a::INT::STRING");
    round_trips("SELECT a::ARRAY<INT>");
    round_trips("SELECT (a + 1)::VARCHAR(10)");
}

#[test]
fn interval_fields_are_rejected_not_read_as_aliases() {
    round_trips("SELECT a::INTERVAL");
    round_trips("SELECT a::INTERVAL AS day");
    for field in ["YEAR", "MONTH", "DAY", "HOUR", "MINUTE", "SECOND"] {
        assert!(rejected::<Spark>(&format!("SELECT a::INTERVAL {field}")));
    }
    assert!(rejected::<Spark>("SELECT a::INTERVAL YEAR TO MONTH"));
    assert!(rejected::<Spark>("SELECT CAST(a AS INTERVAL HOUR)"));
}

#[test]
fn double_colon_binds_tighter_than_arithmetic() {
    assert_eq!(
        expr("a + b::INT"),
        bin(c("a"), Add, cast(c("b"), "INT", CastStyle::DoubleColon))
    );
    assert_eq!(
        expr("a * b::INT"),
        bin(c("a"), Mul, cast(c("b"), "INT", CastStyle::DoubleColon))
    );
}

#[test]
fn a_single_colon_is_still_a_parameter_or_field() {
    round_trips("SELECT :name");
    round_trips("SELECT CAST(a AS STRUCT<b: INT>)");
    assert!(rejected::<Core>("SELECT a::INT"));
}

#[test]
fn ilike_round_trips() {
    round_trips("SELECT a ILIKE 'b'");
    round_trips("SELECT a NOT ILIKE 'b'");
    round_trips("SELECT a ILIKE 'b' ESCAPE '^'");
    round_trips("SELECT a NOT ILIKE 'b' ESCAPE '^' IS NULL");
    assert!(rejected::<Core>("SELECT a ILIKE 'b'"));
}

#[test]
fn group_by_all_round_trips() {
    round_trips("SELECT a, SUM(b) FROM t GROUP BY ALL");
    round_trips("SELECT a FROM t GROUP BY ALL WITH ROLLUP");
    round_trips("SELECT a FROM t GROUP BY ALL HAVING a > 1");
}

#[test]
fn group_by_all_stands_alone() {
    assert!(rejected::<Spark>("SELECT a FROM t GROUP BY ALL, b"));
    assert!(rejected::<Spark>(
        "SELECT a FROM t GROUP BY ALL WITH TOTALS"
    ));
    assert!(rejected::<Core>("SELECT a FROM t GROUP BY ALL"));
}

#[test]
fn group_by_all_is_recorded() {
    let parsed = common::into_select(
        sqlparser::parse_query::<Spark>("SELECT a FROM t GROUP BY ALL").unwrap(),
    );
    assert!(parsed.group_by_all);
    assert!(parsed.group_by.is_empty());
    let parsed =
        common::into_select(sqlparser::parse_query::<Spark>("SELECT a FROM t GROUP BY a").unwrap());
    assert!(!parsed.group_by_all);
    assert_eq!(parsed.group_by.len(), 1);
}

#[test]
fn cache_table_round_trips() {
    round_trips("CACHE TABLE t");
    round_trips("CACHE TABLE db.t");
    round_trips("CACHE TABLE 'name'");
    round_trips("CACHE LAZY TABLE t");
    round_trips("CACHE TABLE t AS SELECT 1");
    round_trips("CACHE TABLE t SELECT 1");
    round_trips("CACHE TABLE t WITH c AS (SELECT 1) SELECT * FROM c");
    round_trips("CACHE LAZY TABLE t OPTIONS('a' = 'b', 'c' = 0.5)");
    round_trips("CACHE LAZY TABLE t OPTIONS('a' = 1) AS SELECT 1");
}

#[test]
fn uncache_table_round_trips() {
    round_trips("UNCACHE TABLE t");
    round_trips("UNCACHE TABLE 'name'");
    round_trips("UNCACHE TABLE IF EXISTS db.t");
}

#[test]
fn malformed_cache_statements_are_rejected() {
    assert!(rejected::<Spark>("CACHE t"));
    assert!(rejected::<Spark>("CACHE TABLE"));
    assert!(rejected::<Spark>("CACHE TABLE t AS"));
    assert!(rejected::<Spark>("CACHE TABLE t OPTIONS"));
    assert!(rejected::<Spark>("CACHE TABLE t x"));
    assert!(rejected::<Spark>("UNCACHE t"));
    assert!(rejected::<Spark>("UNCACHE TABLE IF t"));
    assert!(rejected::<Core>("CACHE TABLE t"));
    assert!(rejected::<Core>("UNCACHE TABLE t"));
}
