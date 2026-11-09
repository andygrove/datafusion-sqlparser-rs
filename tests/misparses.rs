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

//! SQL that must never be silently misunderstood.

mod common;

use common::*;
use sqlparser::ast::BinaryOp::*;
use sqlparser::ast::ExprKind;
use sqlparser::ast::SelectItemKind;
use sqlparser::ast::UnaryOp::*;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::Builtin;
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift, Snowflake, Spark};
use sqlparser::parser::Parser;
use sqlparser::{parse_expr, parse_query, parse_sql};

type E<'a> = Expr<'a, Builtin>;

fn expr(sql: &str) -> E<'_> {
    parse_expr::<Core>(sql).unwrap()
}

fn rejected<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

fn rendered<D: Dialect>(sql: &str) -> String {
    parse_sql::<D>(sql).unwrap()[0].to_string()
}

// Line comments

#[test]
fn line_comment_runs_to_the_end_of_the_line() {
    assert_eq!(expr("a -- comment\n + b"), bin(col("a"), Add, col("b")));
}

#[test]
fn line_comment_can_end_the_input() {
    assert_eq!(expr("a -- comment"), col("a"));
}

#[test]
fn comment_does_not_become_subtraction() {
    assert_eq!(expr("a --1"), col("a"));
}

#[test]
fn separated_minus_signs_are_still_operators() {
    assert_eq!(expr("a - -1"), bin(col("a"), Sub, unary(Neg, num("1"))));
}

#[test]
fn comment_inside_a_statement_is_ignored() {
    assert_eq!(
        rendered::<Core>("UPDATE t SET a = a -- note\n WHERE b = 5"),
        "UPDATE t SET a = a WHERE b = 5"
    );
}

#[test]
fn input_of_only_comments_has_no_statements() {
    assert!(
        parse_sql::<Core>("-- one\n/* two */ -- three")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn comment_markers_inside_quotes_are_text() {
    let ExprKind::Literal(Literal::String(string)) = expr("'a -- b /* c */'").kind else {
        panic!("expected a string");
    };
    assert_eq!(string.text, "a -- b /* c */");
    assert_eq!(
        expr("\"a--b\""),
        ExprKind::Ident(quoted("a--b", '"')).into()
    );
}

// Block comments

#[test]
fn block_comment_can_sit_between_tokens() {
    assert_eq!(expr("a /* c */ + /* d */ b"), bin(col("a"), Add, col("b")));
}

#[test]
fn block_comment_can_span_lines() {
    assert_eq!(expr("a /* line\nline */ + b"), bin(col("a"), Add, col("b")));
}

#[test]
fn block_comment_does_not_need_surrounding_spaces() {
    assert_eq!(expr("a/*c*/+b"), bin(col("a"), Add, col("b")));
}

#[test]
fn division_is_not_a_comment() {
    assert_eq!(expr("a / b"), bin(col("a"), Div, col("b")));
}

#[test]
fn unterminated_block_comment_reports_where_it_starts() {
    let err = parse_expr::<Core>("a /* oops").unwrap_err();
    assert_eq!(err.span.start, 2);
}

#[test]
fn block_comments_nest_by_default() {
    assert_eq!(
        expr("a /* x /* y */ z */ + b"),
        bin(col("a"), Add, col("b"))
    );
    assert!(parse_expr::<Postgres>("a /* x /* y */ z */").is_ok());
}

#[test]
fn unbalanced_nested_comment_is_unterminated_by_default() {
    assert!(parse_expr::<Core>("a /* x /* y */").is_err());
}

#[test]
fn mysql_and_redshift_block_comments_do_not_nest() {
    assert!(parse_expr::<MySql>("a /* x /* y */").is_ok());
    assert!(parse_expr::<MySql>("a /* x /* y */ z */").is_err());
    assert!(parse_expr::<Redshift>("a /* x /* y */").is_ok());
}

// MySQL comment rules

fn my(sql: &str) -> E<'_> {
    parse_expr::<MySql>(sql).unwrap()
}

#[test]
fn mysql_double_dash_needs_whitespace_to_be_a_comment() {
    assert_eq!(my("a--1"), bin(col("a"), Sub, unary(Neg, num("1"))));
    assert_eq!(my("a-- 1"), col("a"));
}

#[test]
fn mysql_double_dash_at_the_end_of_a_line_is_a_comment() {
    assert_eq!(my("a--"), col("a"));
    assert_eq!(my("a--\n + b"), bin(col("a"), Add, col("b")));
}

#[test]
fn mysql_hash_starts_a_comment() {
    assert_eq!(my("a # comment\n + b"), bin(col("a"), Add, col("b")));
}

#[test]
fn hash_is_not_a_comment_elsewhere() {
    assert!(parse_expr::<Core>("a # comment").is_err());
    // In PostgreSQL `#` is the bitwise XOR operator, so this is `a # comment`.
    assert!(parse_expr::<Postgres>("a # comment").is_ok());
}

// Numbers

#[test]
fn exponents_are_part_of_the_number() {
    for sql in ["1e5", "15E29", "1e+5", "1E-5", "1.5e-3"] {
        assert_eq!(expr(sql), num(sql), "{sql}");
    }
}

#[test]
fn letters_directly_after_a_number_are_rejected() {
    for sql in ["SELECT 1a", "SELECT 1e", "SELECT 1e+", "SELECT 10_000"] {
        assert!(rejected::<Core>(sql), "{sql}");
    }
}

#[test]
fn alias_after_a_number_needs_a_space() {
    let parsed = parse_query::<Core>("SELECT 1 a").unwrap();
    assert_eq!(
        body(&parsed).projection,
        vec![
            SelectItemKind::Expr {
                expr: num("1"),
                alias: Some(ident("a")),
            }
            .into()
        ]
    );
}

// SELECT quantifiers

fn select(sql: &str) -> Select<'_, Builtin> {
    into_select(parse_query::<Core>(sql).unwrap())
}

