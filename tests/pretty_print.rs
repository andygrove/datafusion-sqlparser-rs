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

//! `{:#}` renders multi-line SQL with two-space indentation, as the original
//! crate does. Ported from its `tests/pretty_print.rs`, with each statement
//! parsed by a dialect that has its syntax.

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{Kind, V2_DIALECTS, parse_corpus};
use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::dialect::{Core, Dialect, MsSql, MySql, Postgres};
use sqlparser::parse_sql;

fn prettify<D: Dialect>(sql: &str) -> String {
    let statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
    format!("{:#}", statements[0])
}

#[test]
fn select() {
    assert_eq!(
        prettify::<Core>("SELECT a, b, c FROM my_table WHERE x = 1 AND y = 2"),
        r#"
SELECT
  a,
  b,
  c
FROM
  my_table
WHERE
  x = 1 AND y = 2
"#
        .trim()
    );
}

#[test]
fn join() {
    assert_eq!(
        prettify::<Core>("SELECT a FROM table1 JOIN table2 ON table1.id = table2.id"),
        r#"
SELECT
  a
FROM
  table1
  JOIN table2 ON table1.id = table2.id
"#
        .trim()
    );
}

#[test]
fn subquery() {
    assert_eq!(
        prettify::<Core>("SELECT * FROM (SELECT a, b FROM my_table) AS subquery"),
        r#"
SELECT
  *
FROM
  (
    SELECT
      a,
      b
    FROM
      my_table
  ) AS subquery
"#
        .trim()
    );
}

#[test]
fn union() {
    assert_eq!(
        prettify::<Core>("SELECT a FROM table1 UNION SELECT b FROM table2"),
        r#"
SELECT
  a
FROM
  table1
UNION
SELECT
  b
FROM
  table2
"#
        .trim()
    );
}

#[test]
fn group_by() {
    assert_eq!(
        prettify::<Core>("SELECT a, COUNT(*) FROM my_table GROUP BY a HAVING COUNT(*) > 1"),
        r#"
SELECT
  a,
  COUNT(*)
FROM
  my_table
GROUP BY
  a
HAVING
  COUNT(*) > 1
"#
        .trim()
    );
}

#[test]
fn cte() {
    assert_eq!(
        prettify::<Core>("WITH cte AS (SELECT a, b FROM my_table) SELECT * FROM cte"),
        r#"
WITH cte AS (
  SELECT
    a,
    b
  FROM
    my_table
)
SELECT
  *
FROM
  cte
"#
        .trim()
    );
}

#[test]
fn case_when() {
    assert_eq!(
        prettify::<Core>(
            "SELECT CASE WHEN x > 0 THEN 'positive' WHEN x < 0 THEN 'negative' ELSE 'zero' END FROM my_table"
        ),
        r#"
SELECT
  CASE
    WHEN x > 0 THEN
      'positive'
    WHEN x < 0 THEN
      'negative'
    ELSE
      'zero'
  END
FROM
  my_table
"#
        .trim()
    );
}

#[test]
fn window_function() {
    assert_eq!(
        prettify::<Core>(
            "SELECT id, value, ROW_NUMBER() OVER (PARTITION BY category ORDER BY value DESC) as rank FROM my_table"
        ),
        r#"
SELECT
  id,
  value,
  ROW_NUMBER() OVER (
    PARTITION BY category
    ORDER BY value DESC
  ) AS rank
FROM
  my_table
"#
        .trim()
    );
}

#[test]
fn empty_window_stays_on_one_line() {
    assert_eq!(
        prettify::<Core>("SELECT f() OVER () FROM t"),
        "SELECT\n  f() OVER ()\nFROM\n  t"
    );
}

/// Only `CASE` and `OVER` are multi-line inside an expression, as in the
/// original, so a `CASE` nested in an operator stays on one line.
#[test]
fn nested_case_in_an_operator_stays_on_one_line() {
    assert_eq!(
        prettify::<Core>("SELECT 1 + CASE WHEN a THEN 1 END"),
        "SELECT\n  1 + CASE WHEN a THEN 1 END"
    );
}

#[test]
fn multiline_string() {
    assert_eq!(
        prettify::<Core>("SELECT 'multiline\nstring' AS str"),
        r#"
SELECT
  'multiline
string' AS str
"#
        .trim(),
        "A literal string with a newline should be kept as is."
    );
}

/// The original adds indentation inside a string that is exactly one newline.
#[test]
fn string_of_one_newline_is_not_indented() {
    assert_eq!(prettify::<Core>("SELECT '\n'"), "SELECT\n  '\n'");
}

