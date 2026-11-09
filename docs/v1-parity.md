<!---
  Licensed to the Apache Software Foundation (ASF) under one
  or more contributor license agreements.  See the NOTICE file
  distributed with this work for additional information
  regarding copyright ownership.  The ASF licenses this file
  to you under the Apache License, Version 2.0 (the
  "License"); you may not use this file except in compliance
  with the License.  You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

  Unless required by applicable law or agreed to in writing,
  software distributed under the License is distributed on an
  "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
  KIND, either express or implied.  See the License for the
  specific language governing permissions and limitations
  under the License.
-->

# Parity with datafusion-sqlparser-rs

How this crate compares with the original, called v1 here. The measurements use
v1 0.63. Everything can be reproduced with the commands at the end.

## Summary

- **Each dialect's own tests:** 99.1% of the statements in v1's tests written for
  a specific dialect are accepted by that dialect here.
- **Grammar:** 96.6% of the SQL v1 accepts in some dialect also parses here in
  some dialect, with the same rendering apart from case and spacing.
- **All of v1's tests, per dialect:** 54–78%. This is low mainly because v1 does
  not parse strictly and this crate does, so it can never reach 100%. See
  [What the numbers mean](#what-the-numbers-mean).
- **Features:** rendering, pretty-printing, the visitor, `serde` and comments
  are done. Spans are partial. `no_std`, `bigdecimal` and `derive-dialect` are
  missing.
- **Performance:** with the Postgres dialect, 4.4–5.9x faster than v1, with
  4–94x fewer allocations.

This crate is not ready to replace v1, and its API will change.

## Features

| Feature | v1 | Here | Notes |
|---|---|---|---|
| Parse statements | `Parser::parse_sql(&dialect, sql)` | Done | `parse_sql::<D>(sql)` |
| Choose a dialect at runtime | `dialect_from_str` | Done | `DialectKind::from_name` and `parse_sql_as` |
| Dialects | 16 | 14 | Every dialect but `GenericDialect` and `AnsiDialect`, on purpose. `Core` is the shared grammar, not a standard. |
| Custom dialects | `Dialect` trait with fixed hooks | Done, and goes further | Grammar rules are hooks, and a dialect can add tokens, operators and AST nodes from another crate. |
| `derive-dialect` | Yes | Missing | Building on another dialect takes one forwarding method per rule. |
| Render to SQL | `Display` | Done | |
| Pretty-printing | `{:#}` | Done | Some statements differ from v1. `MERGE`, `CREATE INDEX` and `EXPLAIN` are multi-line here. |
| `visitor` feature | Yes | Done | `VisitorMut` replacement strings must outlive the AST. |
| `serde` feature | Yes | Done | `Deserialize` borrows from the input. Strings with JSON escapes need `deserialize_with_arena`. |
| Source spans | Partial | Partial | Here: identifiers, expressions, statements, select items and table references, recorded while parsing as byte offsets. v1 covers more node types, with many empty or approximate spans. |
| Comments | `parse_sql_with_comments` | Done | A comment at the end of a line goes with the code before it. |
| Trailing commas option | `ParserOptions` | By dialect | Set by each dialect's rules, with no runtime switch. |
| Unescaping option | `ParserOptions` | Different | Text is kept as written, and `Ident::value` and `StringLiteral::value` resolve escapes on request. |
| Statements without semicolons | `require_semicolon_stmt_delimiter` | Missing | Used for MS SQL scripts. |
| Recursion limit | `with_recursion_limit`, default 50 | Partial | Nesting is limited to 64 and cannot be changed. `with_max_tree_height` sets the tree height limit, default 1,200. |
| Stack safety | `recursive-protection` feature | Different | The two limits above keep parsing and rendering within 2 MiB in a debug build. `serde` grows the stack through `stacker`. |
| Tokenizer | `Tokenizer` | Partial | `lexer::tokenize::<D>` returns tokens with spans. Whitespace tokens and reusable buffers are not provided. |
| Parse from existing tokens | `with_tokens` | Missing | |
| `no_std` | Yes | Missing | |
| `bigdecimal` feature | Yes | Missing | Numbers are kept as text. |
| Command-line example | `examples/cli.rs` | Missing | |
| `test_utils` module | Public | Not provided | |
| AST compatibility | | Different | The AST borrows from the SQL and has extension slots, so upgrading is not source-compatible. |

## Parser coverage

### How it is measured

`tests/corpus/v1.corpus` holds 6,483 SQL strings: every statement and expression
that v1's test suite parses, with v1's result in each dialect. `tests/v1_corpus.rs`
runs them through both parsers on every test run. A case that regresses fails the
build, and `tests/corpus/known_failures` can only shrink.

The corpus checks three things: whether SQL parses, whether it renders to the
same text, and whether invalid SQL is rejected. It does not compare AST
structure, because the two ASTs differ by design.

### What the numbers mean

**100% parity with v1 is not possible, and is not the goal.** v1 does not parse
strictly. It has one shared parser, so it accepts a great deal of syntax in every
dialect, including syntax the database in question would reject. For example, v1
accepts PostgreSQL's `DROP OPERATOR` in its MySQL and BigQuery dialects, and
accepts `SELECT 1, WHERE` in its SQLite dialect, which SQLite rejects as a syntax
error. Its test suite records that behavior, so a test can pass in v1 for SQL that
is not valid.

These statements all parse with v1's `SqliteDialect`, and SQLite 3.51 reports a
syntax error for each one:

```sql
SELECT 1, WHERE
SELECT TOP 3 * FROM t
DROP OPERATOR && (INTEGER, INTEGER)
CREATE TYPE address AS (street VARCHAR(100))
ALTER USER u1
GRANT SELECT ON t TO u
LOCK TABLE t IN SHARE MODE
MERGE INTO t USING s ON a = b WHEN MATCHED THEN DELETE
```

[apache/datafusion-sqlparser-rs#2237](https://github.com/apache/datafusion-sqlparser-rs/issues/2237)
measured the same thing for PostgreSQL: with PostgreSQL's own parser as the ground
truth, v1's PostgreSQL dialect accepted about 30% of the statements from v1's
common tests that PostgreSQL rejects.

This crate keeps each database family's syntax in its own dialects and rejects the
rest. Every such rejection counts as a failure in a comparison with v1, even when
rejecting the SQL is correct.

Where a case comes from in v1's tests shows how much this matters. Each row counts
one SQL string in one dialect, among those v1 accepts:

| Source of the SQL | Accepted here | Total | Rate |
|---|---:|---:|---:|
| v1's tests written for that dialect | 2,200 | 2,219 | 99.1% |
| v1's shared tests (`sqlparser_common`) | 12,172 | 17,099 | 71.2% |
| v1's tests written for other dialects | 7,980 | 17,556 | 45.5% |
| SQL built by v1's test code, so not attributable | 14,090 | 21,518 | 65.5% |

The shared tests include a lot of syntax from a single vendor, such as `ALTER USER`
and `CREATE TYPE`, because v1 accepts it everywhere. The low rate for other
dialects' tests is the intended effect of strict parsing.

Each number answers a different question:

| Number | Question it answers | Limits |
|---|---|---|
| Own tests, 99.1% | Does a dialect handle the SQL v1 tests for it? | The closest measure of valid SQL for a dialect, but those tests guided this work, and some dialects have few of them. |
| Any dialect, 96.6% | Is the syntax implemented anywhere? | Flatters readiness: SQL that parses in only one dialect counts. |
| Per dialect, 54–78% | How often does a dialect agree with v1? | Understates coverage, because it counts correct rejections as failures. |

"Same rendering" has its own ceiling. This crate keeps text as written where v1
changes it, such as a type written `int`, which v1 renders as `INT`. v1 also loses
some text, such as `TERSE` in `SHOW TERSE OBJECTS`, and this crate does not. The
"Close" column ignores case and spacing, and is the better guide.

We have not separated correct rejections from missing syntax case by case, so the
per-dialect figures mix both. PostgreSQL is highest because it has had the most
work.

### Coverage of each dialect's own tests

"Accepted" means the SQL was not rejected, and a few of these render differently
from v1. Counts are small for some dialects, so a high rate there says little.

| Dialect | Accepted | Statements in v1's tests for it |
|---|---:|---:|
| PostgreSQL | 647 | 652 |
| MySQL | 403 | 407 |
| Snowflake | 396 | 399 |
| MS SQL | 184 | 184 |
| BigQuery | 127 | 127 |
| ClickHouse | 81 | 81 |
| Databricks | 73 | 79 |
| SQLite | 58 | 59 |
| Hive | 58 | 58 |
| Spark | 53 | 53 |
| Redshift | 52 | 52 |
| DuckDB | 35 | 35 |
| Oracle | 20 | 20 |
| Teradata | 13 | 13 |

### Coverage in any dialect

5,717 of the 5,920 cases v1 accepts in some dialect (96.6%) parse here in some
dialect, with the same rendering apart from case and spacing.

| Statement | Accepted by v1 | Handled here | Coverage |
|---|---:|---:|---:|
| `SELECT` | 2,277 | 2,191 | 96.2% |
| `CREATE` | 1,079 | 1,064 | 98.6% |
| `ALTER` | 459 | 441 | 96.1% |
| `DROP` | 355 | 355 | 100.0% |
| `GRANT` | 291 | 289 | 99.3% |
| `SHOW` | 133 | 132 | 99.2% |
| `INSERT` | 110 | 108 | 98.2% |
| `SET` | 100 | 100 | 100.0% |
| `COPY` | 61 | 54 | 88.5% |
| `USE` | 54 | 52 | 96.3% |
| `UPDATE` | 33 | 31 | 93.9% |
| `WITH` | 27 | 25 | 92.6% |
| `MERGE` | 25 | 25 | 100.0% |
| `DELETE` | 21 | 20 | 95.2% |

### Coverage by dialect

"Same rendering" counts SQL that parses here and renders exactly as in v1.
"Close" also allows differences in letter case and spacing. "Rejected" is SQL v1
accepts in that dialect and this crate does not. "Accepted" is SQL v1 rejects and
this crate accepts. No case in the corpus causes a panic.

| Dialect | v1 accepts | Same rendering | Close | Parses, differs | Rejected | Accepted |
|---|---:|---:|---:|---:|---:|---:|
| PostgreSQL | 4,606 | 3,581 (77.7%) | 3,735 (81.1%) | 94 | 777 | 36 |
| Snowflake | 4,338 | 2,783 (64.2%) | 2,873 (66.2%) | 56 | 1,409 | 25 |
| MySQL | 4,253 | 2,510 (59.0%) | 2,605 (61.3%) | 92 | 1,556 | 38 |
| MS SQL | 4,050 | 2,363 (58.3%) | 2,451 (60.5%) | 48 | 1,551 | 54 |
| ClickHouse | 4,106 | 2,375 (57.8%) | 2,412 (58.7%) | 62 | 1,632 | 61 |
| DuckDB | 4,113 | 2,365 (57.5%) | 2,456 (59.7%) | 39 | 1,618 | 57 |
| BigQuery | 4,274 | 2,441 (57.1%) | 2,537 (59.4%) | 79 | 1,658 | 16 |
| Hive | 4,014 | 2,273 (56.6%) | 2,366 (58.9%) | 29 | 1,619 | 109 |
| SQLite | 4,037 | 2,269 (56.2%) | 2,362 (58.5%) | 49 | 1,626 | 45 |
| Spark | 4,187 | 2,349 (56.1%) | 2,445 (58.4%) | 39 | 1,703 | 77 |
| Databricks | 4,245 | 2,377 (56.0%) | 2,474 (58.3%) | 75 | 1,696 | 89 |
| Oracle | 4,058 | 2,260 (55.7%) | 2,353 (58.0%) | 35 | 1,670 | 38 |
| Redshift | 4,125 | 2,247 (54.5%) | 2,339 (56.7%) | 70 | 1,716 | 61 |
| Teradata | 3,986 | 2,168 (54.4%) | 2,256 (56.6%) | 29 | 1,701 | 55 |
| `Core` (compared with v1's `AnsiDialect`) | 3,961 | 2,040 (51.5%) | 2,129 (53.7%) | 29 | 1,803 | 67 |

### Most common reasons for rejection

These count distinct SQL strings across all dialects, by the token where parsing
stopped. Most of the `OPERATOR` cases are PostgreSQL's `CREATE`, `ALTER` and
`DROP OPERATOR`, which v1 accepts in every dialect and this crate keeps to
PostgreSQL.

| Stopped at | Strings |
|---|---:|
| `OPERATOR` | 380 |
| `FUNCTION` | 66 |
| `:` | 65 |
| Unexpected `:` | 58 |
| `USER` | 57 |
| Unexpected `@` | 54 |
| Unexpected `` ` `` | 54 |

`cargo run --release --example parity` prints the full list and writes every
failure to `target/parity-failures.txt`.

### Known missing syntax

Each of these parses in v1 in the dialect named and not here:

- BigQuery scripting: `IF ... END IF` and `CASE ... END CASE`
- ClickHouse: `ENUM8` and `ENUM16` columns
- Redshift: `SELECT * EXCLUDE a`
- MySQL and Redshift: nested joins such as `a JOIN b JOIN c ON ... ON ...`
- PostgreSQL and MS SQL: `TABLESAMPLE SYSTEM` and `TABLESAMPLE BERNOULLI`
- Spark: field access on `STRUCT(... AS ...)`

This list is not complete.

## Deliberate differences

- **No `GenericDialect` or `AnsiDialect`.** A real database should be parsed with
  its own dialect. See [The core is not ANSI SQL](../README.md#the-core-is-not-ansi-sql).
- **Dialects are strict.** One database family's syntax stays in that family's
  dialects, even where v1 accepts it everywhere. See
  [Design decisions](../README.md#design-decisions).
- **Reserved words are per dialect.** `SELECT 1 FROM t sort` parses in PostgreSQL,
  where v1 rejects it.
- **The real database wins.** Where v1's mode and the actual engine differ, this
  crate follows the engine. These cases are recorded in `known_failures`, and
  `AGENTS.md` lists them.
- **The AST borrows from the SQL.** Nodes hold `&str`, so the SQL text must
  outlive the AST. See [Design decisions](../README.md#design-decisions).
- **Node shapes differ.** `Expr`, `Statement` and `SelectItem` are structs holding
  a `kind` and a `span`.

## Performance

Using the Postgres dialect in both crates, mostly with queries from v1's
`sqlparser_bench`. v2 does not cover v1's whole grammar yet, and v1 checks for more
syntax at each step, so expect the timing gap to narrow.

| Query | v1 time | v2 time | Faster | v1 allocations | v2 allocations | v1 bytes | v2 bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| `simple_select` | 2.24 µs | 468 ns | 4.8x | 39 | 9 | 18,989 | 2,032 |
| `with_select` | 11.6 µs | 1.99 µs | 5.9x | 168 | 21 | 87,804 | 7,640 |
| `select_100_columns` | 55.1 µs | 10.6 µs | 5.2x | 1,222 | 13 | 347,434 | 38,720 |
| `select_100_qualified_columns` | 87.4 µs | 15.0 µs | 5.8x | 2,142 | 120 | 910,680 | 72,448 |
| `subqueries_and_union` | 22.6 µs | 4.03 µs | 5.6x | 319 | 51 | 201,146 | 12,680 |
| `insert_update_delete_create` | 19.0 µs | 3.39 µs | 5.6x | 257 | 36 | 146,773 | 10,224 |
| `expressions_and_casts` | 419 µs | 94.1 µs | 4.4x | 8,052 | 1,116 | 1,808,004 | 293,488 |
| `large_statement` | 3.49 ms | 704 µs | 5.0x | 70,959 | 7,032 | 21,539,624 | 2,115,928 |

## Reproducing these numbers

```sh
cargo run --release --example parity
cargo bench --bench v1_timing
cargo bench --bench v1_allocations
```