#[test]
fn distinct_is_a_quantifier_not_a_column() {
    let parsed = select("SELECT DISTINCT name FROM t");
    assert_eq!(parsed.quantifier, SetQuantifier::Distinct);
    assert_eq!(
        parsed.projection,
        vec![
            SelectItemKind::Expr {
                expr: col("name"),
                alias: None,
            }
            .into()
        ]
    );
}

#[test]
fn all_is_a_quantifier_not_a_column() {
    let parsed = select("SELECT ALL name FROM t");
    assert_eq!(parsed.quantifier, SetQuantifier::All);
    assert_eq!(parsed.projection.len(), 1);
}

#[test]
fn select_has_no_quantifier_by_default() {
    assert_eq!(select("SELECT a").quantifier, SetQuantifier::None);
}

#[test]
fn distinct_requires_a_projection() {
    assert!(rejected::<Core>("SELECT DISTINCT FROM t"));
}

#[test]
fn quantifiers_round_trip() {
    for sql in ["SELECT DISTINCT a, b FROM t", "SELECT ALL a"] {
        assert_eq!(rendered::<Core>(sql), sql);
    }
}

#[test]
fn redshift_top_can_precede_distinct() {
    let sql = "SELECT TOP 3 DISTINCT a FROM t";
    assert_eq!(rendered::<Redshift>(sql), sql);
}

// NATURAL JOIN

fn only_join(sql: &str) -> Join<'_, Builtin> {
    let mut from = select(sql).from;
    from.remove(0).joins.remove(0)
}

#[test]
fn natural_join_is_a_join_not_an_alias() {
    let parsed = select("SELECT 1 FROM a NATURAL JOIN b");
    assert_eq!(parsed.from[0].relation.alias, None);
    assert_eq!(parsed.from[0].joins[0].constraint, JoinConstraint::Natural);
    assert_eq!(parsed.from[0].joins[0].kind, JoinKind::Inner);
}

#[test]
fn natural_combines_with_join_kinds() {
    let cases = [
        ("SELECT 1 FROM a NATURAL INNER JOIN b", JoinKind::Inner),
        ("SELECT 1 FROM a NATURAL LEFT JOIN b", JoinKind::Left),
        ("SELECT 1 FROM a NATURAL LEFT OUTER JOIN b", JoinKind::Left),
        ("SELECT 1 FROM a NATURAL RIGHT JOIN b", JoinKind::Right),
        ("SELECT 1 FROM a NATURAL FULL JOIN b", JoinKind::Full),
    ];
    for (sql, kind) in cases {
        assert_eq!(only_join(sql).kind, kind, "{sql}");
        assert_eq!(only_join(sql).constraint, JoinConstraint::Natural, "{sql}");
    }
}

#[test]
fn natural_joined_table_may_have_an_alias() {
    let join = only_join("SELECT 1 FROM a NATURAL JOIN b AS c");
    assert_eq!(join.table.alias, Some(explicit_alias("c")));
}