#[test]
fn insert_values() {
    assert_eq!(
        prettify::<Core>("INSERT INTO my_table (a, b, c) VALUES (1, 2, 3), (4, 5, 6)"),
        r#"
INSERT INTO my_table (a, b, c)
VALUES
  (1, 2, 3),
  (4, 5, 6)
"#
        .trim()
    );
}

#[test]
fn insert_select() {
    assert_eq!(
        prettify::<Postgres>(
            "INSERT INTO my_table (a, b) SELECT x, y FROM source_table RETURNING a AS id"
        ),
        r#"
INSERT INTO my_table (a, b)
SELECT
  x,
  y
FROM
  source_table
RETURNING
  a AS id
"#
        .trim()
    );
}

#[test]
fn update() {
    assert_eq!(
        prettify::<Postgres>("UPDATE my_table SET a = 1, b = 2 WHERE x > 0 RETURNING id, name"),
        r#"
UPDATE my_table
SET
  a = 1,
  b = 2
WHERE
  x > 0
RETURNING
  id,
  name
"#
        .trim()
    );
}

/// The original's second assertion, on `DELETE table1, table2` with no `FROM`,
/// is not ported: no dialect here accepts that statement.
#[test]
fn delete() {
    assert_eq!(
        prettify::<Postgres>("DELETE FROM my_table WHERE x > 0 RETURNING id, name"),
        r#"
DELETE FROM
  my_table
WHERE
  x > 0
RETURNING
  id,
  name
"#
        .trim()
    );
}

#[test]
fn delete_from_several_tables() {
    assert_eq!(
        prettify::<MySql>("DELETE t1, t2 FROM t1 JOIN t2 ON t1.a = t2.a WHERE t1.b > 0"),
        r#"
DELETE
  t1,
  t2 FROM
  t1
  JOIN t2 ON t1.a = t2.a
WHERE
  t1.b > 0
"#
        .trim()
    );
}

/// `REFERENCES other_table (id)` keeps the core's spacing, where the original
/// writes `other_table(id)`.
#[test]
fn create_table() {
    assert_eq!(
        prettify::<Core>(
            "CREATE TABLE my_table (id INT PRIMARY KEY, name VARCHAR(255) NOT NULL, CONSTRAINT fk_other FOREIGN KEY (id) REFERENCES other_table(id))"
        ),
        r#"
CREATE TABLE my_table (
  id INT PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  CONSTRAINT fk_other FOREIGN KEY (id) REFERENCES other_table (id)
)
"#
        .trim()
    );
}

#[test]
fn create_table_with_no_constraints() {
    assert_eq!(
        prettify::<Core>("CREATE TABLE t (a INT, b INT)"),
        "CREATE TABLE t (\n  a INT,\n  b INT\n)"
    );
}

#[test]
fn create_view() {
    assert_eq!(
        prettify::<Core>("CREATE VIEW my_view AS SELECT a, b FROM my_table WHERE x > 0"),
        r#"
CREATE VIEW my_view AS
SELECT
  a,
  b
FROM
  my_table
WHERE
  x > 0
"#
        .trim()
    );
}

/// The original's test has no `AS` here. MS SQL always renders `AS` before a
/// function body, and `{}` output must not change.
#[test]
fn create_function() {
    assert_eq!(
        prettify::<MsSql>(
            "CREATE FUNCTION my_func() RETURNS INT BEGIN SELECT COUNT(*) INTO @count FROM my_table; RETURN @count; END"
        ),
        r#"
CREATE FUNCTION my_func() RETURNS INT AS
BEGIN
  SELECT COUNT(*) INTO @count FROM my_table;
  RETURN @count;
END
"#
        .trim()
    );
}

#[test]
fn json_table() {
    assert_eq!(
        prettify::<MySql>(
            "SELECT * FROM JSON_TABLE(@json, '$[*]' COLUMNS (id INT PATH '$.id', name VARCHAR(255) PATH '$.name')) AS jt"
        ),
        r#"
SELECT
  *
FROM
  JSON_TABLE(
    @json,
    '$[*]' COLUMNS (
      id INT PATH '$.id',
      name VARCHAR(255) PATH '$.name'
    )
  ) AS jt
"#
        .trim()
    );
}

/// The original formats only the first statement and expects a one-line
/// `UPDATE`, which contradicts its own `update` test. This renders each
/// statement, so `UPDATE` is multi-line as it is everywhere else.
#[test]
fn transaction_blocks() {
    let statements = parse_sql::<Core>("BEGIN; UPDATE my_table SET x = 1; COMMIT;").unwrap();
    let rendered: Vec<String> = statements.iter().map(|s| format!("{s:#};")).collect();
    assert_eq!(
        rendered.join("\n"),
        r#"
BEGIN;
UPDATE my_table
SET
  x = 1;
COMMIT;
"#
        .trim()
    );
}

