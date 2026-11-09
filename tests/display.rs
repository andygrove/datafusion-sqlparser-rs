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

//! Rendering the AST back to SQL.

mod common;

use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift};
use sqlparser::{parse_expr, parse_sql};

/// `sql` must render back unchanged, and the output must parse to the same
/// tree.
fn assert_round_trip<D: Dialect>(sql: &str) {
    let rendered = render::<D>(sql);
    assert_eq!(rendered, sql);
    assert_eq!(
        parse_sql::<D>(sql).unwrap(),
        parse_sql::<D>(&rendered).unwrap(),
        "{sql}"
    );
}

fn render<D: Dialect>(sql: &str) -> String {
    let statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

/// `input` must render as `output`, which must mean the same thing.
fn assert_normalized(input: &str, output: &str) {
    let rendered = render::<Core>(input);
    assert_eq!(rendered, output, "{input}");
    assert_eq!(
        parse_sql::<Core>(input).unwrap(),
        parse_sql::<Core>(&rendered).unwrap(),
        "{input}"
    );
}

#[test]
fn expressions_round_trip() {
    let cases = [
        "SELECT a + b * c",
        "SELECT (a + b) * c",
        "SELECT ((a))",
        "SELECT a - b - c",
        "SELECT a - (b - c)",
        "SELECT a = 1 AND b <> 2 OR c <= 3",
        "SELECT a < 1, b > 2, c >= 3, d / 4, e % 5",
        "SELECT NOT a",
        "SELECT NOT (a)",
        "SELECT -a",
        "SELECT - -a",
        "SELECT 1.5",
        "SELECT 'text'",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn source_text_is_kept_exactly() {
    let cases = [
        "SELECT MixedCase FROM SomeTable",
        "SELECT \"Quoted Name\" FROM \"T\"",
        "SELECT \"a\"\"b\"",
        "SELECT 'it''s'",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn names_and_functions_round_trip() {
    let cases = [
        "SELECT t.a, s.t.a FROM s.t",
        "SELECT t.order",
        "SELECT f()",
        "SELECT COUNT(*)",
        "SELECT f(a, 1 + 2)",
        "SELECT s.f(g(a))",
        "SELECT CAST(a + 1 AS int)",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn select_clauses_round_trip() {
    let cases = [
        "SELECT a FROM t x",
        "SELECT a FROM t x JOIN u y ON c",
        "SELECT a FROM (SELECT 1) x",
        "SELECT *",
        "SELECT a, b AS c, t.*, s.t.*",
        "SELECT a FROM t AS x",
        "SELECT a FROM t, u",
        "SELECT a FROM t WHERE a > 1",
        "SELECT a FROM t GROUP BY a, b",
        "SELECT a FROM t GROUP BY a HAVING a > 1",
        "SELECT a FROM t ORDER BY a",
        "SELECT a FROM t ORDER BY a ASC, b DESC, c",
        "SELECT a FROM t LIMIT 10",
        "SELECT a FROM t LIMIT 10 OFFSET 5",
        "SELECT a FROM t WHERE a GROUP BY b HAVING c ORDER BY d LIMIT 1",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn joins_round_trip() {
    let cases = [
        "SELECT 1 FROM a INNER JOIN b ON x",
        "SELECT 1 FROM a LEFT OUTER JOIN b ON x",
        "SELECT 1 FROM a RIGHT OUTER JOIN b USING(x)",
        "SELECT 1 FROM a NATURAL INNER JOIN b",
        "SELECT 1 FROM a NATURAL LEFT OUTER JOIN b",
        "SELECT 1 FROM a JOIN b ON x = y",
        "SELECT 1 FROM a JOIN b",
        "SELECT 1 FROM a LEFT JOIN b USING(x, y)",
        "SELECT 1 FROM a RIGHT JOIN b ON x",
        "SELECT 1 FROM a FULL JOIN b ON x",
        "SELECT 1 FROM a CROSS JOIN b",
        "SELECT 1 FROM a JOIN b AS c ON x LEFT JOIN d ON y, e",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn nested_queries_round_trip() {
    let cases = [
        "SELECT (SELECT 1)",
        "SELECT ((SELECT 1))",
        "SELECT a + (SELECT 1 UNION SELECT 2)",
        "SELECT EXISTS (SELECT 1)",
        "SELECT NOT EXISTS (SELECT 1)",
        "SELECT a IN (1, b)",
        "SELECT a NOT IN (1)",
        "SELECT a IN (SELECT 1)",
        "SELECT a NOT IN (SELECT 1)",
        "SELECT 1 FROM (SELECT 2) AS x",
        "SELECT 1 FROM (SELECT 2)",
        "SELECT 1 FROM (SELECT 2) AS x JOIN (SELECT 3) AS y ON a",
        "WITH c AS (SELECT 1) SELECT 2 FROM c",
        "WITH a AS (SELECT 1), b AS (WITH d AS (SELECT 2) SELECT 3) SELECT 4",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn set_operations_round_trip() {
    let cases = [
        "SELECT 1 UNION SELECT 2",
        "SELECT 1 UNION ALL SELECT 2",
        "SELECT 1 UNION DISTINCT SELECT 2",
        "SELECT 1 EXCEPT SELECT 2",
        "SELECT 1 INTERSECT SELECT 2",
        "SELECT 1 UNION SELECT 2 INTERSECT SELECT 3",
        "(SELECT 1 UNION SELECT 2) INTERSECT SELECT 3",
        "(SELECT 1 LIMIT 5) UNION SELECT 2",
        "SELECT 1 UNION SELECT 2 ORDER BY a LIMIT 3",
        "WITH c AS (SELECT 1) SELECT 2 UNION SELECT 3",
        "(SELECT 1)",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn statements_round_trip() {
    let cases = [
        "INSERT INTO t VALUES (1, a)",
        "INSERT INTO s.t VALUES (1), (2), (3)",
        "INSERT INTO t (a, b) VALUES (1, 2)",
        "INSERT INTO t SELECT 1",
        "INSERT INTO t (a) SELECT 1",
        "INSERT INTO t (SELECT 1)",
        "UPDATE t SET a = 1",
        "UPDATE t AS x SET a = 1, t.b = c + 2 WHERE d = 3",
        "DELETE FROM s.t",
        "DELETE FROM t AS x WHERE a = 1",
        "CREATE TABLE t (a int)",
        "CREATE TABLE IF NOT EXISTS s.t (a int, b varchar(255))",
        "CREATE TABLE t (a decimal(10,2))",
        "CREATE TABLE t (a int NOT NULL UNIQUE PRIMARY KEY, b int NULL)",
        "CREATE TABLE t (a int DEFAULT 1 + 2 NOT NULL)",
        "CREATE TABLE t (a int, b int, PRIMARY KEY (a, b), UNIQUE (b))",
        "CREATE TABLE t AS SELECT 1",
    ];
    for sql in cases {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn mysql_syntax_round_trips() {
    let cases = [
        "SELECT `a b` FROM `t`",
        "SELECT `a``b`",
        "SELECT \"text\"",
        "SELECT a DIV b",
        "SELECT a + b DIV c",
        "SELECT a FROM t LIMIT 5, 10",
        "SELECT a FROM t LIMIT 10 OFFSET 5",
        "INSERT IGNORE INTO t VALUES (1)",
        "INSERT INTO t VALUES (1)",
        "CREATE TABLE t (`a` int AUTO_INCREMENT NOT NULL)",
    ];
    for sql in cases {
        assert_round_trip::<MySql>(sql);
    }
}

#[test]
fn postgres_syntax_round_trips() {
    let cases = [
        "SELECT x::int",
        "SELECT x::int::text",
        "SELECT CAST(x AS int)::text",
        "SELECT (a + b)::int",
        "SELECT -x::int",
        "INSERT INTO t VALUES (1) RETURNING *",
        "UPDATE t SET a = 1 RETURNING a, b AS c",
        "DELETE FROM t RETURNING *",
    ];
    for sql in cases {
        assert_round_trip::<Postgres>(sql);
    }
}

#[test]
fn redshift_syntax_round_trips() {
    let cases = ["SELECT TOP 10 a FROM t", "SELECT a FROM t", "SELECT x::int"];
    for sql in cases {
        assert_round_trip::<Redshift>(sql);
    }
}

#[test]
fn equivalent_spellings_are_normalized() {
    let cases = [
        ("select a from t", "SELECT a FROM t"),
        ("SELECT   a\n\tFROM t ;", "SELECT a FROM t"),
        ("SELECT a b FROM t", "SELECT a AS b FROM t"),
        ("SELECT a != b", "SELECT a <> b"),
        (
            "SELECT 1 FROM a FULL OUTER JOIN b ON c",
            "SELECT 1 FROM a FULL JOIN b ON c",
        ),
    ];
    for (input, output) in cases {
        assert_normalized(input, output);
    }
}

#[test]
fn expressions_render_on_their_own() {
    let parsed = parse_expr::<Postgres>("a + b::int").unwrap();
    assert_eq!(parsed.to_string(), "a + b::int");
}

// Rendering recurses like the other operations, so the tree height limit must
// make it safe too. This is the strictest case: a debug build on a 2 MiB stack.

#[test]
fn tallest_operator_chain_renders_without_overflowing_the_stack() {
    let sql = format!("{}1", "1+".repeat(1190));
    let rendered = parse_expr::<Core>(&sql).unwrap().to_string();
    assert!(rendered.starts_with("1 + 1"));
}

#[test]
fn tallest_nested_queries_render_without_overflowing_the_stack() {
    let sql = format!(
        "{}{}1{}",
        "(SELECT ".repeat(30),
        "1+".repeat(960),
        ")".repeat(30)
    );
    let rendered = parse_expr::<Core>(&sql).unwrap().to_string();
    assert!(rendered.starts_with("(SELECT (SELECT"));
}

fn rendered_len<D: Dialect>(sql: &str) -> usize {
    parse_expr::<D>(sql).unwrap().to_string().len()
}

#[test]
fn tallest_cast_chain_renders_without_overflowing_the_stack() {
    let sql = format!("x{}", "::int".repeat(590));
    assert_eq!(rendered_len::<Postgres>(&sql), sql.len());
}

#[test]
fn tallest_dialect_operator_chain_renders_without_overflowing_the_stack() {
    let sql = format!("1{}", " DIV 1".repeat(590));
    assert_eq!(rendered_len::<MySql>(&sql), sql.len());
}

#[test]
fn tallest_in_chain_renders_without_overflowing_the_stack() {
    let sql = format!("1{}", " IN (1)".repeat(590));
    assert_eq!(rendered_len::<Core>(&sql), sql.len());
}

#[test]
fn tallest_union_chain_renders_without_overflowing_the_stack() {
    let sql = format!("{}SELECT 1", "SELECT 1 UNION ALL ".repeat(1190));
    assert_eq!(render::<Core>(&sql), sql);
}

#[test]
fn update_and_delete_keep_how_aliases_were_written() {
    for sql in [
        "UPDATE t x SET a = 1",
        "UPDATE t AS x SET a = 1",
        "DELETE FROM t x WHERE a",
        "DELETE FROM t AS x WHERE a",
    ] {
        assert_round_trip::<Core>(sql);
    }
}

#[test]
fn full_outer_join_renders_as_full_join() {
    assert_normalized(
        "SELECT 1 FROM a FULL OUTER JOIN b ON c",
        "SELECT 1 FROM a FULL JOIN b ON c",
    );
}