#[test]
fn natural_requires_join() {
    assert!(rejected::<Core>("SELECT 1 FROM a NATURAL"));
    assert!(rejected::<Core>("SELECT 1 FROM a NATURAL b"));
}

#[test]
fn natural_cross_join_is_rejected() {
    assert!(rejected::<Core>("SELECT 1 FROM a NATURAL CROSS JOIN b"));
}

#[test]
fn natural_join_rejects_a_constraint() {
    assert!(rejected::<Core>("SELECT 1 FROM a NATURAL JOIN b ON x"));
    assert!(rejected::<Core>("SELECT 1 FROM a NATURAL JOIN b USING (x)"));
}

#[test]
fn join_spelling_does_not_change_the_join_kind() {
    assert_eq!(
        only_join("SELECT 1 FROM a INNER JOIN b").kind,
        only_join("SELECT 1 FROM a JOIN b").kind
    );
    assert_eq!(
        only_join("SELECT 1 FROM a LEFT OUTER JOIN b").kind,
        only_join("SELECT 1 FROM a LEFT JOIN b").kind
    );
}

#[test]
fn natural_joins_round_trip() {
    for sql in [
        "SELECT 1 FROM a NATURAL JOIN b",
        "SELECT 1 FROM a NATURAL LEFT JOIN b AS c",
        "SELECT 1 FROM a NATURAL FULL JOIN b JOIN c ON x",
    ] {
        assert_eq!(rendered::<Core>(sql), sql);
    }
}

// Words reserved by particular dialects

#[test]
fn redshift_does_not_read_only_as_a_table_name() {
    for sql in [
        "SELECT * FROM ONLY t",
        "UPDATE ONLY t SET a = 1",
        "DELETE FROM ONLY t",
    ] {
        assert!(rejected::<Redshift>(sql), "{sql}");
    }
}

#[test]
fn only_remains_an_ordinary_name_in_mysql() {
    assert!(!rejected::<MySql>("SELECT * FROM ONLY t"));
}

#[test]
fn redshift_does_not_read_null_tests_as_aliases() {
    for sql in ["SELECT a NOTNULL", "SELECT a ISNULL"] {
        assert!(rejected::<Redshift>(sql), "{sql}");
    }
}

#[test]
fn mysql_does_not_read_select_modifiers_as_columns() {
    for word in [
        "HIGH_PRIORITY",
        "STRAIGHT_JOIN",
        "SQL_CALC_FOUND_ROWS",
        "SQL_SMALL_RESULT",
        "SQL_BIG_RESULT",
    ] {
        let sql = format!("SELECT DISTINCT {word} a FROM t");
        let statements = sqlparser::parse_sql::<MySql>(&sql).unwrap();
        assert_eq!(statements[0].to_string(), sql);
    }
}

#[test]
fn mysql_reserved_words_stay_ordinary_names_elsewhere() {
    assert!(!rejected::<Core>("SELECT high_priority FROM t"));
    assert!(!rejected::<Postgres>("SELECT high_priority FROM t"));
}

// Unquoted interval values

fn assert_interval<D: Dialect<Ext = Builtin>>(sql: &str) {
    assert!(
        matches!(parse_expr::<D>(sql).unwrap().kind, ExprKind::Interval(_)),
        "{sql}"
    );
}

#[test]
fn signed_interval_value_is_an_interval_not_a_subtraction() {
    assert_interval::<Spark>("INTERVAL -1 DAY");
    assert_interval::<Spark>("INTERVAL +1 DAY");
    assert_interval::<MySql>("INTERVAL -1 DAY");
    assert_interval::<Snowflake>("INTERVAL -1 DAY");
    assert_eq!(
        rendered::<Spark>("SELECT INTERVAL -1 DAY FROM t"),
        "SELECT INTERVAL -1 DAY FROM t"
    );
}

#[test]
fn parenthesized_interval_value_is_an_interval_not_a_call() {
    assert_interval::<Spark>("INTERVAL (1) DAY");
    assert_interval::<MySql>("INTERVAL (1 + (2)) DAY");
    assert_interval::<Snowflake>("INTERVAL (1) DAY");
}

#[test]
fn interval_without_a_unit_can_still_be_a_column_or_function() {
    assert!(matches!(
        parse_expr::<Spark>("interval - 1").unwrap().kind,
        ExprKind::Binary { .. }
    ));
    assert!(matches!(
        parse_expr::<Spark>("interval (1)").unwrap().kind,
        ExprKind::Function { .. }
    ));
    assert_eq!(
        rendered::<Spark>("SELECT interval - 1 AS day FROM t"),
        "SELECT interval - 1 AS day FROM t"
    );
}