/// `IF ... THEN ... END IF` is not in any dialect here. This is the same
/// layout on MS SQL's form, which has no `THEN` or `END IF`.
#[test]
fn control_flow() {
    assert_eq!(
        prettify::<MsSql>("IF x > 0 SELECT 'positive'; ELSE SELECT 'negative';"),
        r#"
IF x > 0
  SELECT 'positive';
ELSE
  SELECT 'negative';
"#
        .trim()
    );
    assert_eq!(
        prettify::<MsSql>("IF x > 0 BEGIN SELECT 1; SELECT 2; END ELSE BEGIN SELECT 3; END"),
        r#"
IF x > 0 BEGIN
  SELECT 1;
  SELECT 2;
END
ELSE BEGIN
  SELECT 3;
END
"#
        .trim()
    );
    assert_eq!(
        prettify::<MsSql>("WHILE 1 = 1 BEGIN SELECT 1; END"),
        "WHILE 1 = 1 BEGIN\n  SELECT 1;\nEND"
    );
}

#[test]
fn merge() {
    assert_eq!(
        prettify::<Core>(
            "MERGE INTO target_table t USING source_table s ON t.id = s.id WHEN MATCHED THEN UPDATE SET t.value = s.value WHEN NOT MATCHED THEN INSERT (id, value) VALUES (s.id, s.value)"
        ),
        r#"
MERGE INTO target_table t
USING source_table s ON t.id = s.id
WHEN MATCHED THEN
  UPDATE SET t.value = s.value
WHEN NOT MATCHED THEN
  INSERT (id, value) VALUES (s.id, s.value)
"#
        .trim()
    );
}

#[test]
fn create_index() {
    assert_eq!(
        prettify::<Core>("CREATE INDEX idx_name ON my_table (column1, column2)"),
        r#"
CREATE INDEX idx_name
ON my_table (column1, column2)
"#
        .trim()
    );
}

#[test]
fn explain() {
    assert_eq!(
        prettify::<Core>("EXPLAIN ANALYZE SELECT * FROM my_table WHERE x > 0"),
        r#"
EXPLAIN ANALYZE
SELECT
  *
FROM
  my_table
WHERE
  x > 0
"#
        .trim()
    );
}

/// `{}` must stay on one line, whatever `{:#}` does.
#[test]
fn plain_output_is_one_line() {
    for sql in [
        "SELECT a, CASE WHEN x THEN 1 ELSE 2 END FROM (SELECT 1) AS t JOIN u ON 1 = 1 WHERE b",
        "WITH c AS (SELECT 1) SELECT f() OVER (PARTITION BY a ORDER BY b) FROM c UNION SELECT 2",
        "INSERT INTO t (a) VALUES (1), (2)",
        "CREATE TABLE t (a INT, CHECK (a > 0))",
    ] {
        let statements = parse_sql::<Core>(sql).unwrap();
        assert!(!statements[0].to_string().contains('\n'), "{sql}");
    }
}

fn dialect_kind(name: &str) -> DialectKind {
    match name {
        "ansi" => DialectKind::Core,
        other => DialectKind::from_name(other).unwrap_or_else(|| panic!("unknown dialect {other}")),
    }
}

/// Every statement in the corpus must render with `{:#}` to SQL that parses
/// back, under the same dialect, to the same `{}` text.
#[test]
fn pretty_sql_parses_back_to_the_same_text() {
    let cases = parse_corpus(include_str!("corpus/v1.corpus"));
    let mut failures = Vec::new();
    let mut checked = 0;
    for dialect in V2_DIALECTS {
        let kind = dialect_kind(dialect);
        for case in cases.iter().filter(|case| case.kind == Kind::Statement) {
            let Ok(statements) = parse_sql_as(kind, &case.sql) else {
                continue;
            };
            for statement in &statements {
                checked += 1;
                let plain = statement.to_string();
                let pretty = format!("{statement:#}");
                match parse_sql_as(kind, &pretty) {
                    Ok(reparsed) => {
                        let again: Vec<String> = reparsed.iter().map(|s| s.to_string()).collect();
                        if again != [plain.clone()] {
                            failures.push(format!(
                                "[{dialect}] {plain}\n  pretty: {pretty}\n  became: {}",
                                again.join("; ")
                            ));
                        }
                    }
                    Err(error) => {
                        failures.push(format!(
                            "[{dialect}] {plain}\n  pretty: {pretty}\n  failed: {error}"
                        ));
                    }
                }
            }
        }
    }
    assert!(checked > 1_000, "only {checked} statements were checked");
    assert!(
        failures.is_empty(),
        "{} pretty statements did not parse back:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}
