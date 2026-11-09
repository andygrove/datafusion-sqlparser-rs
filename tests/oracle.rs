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

//! Oracle Database: quote-delimited strings, optimizer hints, hierarchical
//! queries and the forms of `INSERT` and `DELETE` that only it has.

mod common;

use sqlparser::dialect::Oracle;
use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::parse_sql;

fn round_trips(sql: &str) {
    let statements = parse_sql::<Oracle>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<Oracle>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements[0].to_string(), expected);
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Oracle>(sql).is_err(), "accepted: {sql}");
}

#[test]
fn can_be_chosen_at_runtime() {
    let kind = DialectKind::from_name("Oracle").unwrap();
    assert_eq!(parse_sql_as(kind, "SELECT 1 FROM dual").unwrap().len(), 1);
}

#[test]
fn concatenation_binds_like_addition() {
    round_trips("SELECT 3 / 5 || 'asdf' || 7 * 9 FROM dual");
    round_trips("SELECT 3 + 5 || '.3' || 7 - 9 FROM dual");
}

#[test]
fn quote_delimited_strings_round_trip() {
    round_trips(
        "SELECT Q'.abc.', Q'Xab'cX', Q'|abc'''|', Q'{abc}d}', Q'[]abc[]', Q'<a'bc>', Q'<<<a'bc>', Q'('abc'('abc)', Q'(abc'def))', Q'(abc'def)))' FROM dual",
    );
    round_trips("SELECT NQ'.abc.' FROM dual");
    parses_to(
        "select q'!a'b'c!d!' from dual",
        "SELECT Q'!a'b'c!d!' FROM dual",
    );
    for prefix in ["nq", "Nq", "nQ", "NQ"] {
        parses_to(
            &format!("select {prefix}'!a'b'c!d!' from dual"),
            "SELECT NQ'!a'b'c!d!' FROM dual",
        );
    }
}

#[test]
fn quote_delimiters_can_be_any_character() {
    round_trips("SELECT Q'éabcé' FROM dual");
}

#[test]
fn invalid_quote_delimited_strings_are_rejected() {
    for delimiter in [' ', '\t', '\r', '\n'] {
        rejected(&format!("SELECT Q'{delimiter}abc{delimiter}' FROM dual"));
    }
    rejected("SELECT Q'");
    rejected("SELECT Q'|asdfa....");
    rejected("SELECT NQ'");
}

#[test]
fn q_and_nq_stay_ordinary_words() {
    round_trips("SELECT q, quux, q.abc FROM dual q");
    round_trips("SELECT nq, nqoo, nq.abc FROM dual q");
}

#[test]
fn quote_delimited_strings_are_not_names() {
    rejected("SELECT 1 Q'.x.' FROM dual");
    rejected("SELECT a.Q'.x.' FROM dual");
    rejected("SELECT * FROM Q'.x.'");
}

#[test]
fn optimizer_hints_round_trip() {
    round_trips("SELECT /*+one two three*/ /*+four five six*/ 1 FROM dual");
    parses_to(
        "SELECT /*one two three*/ /*+four five six*/ 1 FROM dual",
        "SELECT /*+four five six*/ 1 FROM dual",
    );
    round_trips("SELECT --+ one two three /* asdf */\n 1 FROM dual");
    round_trips("SELECT --abc+ text\n 1 FROM dual");
    round_trips("INSERT /*+ append */ INTO t1 SELECT * FROM all_objects");
    round_trips("UPDATE /*+ DISABLE_PARALLEL_DML */ table_name SET column1 = 1");
    round_trips("DELETE --+ ENABLE_PARALLEL_DML\n FROM table_name");
    round_trips(
        "MERGE /*+ CLUSTERING */ INTO people_target pt USING people_source ps ON (pt.person_id = ps.person_id) WHEN NOT MATCHED THEN INSERT (pt.person_id, pt.first_name, pt.last_name, pt.title) VALUES (ps.person_id, ps.first_name, ps.last_name, ps.title)",
    );
}

#[test]
fn hints_come_before_distinct_only() {
    round_trips("SELECT /*+ x */ DISTINCT a FROM t");
    parses_to(
        "SELECT DISTINCT /*+ x */ a FROM t",
        "SELECT DISTINCT a FROM t",
    );
}

#[test]
fn comments_with_other_text_before_the_plus_are_not_hints() {
    parses_to("SELECT /* a b+c */ 1 FROM dual", "SELECT 1 FROM dual");
    parses_to("SELECT /*a+ */ 1 FROM dual", "SELECT /*a+ */ 1 FROM dual");
}