// Snowflake declarations

#[test]
fn declare_stops_before_every_kind_of_statement() {
    for next in [
        "SHOW TABLES",
        "TRUNCATE t",
        "CALL p()",
        "COMMIT",
        "ROLLBACK",
        "LIST @s",
        "GRANT SELECT ON t TO r",
        "REVOKE SELECT ON t FROM r",
        "DESCRIBE TABLE t",
        "EXPLAIN SELECT 1",
        "PUT 'f' @s",
        "REMOVE @s",
    ] {
        let sql = format!("DECLARE a INT; {next}");
        assert_eq!(
            parse_sql::<Snowflake>(&sql).ok().map(|s| s.len()),
            Some(2),
            "{sql}"
        );
    }
}

#[test]
fn declare_still_reads_several_variables() {
    let statements = parse_sql::<Snowflake>("DECLARE a INT; b INT; SHOW TABLES").unwrap();
    assert_eq!(statements.len(), 2);
    assert!(statements[0].to_string().contains("b INT"));
}

// Clauses after a table sample

#[test]
fn offset_after_a_sample_belongs_to_the_query() {
    let query = parse_query::<Spark>("SELECT * FROM t TABLESAMPLE (10) OFFSET 5").unwrap();
    assert!(query.limit.is_some());
    let query = parse_query::<Snowflake>("SELECT * FROM t SAMPLE (10) OFFSET 5").unwrap();
    assert!(query.limit.is_some());
}

// Dollar-quoted comments

#[test]
fn dollar_quoted_comment_with_a_quote_keeps_its_dollars() {
    for sql in [
        "CREATE TABLE t (a INT) COMMENT = $$x'; DROP TABLE y; --$$",
        "CREATE TABLE t (a INT COMMENT $$it's$$)",
        "CREATE TABLE t (a INT) COMMENT = $$back\\slash$$",
    ] {
        let once = rendered::<Snowflake>(sql);
        assert_eq!(once, sql);
        assert_eq!(rendered::<Snowflake>(&once), sql);
    }
}

#[test]
fn plain_dollar_quoted_comment_renders_as_a_string() {
    assert_eq!(
        rendered::<Snowflake>("CREATE TABLE t (a INT) COMMENT = $$plain$$"),
        "CREATE TABLE t (a INT) COMMENT = 'plain'"
    );
}

#[test]
fn tagged_dollar_quoted_comment_with_a_quote_is_rejected() {
    assert!(rejected::<Snowflake>(
        "CREATE TABLE t (a INT) COMMENT = $a$it's$a$"
    ));
}

// Spark operator precedence

#[test]
fn spark_concat_binds_like_addition() {
    assert_eq!(
        parse_expr::<Spark>("a || b * c").unwrap(),
        bin(col("a"), StringConcat, bin(col("b"), Mul, col("c")))
    );
    assert_eq!(
        parse_expr::<Spark>("a * b || c").unwrap(),
        bin(bin(col("a"), Mul, col("b")), StringConcat, col("c"))
    );
    assert_eq!(
        parse_expr::<Spark>("a + b || c").unwrap(),
        bin(bin(col("a"), Add, col("b")), StringConcat, col("c"))
    );
}

#[test]
fn spark_between_upper_bound_can_be_a_comparison() {
    let ExprKind::Between { high, .. } = parse_expr::<Spark>("a BETWEEN 1 AND 2 = b").unwrap().kind
    else {
        panic!("not a BETWEEN");
    };
    assert_eq!(*high, bin(num("2"), Eq, col("b")));
}

#[test]
fn between_upper_bound_stops_before_a_comparison_elsewhere() {
    assert!(matches!(
        parse_expr::<Core>("a BETWEEN 1 AND 2 = b").unwrap().kind,
        ExprKind::Binary { op: Eq, .. }
    ));
}

// Column lists

#[test]
fn second_column_list_is_rejected() {
    assert!(rejected::<Snowflake>("CREATE TABLE t (a INT) (b INT)"));
    assert!(parse_sql::<Snowflake>("CREATE TABLE t () (b INT)").is_ok());
}

// Parenthesized samples

#[test]
fn aliased_parenthesized_sample_renders_sql_that_parses_the_same() {
    let sql = "SELECT * FROM (t SAMPLE (10)) AS x";
    let once = rendered::<Snowflake>(sql);
    assert_eq!(rendered::<Snowflake>(&once), once);
}

