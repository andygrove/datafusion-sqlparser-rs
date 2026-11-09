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

# AGENTS.md

Guidance for agents working in this repository. See [README.md](README.md) for
the project's goals and scope.

## Project

sqlparser is an in-development v2 of
[datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs).
The long-term goal is full feature parity with that crate, a dialect-free core,
zero-copy parsing, and benchmarks showing it is faster. A local checkout of the
original is the reference for behavior and test cases, so keep a checkout of it
nearby.

New source, build and documentation files start with the ASF license header.

## Commands

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo bench --bench v1_timing       # Parse time against the original crate
cargo bench --bench v1_allocations  # Heap allocations against the original crate
cargo run --release --example parity  # Measure parity against the corpus
UPDATE_KNOWN_FAILURES=1 cargo test --test v1_corpus  # After intended changes
dev/generate_corpus.sh /path/to/datafusion-sqlparser-rs  # Regenerate the corpus
```

The README's Rust examples run as doctests. Benchmark queries must parse in
both crates and stay within the grammar v2 supports.

## Layout

| Path | Purpose |
|---|---|
| `src/ast.rs` | The syntax tree, generic over the dialect |
| `src/lexer.rs` | Tokens that borrow from the input |
| `src/parser.rs` | Token cursor and Pratt loop; contains no grammar |
| `src/grammar.rs` | The core grammar rules |
| `src/grammar/*.rs` | Core rules by topic: queries, expressions, DML, DDL, statements, patterns, `PIVOT` and sampling, and pipe syntax |
| `src/display.rs` | Rendering the AST back to SQL |
| `src/dialect/mod.rs` | The `Dialect` and `AstExt` traits |
| `src/dialect/builtin.rs` | The shared built-in AST and runtime dialect selection |
| `src/dialect/*.rs` | Built-in dialects |
| `src/dialect/snowflake/ddl.rs` | Snowflake's table, stage and scripting statements |
| `tests/stack_safety.rs` | The deepest SQL each rule accepts is safe on a 2 MiB stack |
| `tests/custom_dialect.rs` | A dialect built only on the public API |
| `examples/custom_dialect.rs` | The runnable dialect example linked from the README |
| `docs/dialect-features.md` | The feature matrix by dialect |
| `tests/v1_corpus.rs` | The parity gate against the original crate |
| `tests/render_reparse.rs` | Rendered SQL must parse back to the same text |
| `tests/ast_structure.rs` | Tree shape for statements that render tests cannot check |
| `tests/statement_hooks.rs` | Each statement goes through a hook; dialects can reuse rules |

## Design rules

These are the point of the project. Do not trade them away for convenience.

1. **The core never knows which dialect is running.** `ast.rs`, `lexer.rs`,
   `parser.rs` and `grammar.rs` (and `grammar/`) must not name a dialect, check a dialect's
   type, or gain `supports_*`-style flags. If a dialect needs to change
   something and there is no hook, add a hook or split the rule.
2. **Rules call each other through `D::`.** A core rule that calls another
   overridable rule directly would silently bypass overrides.
3. **Prefer small rules.** A dialect should override only the part that
   differs. `parse_limit` and `parse_limit_offset` are separate for this reason.
4. **No string copies during parsing.** Tokens and AST nodes hold `&'a str`.
   Escapes are resolved on demand by `value()`, which returns a `Cow` and
   allocates only when an escape is present. Allocating error messages is fine.
5. **New keywords and operators need no central registry.** They are matched
   by source text.
6. **Third-party dialects must be first-class.** Anything a built-in dialect
   does must be possible through the public API.
7. **Built-in dialects share one AST.** They all use `Ext = Builtin`, so
   downstream tools handle one tree and a dialect can be chosen at runtime.

8. **Parsed trees must be safe to use.** Untrusted SQL must not be able to
   build a tree that overflows the stack when dropped, cloned, compared or
   walked. Recursive rules go through `Parser::nested`, and loops that build
   left-deep trees call `Parser::deepen`. Do not replace this with a custom
   `Drop`: it would not protect the other operations, and it would stop
   callers destructuring AST nodes by value. Rendering those chains must not
   use `write!`, which overflows the stack in debug builds.

There is deliberately no equivalent of the original's `GenericDialect` or
`AnsiDialect`. `Core` is the shared grammar, with no claim to match the standard.

## Visitors

The `visitor` feature gives every AST type generated traversal through
`#[derive(Visit, VisitMut)]` in `derive/`. It is off by default, so a default
build has no dependencies.

- A new AST type needs `#[cfg_attr(feature = "visitor", derive(crate::ast::Visit, crate::ast::VisitMut))]`.
  A type with no type parameter that holds `Builtin` nodes also needs
  `visit(ext = "crate::dialect::Builtin")`.
- A new `ObjectName` field that names a table or view needs
  `#[cfg_attr(feature = "visitor", visit(with = "visit_relation"))]`, as an
  index, schema, function or role does not.
- Dialects reach the visitors through the `VisitBound` supertrait on each
  `AstExt` slot, which is only a real bound with the feature on.
- A mutable visitor can only store text that lives for the tree's `'a`, unlike
  the original, whose `String` fields take an owned replacement. See `VisitorMut`.

These are deliberate differences from the original's visitor:

- Relations are also reported for `DROP`, foreign key `REFERENCES`, `ALTER TABLE
  ... RENAME TO` and `DROP INDEX ... ON t`, none of which the original annotates. Names
  in `DROP` are relations only for tables and views, and `SHOW ... FROM x`
  only where `x` is a table.
- `TableFactor` has no alias, which is on `TableRef`, so the original's
  `TABLE FACTOR` hook's rendering, which includes an alias, differs.

## Spans

`Expr`, `Statement` and `SelectItem` are structs of `kind` and `span`, and
`TableRef` has a `span` field. Spans are recorded while parsing, never computed
from children, so they include keywords and parentheses. `Parser` sets them in
one place per node: the Pratt loop for expressions, and `Parser::parse_statement`,
`parse_select_item`, `parse_table_ref` and `parse_function_call`.

Dialect rules return these nodes with an empty span through `.into()`, and the
caller fills it in. So reach sub-rules through the `Parser` methods rather than
`D::`, and wrap anything else with `Parser::spanned_expr`. A rule that builds an
`Expr` outside those paths must call `Expr::new` with the right span.
`tests/spans.rs` checks every expression in the corpus: no empty span, inside its
statement, and its text re-parses to the same expression.

The wrappers' `Clone`, `Debug` and `PartialEq` are hand-written with
`#[inline(always)]` because a derived impl adds stack frames per level of a tree.

## Comments

The lexer drops comments. `src/ast/comments.rs` recovers them from the gaps
between tokens when asked, through `Dialect::lex_comment`, so `parse_sql` pays
nothing. They stay out of the AST on purpose; see the module's documentation.
`Comments::before` and `after` rely on `CommentWithSpan::trails`, which treats a
`,` or `;` before a comment as not being the code it trails.

## Serde

The `serde` feature derives `Serialize` and `Deserialize` for every AST type,
including the dialect nodes. A new AST type needs these attributes, keeping only
the part of the bound that applies:

```rust,ignore
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(bound(serialize = "", deserialize = "'de: 'a, X: crate::dialect::DeserializeExt<'de, 'a>")))]
```

A type with `'a` but no `X` needs only `bound(deserialize = "'de: 'a")`, and a
type with neither needs no bound. The container-level `'de: 'a` is what spares
every field a `#[serde(borrow)]`. `DeserializeExt` stands in for a bound per
slot. `SerdeBound` on each `AstExt` slot is a real bound only with the feature
on, and covers `Serialize` only, so a third-party dialect can skip `Deserialize`.

AST strings must be `&'a str`, never `&'static str`, so that they can be
deserialized. Strings the input cannot lend go through `StringArena`, a wrapper
around the deserializer in `src/ast/serde_arena.rs`. The arena is borrowed for
`'de` so that the borrow checker keeps it alive as long as the tree; a
thread-local would not give that guarantee.

Serializing or deserializing the deepest trees the parser accepts does not fit a
small stack in a debug build: a level of the tree is many frames of serde code. Recursive types
therefore use `serde(remote = "Self")` with a hand-written impl in
`src/ast/serde_stack.rs` that grows the stack through `stacker`, and a new
recursive type needs the same, in both directions.

## Adding or changing a dialect

- Put dialect-specific syntax in `src/dialect/<name>.rs`. Define new AST nodes
  there, generic over `X: AstExt`, and add them to `BuiltinExpr`,
  `BuiltinSelect`, `BuiltinStatement` or `BuiltinInsert` in `builtin.rs`. Table
  factors and join conditions have slots in `AstExt` too; the built-in dialects
  leave them as `NoExt`.
- Add new built-in dialects to `DialectKind`, `DialectKind::from_name`,
  `parse_sql_as`, `parse_query_as` and `V2_DIALECTS` in `tests/v1_corpus.rs`.
- Syntax shared by most dialects belongs in the core. Syntax shared by a few
  belongs in a reusable rule that those dialects opt into.
- Dialect syntax that means the same as a core construct should produce the
  core node, recording the written form if needed. `x::int` is `Expr::Cast`
  with `CastStyle::DoubleColon`.
- Write rules as public functions generic over the host dialect and have the
  `Dialect` impl call them, so that derived dialects can reuse them; see
  `redshift.rs`. Use an `ExprExtOf<'a, D>: From<MyExpr<'a, D::Ext>>` bound for a
  rule that any AST can host, or `D: Dialect<Ext = Builtin>` for one that builds
  built-in nodes. Call other rules through `D::`, never through a named dialect.
- Test both directions: the dialect accepts its syntax and produces the right
  tree, and another dialect behaves differently.

## Parity

`tests/v1_corpus.rs` is the regression gate. Do not update
`tests/corpus/known_failures` to hide a regression: a new entry there means
SQL that used to work no longer does.

The parity tool's "Parsed but different" column should only contain deliberate
differences: `ONLY` outside the PostgreSQL family, optimizer hint comments such
as `/*+ ... */` (which are dropped), `B"text"` read as a column with an alias,
and MySQL backslash escapes. Treat anything else there as a bug.

Syntax that belongs to one database family stays in that family's dialects,
even where the original accepts it everywhere. Operators such as `~` and `@>`
are PostgreSQL-family only, `->` is limited to that family and MySQL, and `::` and `ILIKE` are limited to that family,
Snowflake and Spark, so their "wrongly rejected" entries for other dialects are
deliberate and are not gaps. The original also accepts Snowflake's sampling
methods, `SEED`, `PIVOT ... IN (ANY)` and `PIVOT ... IN (SELECT ...)` in Spark, and
Spark's `TABLESAMPLE (BUCKET ...)` in Snowflake. Each stays in its own dialect.

MySQL keeps `ON t (a)` and `REFERENCES u (b)` spacing in rendering where the
original writes `ON t(a)` and `REFERENCES u(b)`, as the core does, and `VALUE` is
read as `VALUES` and rendered that way. Conditional comments `/*! ... */` are
dropped like other comments, while the original parses their contents as SQL.
`SHOW COLUMNS FROM t FROM db` keeps both `FROM` clauses as written. Syntax the
original's MySQL tests accept but real MySQL does not stays rejected:
`CREATE TRIGGER ... EXECUTE FUNCTION f()`, a string as an index name
(`INDEX 'name' (a)`), `@` inside an unquoted name, and `"table"` as a table
name, since `"..."` is a string in MySQL. Prefixes MySQL does not have, as in
`E'x'`, `R'x'` and `JSON 'x'`, read as a column with a string alias, where the
original reads a typed literal.

Redshift deliberately has fewer PostgreSQL rules than the name "built on
PostgreSQL" suggests: `ISNULL`, `NOTNULL`, `RETURNING`, `SET NAMES` and
`ADD COLUMN IF NOT EXISTS` are PostgreSQL-only here, as in the original.

`known_failures` entries added on purpose, because the original's behaviour is
wrong for the database: `SELECT 0xdeadBEEF` in Postgres renders as written, since
`0x1F` is an integer there and `X'1F'` is a bit string.

The "Wrongly accepted" column also has deliberate entries, where the original is
stricter than the dialect it models: words it reserves as aliases in every
dialect (such as `sort` and `anti`), `DROP PROJECTION` read as dropping a column, non-ASCII identifiers in ANSI, `FILTER` in
ANSI and Snowflake, `5 DIV` outside MySQL, its nesting limit of 50, `a^@b` in MySQL (v1 reads a
single `^@` token, but it is `a ^ @b`), and subscripts in Redshift, which v1
reads as bracket-quoted names although Redshift has them for `SUPER` values. Names starting with `_` in ANSI, which v1
rejects, are also accepted. Data types and special
function forms such as `TRIM` are still unvalidated, which also shows up there.

`tests/corpus/known_failures/snowflake.txt` and `spark.txt` started from empty
dialects, so they also list everything those dialects share with the unfinished parts of the grammar.

`SHOW` is deliberately lenient. What can be shown varies widely, so clauses are
accepted in any order and with any object, including forms the original only
allows in one dialect, such as `SHOW VARIABLES WHERE ...` outside MySQL.

Some of the original's `SHOW` renderings lose words: `SHOW GLOBAL VARIABLES LIKE 'x'`
renders as `SHOW LIKE`. v2 keeps them, so these are deliberate "Parsed but
different" entries, as are the clause orders it normalizes.

`USING` written before `ON` in `CREATE INDEX` keeps its position, while the
original moves it after the table, so those entries are deliberate differences.

Spark reads `"x"` as a string and backslashes as escapes, which is its default
behaviour. The original treats `"x"` as a quoted name and has no backslash
escapes in this dialect, so those entries are deliberate, as is `!!a`, which it
reads as an unsupported `!!` operator. Spark also accepts a `TABLESAMPLE` before
the table's alias and a column list after `PARTITION (...)`, both real Spark
syntax that the original rejects in this dialect, so those "Wrongly accepted"
entries are deliberate. `a::INT[3]` is a subscript on a cast in Spark, which the original
rejects. `DEFAULT ON NULL` in `PIVOT` stays Snowflake-only and
`RIGHT SEMI`/`RIGHT ANTI` are not Spark syntax, so those "Wrongly rejected" entries
are deliberate too.

Databricks is built on the Spark rules, as Redshift is on PostgreSQL's. Its
`known_failures` entries beyond Spark's are deliberate, with one exception noted
below. Strings with backslashes render as written, where the original unescapes
them, which also accounts for the `E'...'` and `U&'...'` entries. `DIV`,
`CREATE TABLE ... USING` and `TABLESAMPLE (100M)` are real Databricks syntax that
the original rejects in this dialect; `100M` currently parses as a name.
`OPTIMIZE TABLE` is ClickHouse's, `AT(...)`, `BEFORE(...)` and `CHANGES(...)` are
Snowflake's, `FOR SYSTEM_TIME AS OF` is BigQuery's and MSSQL's, and `123invalid` in
`ALTER OPERATOR` is PostgreSQL syntax, so those stay out. The exception is a
genuine gap: `t.15e29` and `t.12e34` lex `.15e29` as a number, because a lexer
hook cannot see the token before it, while `t.1two3` and `a.1c` work.

Hive is built on the Spark rules too, without Spark's `USING`, `* EXCEPT`,
`VALUES` as a table or optional `AS` in a CTE. Strings keep backslash escapes
as real Hive does, unlike v1's Hive mode. `"..."` delimits a name, as v1's tests
require, although real Hive reads it as a string. `ANALYZE` needs `TABLE`, as
in real Hive. `FROM` before `INSERT` is only accepted after `WITH`, as in v1.
`100M` in `TABLESAMPLE` parses as a name, as in Databricks.

PostgreSQL and Redshift lex the operators and strings their test files use, and
parse the DDL and utility statements of those files. Deliberate differences from
the original: strings render as written, so `E'...'` keeps its escapes and invalid
escapes are accepted (the real engine rejects them when the value is used); `COPY
... FROM STDIN;` with nothing after it has no payload, where the original keeps a
blank line as one; `ON CONFLICT` rejects the `WHERE` the real engine rejects without
a target; a statement list `CREATE ROLE a, b` (MySQL), bracket array literals in
`UNNEST`, `CASE ... END . 2`, `INSERT INTO BY NAME` and `UPDATE t SET where = 1` are
rejected, none of them being real syntax for those databases. Operators follow
PostgreSQL's own rules; `DO LANGUAGE N'x'` renders `N 'x'`, where the original reads
`N` as `nchar`. Redshift accepts `[name]` as a delimited identifier only where a
subscript cannot appear; it does not trim the spaces inside a `[ "a" ]` the way
the original does.

BigQuery, ClickHouse, DuckDB, MS SQL, Oracle, SQLite and Teradata each follow the
same rule: their `known_failures` entries are the shared core gaps, plus the
deliberate differences below. Where these claim what the real engine does, the
claim is from memory and was not checked against a live server.

- All of them: other families' syntax stays out even where v1 accepts it.
  Strings render as written, where the corpus records v1 unescaping them.
- BigQuery: `b'...'` honours backslashes when finding the closing quote.
  `OPTIONS` values on struct fields must be literals.
- ClickHouse: `OPTIMIZE` requires `TABLE`. `FORMAT`, `SETTINGS`, `PREWHERE`,
  `SAMPLE` and `GLOBAL` cannot be unquoted aliases. `SETTINGS` and `FORMAT` come
  after `FETCH`. `.5` and `1e3` render as written.
- DuckDB: `a ISNULL` is the operator, `&&` is accepted, and backtick names,
  `PERSISTENT` tables and `0b101` are rejected. Still missing, though v1 accepts
  them: trailing commas in columns and `VALUES`, `EXCLUDE (t.a)`, typed lambda
  parameters, and `ASOF JOIN`.
- MS SQL: `f(x : y)` is only accepted in `JSON_OBJECT` and `JSON_ARRAY`, and
  `[a]]b]` keeps its escape. v1's `tsql()` mode, with no `;` between statements,
  is not supported.
- Oracle: `EXECUTE IMMEDIATE` takes exactly one expression and `DELETE` one table.
- SQLite: precedence, `IS` and `ISNULL`, `NOT MATCH`/`NOT GLOB`, `PRAGMA a = b`
  and `CREATE TRIGGER IF NOT EXISTS` follow the real engine. Still missing:
  `VACUUM`.
- Teradata: `WITH [NO] DATA ON COMMIT ...` is accepted, `ON COMMIT DROP` and
  `EXECUTE ('x')` are not, and `CREATE MULTISET GLOBAL TEMPORARY` renders as
  written. `PRIMARY INDEX` is not implemented, as in v1.

Snowflake reads `IDENTIFIER(...)` as a single name part holding its source
text, taking one string or bind variable, where the original keeps a call with
any arguments. `TOP (n)` is still a function named `TOP`, since `TOP` takes a
number here. `SELECT f().*` is rejected, as are view options written
`WITH (...)`, which are not Snowflake syntax. `FROM VALUES (...)` renders as
written where the original adds parentheses, and `ADD IF NOT EXISTS` keeps the
words the original drops.

Shared grammar rules keep what was written where the original normalizes it, so
these are deliberate "Parsed but different" entries: `LIMIT ALL` (the original
drops it), `ABORT` with `WORK` or `TRANSACTION` (rendered as written), `IS JSON
WITH UNIQUE` without `KEYS`, and the `ALL` in `SELECT DISTINCT ALL name`, which reads
as a column named `all` in Spark, Databricks and ClickHouse. `SELECT * INTO TEMPORARY
UNLOGGED TABLE` stays rejected, as no engine accepts both words.

The corpus and the `sqlparser` dependency are v1 0.63, while the reference
checkout's test files are 0.64. Where they disagree the dialects follow 0.64, so
a few known failures are version differences rather than gaps.

Pipe syntax is a reusable rule, `grammar::parse_pipe_operators`, that a dialect
opts into through its hook and by lexing `|>`. As in the original, `|> INTERSECT`
and `|> EXCEPT` require `DISTINCT`.

`{:#}` pretty-prints as the original does, with these deliberate differences. `MERGE`,
`CREATE INDEX`, `EXPLAIN`, MS SQL bodies (`BEGIN ... END`, `IF`, `WHILE`,
`CREATE FUNCTION`/`PROCEDURE`/`TRIGGER`) and MySQL's `JSON_TABLE` have the multi-line
forms that the original's ignored `pretty_print` tests ask for, though the original
leaves them on one line. A column-only `CREATE TABLE` has no trailing spaces, `OVER ()`
stays on one line, and a string that is exactly one newline is not indented.

The corpus has no entry for `Core`, so it is compared with the original's ANSI
dialect under the name `ansi`. Treat those numbers as a rough reference: they
are not a target, and `ANSI` in this section means that comparison.

The tool compares rendered SQL, so it cannot see two trees that render the same.
`t ANTI JOIN u` now counts as matching, even though v2 reads `ANTI` as an alias
and the original reads it as a join.

## Testing

Write the failing test first. Assert on the real AST using the builders in
`tests/common/mod.rs`, with hand-derived expected values. Spans are ignored by
`Ident` equality, so test them explicitly where they matter.

## Open questions

These are not settled. Raise them rather than deciding silently.

- **Derived-dialect boilerplate.** A derived dialect must forward each hook its
  parent overrides. A macro may be needed as the number of hooks grows.
- **Exact round-trips.** Rendering writes `AS` for select-item aliases, `<>` for
  `!=` and `FULL JOIN` for `FULL OUTER JOIN`, as the original does. Data type
  names keep their source case, while the original upper-cases known types.
- **AST lifetime variance.** Extension types are generic associated types, which
  makes the AST invariant over `'a`. Comparing trees needs both to be parsed
  with a common lifetime, and a tree cannot be coerced to a shorter one.
- **Dialect configuration.** Dialects are stateless types, which leaves no
  place for settings such as MySQL's `ANSI_QUOTES`.
- **Stack margins.** Each new `Expr` variant adds to the debug-build frames of
  derived `Clone` and `Debug`. Box rare syntax so that `Expr` stays at 64 bytes
  (a test enforces it), and re-check `tests/tree_limits.rs` when adding nodes.
- **Performance.** Tokens are collected into one `Vec` up front, and keywords
  and operators are compared as strings. Profiling found failed keyword
  comparisons are cheap, since a length mismatch rejects them at once, so there
  is no case yet for interned token IDs. Profile before changing this, and
  re-run the benchmarks as the grammar grows, because today's comparison
  flatters v2.

## Known gaps

SQL must never be silently misunderstood: if a keyword cannot be parsed yet, it
should be rejected rather than read as an alias. The parity tool's "Parsed but
different" column is the place to look for violations.

Outside Spark, `t ANTI JOIN u` and `SEMI` read as table aliases, which is correct for
standard SQL and PostgreSQL. The original treats them as joins in every dialect,
so add them when adding the dialects that have them.

Comments are skipped and not kept in the AST. Underscores in numbers are
rejected. Array slices exist in PostgreSQL and DuckDB and `XMLTABLE` in PostgreSQL,
Oracle and Teradata; other JSON and XML table functions are per dialect.
`TABLESAMPLE` and `PIVOT` on a table exist only in Snowflake and Spark.

Spark rejects `INTERVAL` types with fields, such as `x::INTERVAL DAY`, because
they are not parsed yet. The original reads `DAY` there as an alias.

Reserved words other than `LEFT` and `RIGHT` cannot be unqualified function
names.

`ALTER` for objects other than tables is mostly missing outside the dialects
that have their own forms, and upsert syntax exists only where a dialect added it.