#[test]
fn hints_elsewhere_stay_comments() {
    parses_to(
        "SELECT 1 /*+ late */ FROM /*+ later */ dual",
        "SELECT 1 FROM dual",
    );
}

#[test]
fn hierarchical_queries() {
    round_trips(
        r#"SELECT last_name AS "Employee", CONNECT_BY_ISCYCLE AS "Cycle", LEVEL, SYS_CONNECT_BY_PATH(last_name, '/') AS "Path" FROM employees WHERE level <= 3 AND department_id = 80 START WITH last_name = 'King' CONNECT BY NOCYCLE PRIOR employee_id = manager_id AND LEVEL <= 4 ORDER BY "Employee", "Cycle", LEVEL, "Path""#,
    );
    round_trips(
        r#"SELECT last_name AS "Employee", CONNECT_BY_ROOT last_name AS "Manager", LEVEL - 1 AS "Pathlen", SYS_CONNECT_BY_PATH(last_name, '/') AS "Path" FROM employees WHERE LEVEL > 1 AND department_id = 110 CONNECT BY PRIOR employee_id = manager_id ORDER BY "Employee", "Manager", "Pathlen", "Path""#,
    );
    round_trips(
        "SELECT employee_id FROM employees CONNECT BY manager_id = PRIOR employee_id START WITH title = 'president'",
    );
}

#[test]
fn insert_with_a_table_alias() {
    round_trips("INSERT INTO foo_t t SELECT 1, 2, 3 FROM dual");
    round_trips("INSERT INTO foo_t asdf (a, b, c) SELECT 1, 2, 3 FROM dual");
    round_trips("INSERT INTO foo_t t (a, b, c) VALUES (1, 2, 3)");
    round_trips("INSERT INTO foo_t t VALUES (1, 2, 3)");
    round_trips("INSERT INTO foo_t t (t.id, t.val) SELECT 1, 2 FROM dual");
}

#[test]
fn insert_without_an_alias() {
    rejected("INSERT INTO t default SELECT 'a' FROM dual");
    round_trips("INSERT INTO t SELECT 'a' FROM dual");
    round_trips("INSERT INTO dual WITH w AS (SELECT 1 AS y FROM dual) SELECT y FROM w");
    round_trips("INSERT INTO t VALUES (1)");
}

#[test]
fn insert_into_a_query() {
    round_trips(
        "INSERT INTO (SELECT employee_id, last_name FROM employees) VALUES (207, 'Gregory')",
    );
    round_trips(
        "INSERT INTO (SELECT foo_t.id, foo_t.val FROM foo_t) (id, val) SELECT 10, 20 FROM dual",
    );
    round_trips("INSERT INTO (SELECT foo_t.id, foo_t.val FROM foo_t) SELECT 10, 20 FROM dual");
    round_trips(
        "INSERT INTO (WITH cte AS (SELECT 1 AS id, 2 AS val FROM dual) SELECT foo_t.id, foo_t.val FROM foo_t) SELECT 10, 20 FROM dual",
    );
}

#[test]
fn returning_is_accepted_on_dml() {
    round_trips("INSERT INTO t VALUES (1) RETURNING a");
    round_trips("UPDATE t SET a = 1 RETURNING a, b");
    round_trips("DELETE FROM t WHERE a = 1 RETURNING a");
}