// Hex literals

#[test]
fn postgres_hex_integer_is_a_number() {
    assert_eq!(rendered::<Postgres>("SELECT 0x1F"), "SELECT 0x1F");
    assert_eq!(rendered::<Postgres>("SELECT X'1F'"), "SELECT X'1F'");
    assert_eq!(rendered::<MySql>("SELECT 0x1F"), "SELECT X'1F'");
}

// Clauses that need what comes before them

#[test]
fn with_rollup_needs_a_group_by() {
    assert!(rejected::<MySql>("SELECT a FROM t WITH ROLLUP"));
    assert!(rejected::<Spark>("SELECT a FROM t WITH ROLLUP"));
    assert!(parse_sql::<MySql>("SELECT a FROM t GROUP BY a WITH ROLLUP").is_ok());
}

#[test]
fn grantee_has_at_most_a_known_kind() {
    assert!(rejected::<Core>("GRANT SELECT ON t TO a b"));
    for sql in [
        "GRANT SELECT ON t TO ROLE b",
        "GRANT SELECT ON t TO USER b",
        "GRANT SELECT ON t TO SHARE b",
        "GRANT SELECT ON t TO GROUP b",
        "GRANT SELECT ON t TO DATABASE ROLE b",
        "GRANT SELECT ON t TO APPLICATION ROLE b",
        "GRANT SELECT ON t TO APPLICATION b",
    ] {
        assert_eq!(rendered::<Snowflake>(sql), sql);
    }
}

#[test]
fn spark_option_key_is_not_an_expression() {
    assert!(rejected::<Spark>(
        "CREATE TABLE t (a INT) USING p OPTIONS (k -1)"
    ));
    assert!(parse_sql::<Spark>("CREATE TABLE t (a INT) USING p OPTIONS (k 1)").is_ok());
    assert!(parse_sql::<Spark>("CREATE TABLE t (a INT) USING p OPTIONS (a.b = 1)").is_ok());
}

// Spark clauses that apply to a whole query

#[test]
fn sort_by_after_a_union_applies_to_the_whole_query() {
    let query = parse_query::<Spark>("SELECT a FROM t UNION SELECT b FROM u SORT BY a").unwrap();
    assert!(query.distribution.is_some());
    assert!(matches!(query.body, SetExpr::SetOperation { .. }));
}

#[test]
fn distribution_clauses_follow_order_by() {
    let sql = "SELECT a FROM t ORDER BY a CLUSTER BY b DISTRIBUTE BY c SORT BY d LIMIT 1";
    assert_eq!(rendered::<Spark>(sql), sql);
    assert!(rejected::<Spark>("SELECT a FROM t SORT BY a ORDER BY a"));
}

// Syntax of one database stays in that database

#[test]
fn snowflake_sample_syntax_is_not_spark_syntax() {
    for sql in [
        "SELECT 1 FROM t TABLESAMPLE BERNOULLI (10)",
        "SELECT 1 FROM t TABLESAMPLE (10) SEED (1)",
        "SELECT 1 FROM t TABLESAMPLE (10) REPEATABLE (1)",
        "SELECT 1 FROM t PIVOT(SUM(a) FOR b IN (ANY ORDER BY b))",
        "SELECT 1 FROM t PIVOT(SUM(a) FOR b IN (SELECT 1))",
    ] {
        assert!(rejected::<Spark>(sql), "{sql}");
    }
}

#[test]
fn spark_buckets_are_not_snowflake_syntax() {
    let sql = "SELECT 1 FROM t TABLESAMPLE (BUCKET 1 OUT OF 4)";
    assert!(rejected::<Snowflake>(sql));
    assert_eq!(rendered::<Spark>(sql), sql);
}

#[test]
fn snowflake_keeps_its_sample_and_pivot_syntax() {
    for sql in [
        "SELECT 1 FROM t SAMPLE BERNOULLI (10)",
        "SELECT 1 FROM t SAMPLE (10) SEED (1)",
        "SELECT 1 FROM t PIVOT(SUM(a) FOR b IN (ANY))",
        "SELECT 1 FROM t PIVOT(SUM(a) FOR b IN (SELECT 1))",
    ] {
        assert_eq!(rendered::<Snowflake>(sql), sql);
    }
}

#[test]
fn postgres_infix_rule_includes_its_null_tests() {
    let parser = Parser::<Postgres>::new("ISNULL").unwrap();
    assert!(sqlparser::dialect::postgres::infix_rule(&parser).is_some());
}