#[test]
fn delete_without_from() {
    round_trips("DELETE t WHERE a = 1");
    round_trips("DELETE t");
    round_trips(r#"DELETE "table" WHERE 1"#);
    round_trips("DELETE /*+ x */ t WHERE a = 1");
    rejected("DELETE WHERE a = 1");
}

#[test]
fn limit_is_not_reserved_for_tables() {
    round_trips("SELECT * FROM limit");
    round_trips("SELECT * FROM t JOIN limit ON TRUE");
}

#[test]
fn boolean_words_are_names() {
    round_trips("SELECT TRUE, FALSE, true FROM dual");
    round_trips("SELECT * FROM t WHERE true = 1");
}

#[test]
fn outer_join_operator() {
    round_trips("SELECT t1.c1, t2.c2 FROM t1, t2 WHERE t1.c1 = t2.c2 (+)");
    round_trips("SELECT 1 FROM T WHERE a = b (+)");
    round_trips("SELECT 1 FROM T WHERE t1.c1 = t2.c2.d3 (+)");
    rejected("SELECT 1 FROM T WHERE 1 (+) = 2");
}

#[test]
fn execute_immediate() {
    round_trips("EXECUTE IMMEDIATE 'SELECT 1'");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' INTO a, b");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' USING 1 AS x, y");
    round_trips("EXECUTE IMMEDIATE 'SELECT 1' INTO a USING 1 AS b");
    round_trips("EXECUTE IMMEDIATE v_sql");
    rejected("EXECUTE IMMEDIATE");
}

#[test]
fn match_recognize() {
    round_trips(
        "SELECT * FROM my_table MATCH_RECOGNIZE(PATTERN (^ S1 S2*? ( {- S3 -} S4 )+ | PERMUTE(S1, S2){1,2} $) DEFINE DUMMY AS 1 = 1)",
    );
    round_trips(
        "SELECT * FROM my_table MATCH_RECOGNIZE(PATTERN (S1{1} | S2{2,3}) DEFINE DUMMY AS 1 = 1)",
    );
    rejected("SELECT * FROM tbl MATCH_RECOGNIZE");
}

#[test]
fn null_treatment_in_arguments() {
    round_trips(
        "SELECT FIRST_VALUE(a IGNORE NULLS) OVER (), FIRST_VALUE(b RESPECT NULLS) OVER () FROM mytable",
    );
    round_trips(
        "SELECT FIRST_VALUE(column2 IGNORE NULLS) OVER (PARTITION BY column1 ORDER BY column2) FROM some_table",
    );
    round_trips("SELECT FIRST_VALUE(a) IGNORE NULLS OVER () FROM t");
}

#[test]
fn comment_on() {
    round_trips("COMMENT ON COLUMN tab.name IS 'comment'");
    round_trips("COMMENT ON TABLE public.tab IS NULL");
}

#[test]
fn create_table_with_select() {
    parses_to(
        "CREATE TABLE foo (baz INT) SELECT bar",
        "CREATE TABLE foo (baz INT) AS SELECT bar",
    );
    parses_to(
        "CREATE TABLE foo SELECT bar FROM baz",
        "CREATE TABLE foo AS SELECT bar FROM baz",
    );
    round_trips("CREATE TABLE foo AS SELECT bar FROM baz");
}

#[test]
fn session_parameters_without_an_operator() {
    round_trips("SET DEFINE OFF");
    round_trips("SET a b");
    round_trips("SET ANSI_NULLS, ANSI_PADDING ON");
    round_trips("SET ROWCOUNT 4");
    round_trips("SET DEADLOCK_PRIORITY -8");
    round_trips("SET a = 1");
    round_trips("SET TIME ZONE 'UTC'");
    rejected("SET");
    rejected("SET a");
}

#[test]
fn grouping_expressions() {
    round_trips("SELECT a, b FROM t GROUP BY GROUPING SETS ((a), (b), (c))");
    round_trips("SELECT a FROM t GROUP BY ROLLUP (a, b)");
    round_trips("SELECT a FROM t GROUP BY a, CUBE (b, c)");
}

#[test]
fn minus_and_hierarchy_words_are_not_aliases() {
    round_trips("SELECT 1 FROM dual MINUS SELECT 2 FROM dual");
    round_trips("SELECT a FROM t START WITH a = 1 CONNECT BY b = PRIOR a");
}

#[test]
fn returning_follows_a_query_source() {
    round_trips("INSERT INTO t SELECT 1 RETURNING 2");
    round_trips("INSERT INTO table1 SELECT * FROM table2 RETURNING id");
    rejected("SELECT 1 RETURNING");
    rejected("SELECT * FROM tbl RETURNING");
}

#[test]
fn clause_words_are_not_table_aliases() {
    rejected("SELECT * FROM tbl PIVOT");
    rejected("SELECT * FROM tbl UNPIVOT");
    rejected("SELECT * FROM tbl_name sample JOIN tbl_name_2 ON sample.id = tbl_name_2.id");
}

#[test]
fn aggregates_have_no_filter_clause() {
    rejected("SELECT COUNT(*) FILTER (WHERE i > 0) FROM t");
    rejected("SELECT count(x) FILTER (WHERE y) OVER () FROM t");
}

#[test]
fn insert_alias_can_use_as() {
    round_trips("INSERT INTO t AS x (a) VALUES (1)");
}

#[test]
fn identifiers() {
    round_trips("SELECT a$b, a#b, a@b, é1 FROM t");
    rejected("SELECT _a FROM t");
    rejected("SELECT 💝 FROM t");
}

#[test]
fn block_comments_do_not_nest() {
    rejected("select /* comment level 1 /* comment level 2 */ */ 1;");
}

mod shape {
    use super::common::*;
    use sqlparser::ast::BinaryOp::*;
    use sqlparser::ast::*;
    use sqlparser::dialect::Oracle;
    use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement};
    use sqlparser::dialect::oracle::{
        HintStyle, InsertTarget, OptimizerHint, OracleExpr, OracleStatement,
    };
    use sqlparser::parse_sql;

    fn statement(sql: &str) -> Statement<'_, Builtin> {
        parse_sql::<Oracle>(sql).unwrap().remove(0)
    }

    fn only_select(sql: &str) -> Select<'_, Builtin> {
        match statement(sql).kind {
            StatementKind::Query(query) => into_select(*query),
            other => panic!("not a query: {other:?}"),
        }
    }

    fn string(text: &str) -> Expr<'_, Builtin> {
        ExprKind::Literal(Literal::String(StringLiteral {
            prefix: Default::default(),
            triple: false,
            national: false,
            text,
            quote: '\'',
            backslash_escapes: false,
        }))
        .into()
    }

    fn projected<'a>(select: &'a Select<'a, Builtin>, n: usize) -> &'a Expr<'a, Builtin> {
        match &select.projection[n].kind {
            SelectItemKind::Expr { expr, .. } => expr,
            other => panic!("not an expression: {other:?}"),
        }
    }

    #[test]
    fn concatenation_is_left_associative_with_addition() {
        let select = only_select("SELECT 3 + 5 || '.3' || 7 - 9 FROM dual");
        let expected = bin(
            bin(
                bin(bin(num("3"), Add, num("5")), StringConcat, string(".3")),
                StringConcat,
                num("7"),
            ),
            Sub,
            num("9"),
        );
        assert_eq!(projected(&select, 0), &expected);
    }

    #[test]
    fn multiplication_binds_tighter_than_concatenation() {
        let select = only_select("SELECT 3 / 5 || 'asdf' || 7 * 9 FROM dual");
        let expected = bin(
            bin(bin(num("3"), Div, num("5")), StringConcat, string("asdf")),
            StringConcat,
            bin(num("7"), Mul, num("9")),
        );
        assert_eq!(projected(&select, 0), &expected);
    }

    fn quote_delimited<'a>(expr: &'a Expr<'a, Builtin>) -> (bool, char, &'a str, char) {
        let ExprKind::Ext(BuiltinExpr::Oracle(OracleExpr::QuoteDelimited(string))) = &expr.kind
        else {
            panic!("not a quote-delimited string: {expr:?}");
        };
        (string.national, string.start, string.value, string.end)
    }

    #[test]
    fn quote_delimited_strings_keep_their_delimiters_and_value() {
        let sql = "SELECT Q'.abc.', Q'Xab'cX', Q'|abc'''|', Q'{abc}d}', Q'[]abc[]', Q'<a'bc>', Q'<<<a'bc>', Q'('abc'('abc)', Q'(abc'def))', Q'(abc'def)))', NQ'!a'b'c!d!' FROM dual";
        let select = only_select(sql);
        let expected = [
            (false, '.', "abc", '.'),
            (false, 'X', "ab'c", 'X'),
            (false, '|', "abc'''", '|'),
            (false, '{', "abc}d", '}'),
            (false, '[', "]abc[", ']'),
            (false, '<', "a'bc", '>'),
            (false, '<', "<<a'bc", '>'),
            (false, '(', "'abc'('abc", ')'),
            (false, '(', "abc'def)", ')'),
            (false, '(', "abc'def))", ')'),
            (true, '!', "a'b'c!d", '!'),
        ];
        for (n, expected) in expected.into_iter().enumerate() {
            assert_eq!(quote_delimited(projected(&select, n)), expected);
        }
    }

    #[test]
    fn hints_are_collected_in_order() {
        let select = only_select("SELECT /*+one two three*/ /*x+four*/ 1 FROM dual");
        assert_eq!(
            select.modifiers.hints,
            vec![
                OptimizerHint {
                    prefix: "",
                    text: "one two three",
                    style: HintStyle::MultiLine,
                },
                OptimizerHint {
                    prefix: "x",
                    text: "four",
                    style: HintStyle::MultiLine,
                },
            ]
        );
    }

    #[test]
    fn single_line_hints_exclude_the_newline() {
        let select = only_select("SELECT --abc+ text\n 1 FROM dual");
        assert_eq!(
            select.modifiers.hints,
            vec![OptimizerHint {
                prefix: "abc",
                text: " text",
                style: HintStyle::SingleLine,
            }]
        );
    }

    #[test]
    fn plain_comments_leave_no_hints() {
        let select = only_select("SELECT /*one two three*/ 1 FROM dual");
        assert!(select.modifiers.hints.is_empty());
    }

    fn oracle(statement: Statement<'_, Builtin>) -> OracleStatement<'_> {
        match statement.kind {
            StatementKind::Ext(BuiltinStatement::Oracle(statement)) => statement,
            other => panic!("not an Oracle statement: {other:?}"),
        }
    }

    #[test]
    fn plain_insert_stays_a_core_node() {
        let StatementKind::Insert(insert) = statement("INSERT INTO t (a) VALUES (1)").kind else {
            panic!("not a core insert");
        };
        assert_eq!(insert.table, object_name(&["t"]));
        assert_eq!(insert.columns, vec![ident("a")]);
    }

    #[test]
    fn insert_alias_and_qualified_columns() {
        let OracleStatement::Insert(insert) = oracle(statement(
            "INSERT INTO foo_t t (t.id, t.val) SELECT 1, 2 FROM dual",
        )) else {
            panic!("not an insert");
        };
        assert_eq!(insert.target, InsertTarget::Table(object_name(&["foo_t"])));
        let alias = insert.alias.as_ref().unwrap();
        assert_eq!(alias.name, ident("t"));
        assert!(!alias.explicit);
        assert_eq!(
            insert.columns,
            vec![object_name(&["t", "id"]), object_name(&["t", "val"])]
        );
    }

    #[test]
    fn insert_into_a_query_keeps_the_query() {
        let OracleStatement::Insert(insert) =
            oracle(statement("INSERT INTO (SELECT a FROM t) (a) VALUES (1)"))
        else {
            panic!("not an insert");
        };
        let InsertTarget::Query(query) = &insert.target else {
            panic!("not a query target");
        };
        assert_eq!(query.to_string(), "SELECT a FROM t");
        assert_eq!(insert.columns, vec![object_name(&["a"])]);
    }

    #[test]
    fn hinted_update_wraps_the_core_node() {
        let OracleStatement::Update { hints, update } =
            oracle(statement("UPDATE /*+ x */ t SET a = 1"))
        else {
            panic!("not an update");
        };
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].text, " x ");
        assert_eq!(update.assignments.len(), 1);
    }

    #[test]
    fn delete_records_a_missing_from() {
        let OracleStatement::Delete {
            hints,
            from_keyword,
            delete,
        } = oracle(statement("DELETE t WHERE a = 1"))
        else {
            panic!("not a delete");
        };
        assert!(hints.is_empty());
        assert!(!from_keyword);
        assert!(delete.selection.is_some());
    }

    #[test]
    fn delete_with_from_stays_a_core_node() {
        assert!(matches!(
            statement("DELETE FROM t").kind,
            StatementKind::Delete(_)
        ));
    }

    #[test]
    fn execute_immediate_has_its_parts() {
        let OracleStatement::ExecuteImmediate { sql, into, using } = oracle(statement(
            "EXECUTE IMMEDIATE 'SELECT 1' INTO a, b USING 1 AS x, y",
        )) else {
            panic!("not an execute");
        };
        assert_eq!(sql, string("SELECT 1"));
        assert_eq!(into, vec![ident("a"), ident("b")]);
        assert_eq!(using.len(), 2);
        assert_eq!(using[0], (num("1"), Some(ident("x"))));
        assert_eq!(using[1], (col("y"), None));
    }

    #[test]
    fn boolean_words_are_identifiers() {
        let select = only_select("SELECT TRUE FROM dual");
        assert_eq!(projected(&select, 0), &col("TRUE"));
    }

    #[test]
    fn in_argument_null_treatment_is_recorded() {
        let select = only_select("SELECT f(a IGNORE NULLS) FROM t");
        let ExprKind::Function { options, .. } = &projected(&select, 0).kind else {
            panic!("not a function");
        };
        let options = options.as_ref().unwrap();
        assert_eq!(options.inner_null_treatment, Some(NullTreatment::Ignore));
        assert_eq!(options.null_treatment, None);
    }
}
