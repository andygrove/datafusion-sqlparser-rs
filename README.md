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

# sqlparser

A next-generation SQL parser for Rust, intended to become v2 of
[datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs).

**Status: in development, and not ready to replace the original.** The API will
change. Across all dialects, 99.1% of the statements in the original's tests
written for a specific dialect are accepted by that dialect here. Across all of the
original's tests, a dialect agrees with
it 54–78% of the time. That figure cannot reach 100%, because the original does not
parse strictly: it accepts much of its syntax in every dialect, including SQL the
database would reject, and this crate deliberately does not.
[Parity with the original](docs/v1-parity.md) explains what these numbers mean and
lists which features are done and which are missing.

The proposal to make this v2 is
[apache/datafusion-sqlparser-rs#2893](https://github.com/apache/datafusion-sqlparser-rs/issues/2893).

## Why a v2

The original crate works well, but three problems are hard to fix incrementally:

- **Dialect logic lives in the core parser.** Dialects can only step in at a
  few entry points, so differences in the middle of a statement become
  `dialect_of!` checks and `supports_*` flags spread through a parser of more
  than 23,000 lines.
- **Strings are copied.** Every word token and identifier owns a `String`.
- **Reserved words leak between dialects.** The single shared grammar forbids
  words such as `sort`, `anti` and `top` as table aliases in every dialect, even
  where the database itself accepts them. v2 improves on this: each dialect
  reserves only its own words, so `SELECT 1 FROM t sort` parses in PostgreSQL
  while MySQL's reserved words stay specific to MySQL.

## Goals

- **A dialect-free core.** A core Pratt parser implements the common grammar
  and contains no knowledge of any specific dialect.
- **Dialects extend and override the core.** Grammar rules are hooks. A dialect
  overrides the rules it changes and inherits the rest.
- **Open to third parties.** Anyone can write a dialect in their own crate,
  including new tokens, operators and AST nodes, without changing this crate.
- **Zero-copy parsing.** Tokens and AST nodes borrow from the SQL text.
- **Feature parity with the original crate** (see below).
- **Measurably faster than the original crate**, shown by benchmarks against
  it.

## Scope

v2 is intended to support all existing functionality of the original crate:

- All of its dialects other than ANSI and Generic: BigQuery, ClickHouse,
  Databricks, DuckDB, Hive, MS SQL, MySQL, Oracle, PostgreSQL, Redshift,
  Snowflake, Spark, SQLite and Teradata. See
  [The core is not ANSI SQL](#the-core-is-not-ansi-sql).
- The full statement surface: queries, DML, DDL, DCL, triggers and the rest.
- Rendering the AST back to SQL, including pretty-printing.
- Source spans and comment handling.
- The AST visitor, `serde` support, `no_std` support and recursion protection.
- Custom dialects, including the `derive-dialect` feature's use cases.

Not started yet: `no_std`. Source spans cover identifiers,
expressions, statements, select items and table references, but not yet every
other node.

The original crate's test suite, about 1,700 tests, is the reference for what
parity means. The AST will differ, because it borrows from the input and has
extension slots, so v2 is not a source-compatible upgrade.

### Measuring parity

`tests/corpus/v1.corpus` holds every SQL string that the original crate's test
suite parses, with the original's result in each of its dialects. `tests/v1_corpus.rs`
runs it through both parsers: the original must match it exactly, and this crate
must match it apart from `tests/corpus/known_failures`, a list that can only
shrink. To see the current state:

```sh
cargo run --release --example parity
```

The corpus checks parsing and rendering, including SQL that must be rejected. It
does not check AST structure, because the two crates' ASTs differ by design. Current results
are in [Parity with the original](docs/v1-parity.md).

### Benchmarks

`benches/` compares this crate with the original on parse time and heap
allocations, using the Postgres dialect in both:

```sh
cargo bench --bench v1_timing
cargo bench --bench v1_allocations
```

Most of the queries come unchanged from the original's `sqlparser_bench`, but
today's results are still not a like-for-like comparison. v2 does not yet cover
the original's whole grammar, and the original checks for more syntax at each
step. Expect the timing gap to narrow as the grammar grows. The target is to
stay measurably faster at full feature parity. Current results are in
[Parity with the original](docs/v1-parity.md#performance).

## Dialects

`BigQuery`, `ClickHouse`, `Databricks`, `DuckDb`, `Hive`, `MsSql`, `MySql`,
`Oracle`, `Postgres`, `Redshift`, `Snowflake`, `Spark`, `Sqlite` and `Teradata`.
See [features by dialect](docs/dialect-features.md) and
[parity with the original](docs/v1-parity.md).

```rust
use sqlparser::dialect::{MySql, Postgres};
use sqlparser::parse_query;

let query = parse_query::<MySql>("SELECT `id` FROM users LIMIT 5, 10").unwrap();
assert!(query.limit.is_some());

// Backticks are MySQL syntax.
assert!(parse_query::<Postgres>("SELECT `id` FROM users").is_err());
```

To choose a dialect at runtime, for example from a config value:

```rust
use sqlparser::dialect::DialectKind;
use sqlparser::dialect::builtin::parse_query_as;

let kind = DialectKind::from_name("postgres").unwrap();
let query = parse_query_as(kind, "SELECT x::int FROM t").unwrap();
assert!(query.limit.is_none());
```

## The core is not ANSI SQL

The `Core` dialect is the grammar that the other dialects build on: the syntax
that most databases share. There is deliberately no ANSI dialect, and `Core`
makes no claim to conform to the ANSI SQL standard. The standard is not freely
available, so conformance could not be checked, and no database implements
exactly the standard: each is a mix of a subset and a superset of it. A real
database should be parsed with its own dialect.

## How dialects work

`Parser<'a, D>` is generic over a `Dialect`. Grammar rules are trait methods
whose default is the core rule, and rules always call each other through `D::`,
so an override applies everywhere the rule is used:

```rust,ignore
impl Dialect for MySql {
    type Ext = Builtin; // The AST shared by all built-in dialects

    fn parse_limit<'a>(p: &mut Parser<'a, Self>) -> PResult<Limit<'a, Builtin>> {
        // Handle `LIMIT 5, 10`, otherwise fall back to the core rule.
    }
}
```

All built-in dialects produce one AST type, so downstream tools handle a single
tree whichever dialect was used. A third-party dialect can use that AST too, or
supply its own `AstExt` to add nodes in the places it has slots for them:
expressions, statements, table factors, join conditions, and the data after
`SELECT` and `INSERT`. A new kind of join is a `JoinKind::Custom`. Other node
kinds cannot be extended yet.

The Pratt parser asks the dialect how to handle each infix token. The answer
holds the binding power and the parse action together, so they cannot drift
apart. Keywords and operators are matched by their source text rather than a
central enum, so a dialect can add new ones without touching the core.

## Design decisions

### Strings are `Cow<str>`, borrowed while parsing

The original crate stores `String` throughout, partly because it resolves escapes
while parsing: `'it''s'` becomes `it's`, which is not a slice of the input. This
crate keeps text exactly as written and resolves escapes only when asked, through
`Ident::value` and `StringLiteral::value`. Every string the parser produces is
then a slice of the SQL, held as `Cow::Borrowed`, so parsing never owns one.

The strings are `Cow<'a, str>` rather than `&'a str` so that code which builds
or rewrites trees can store text the input does not contain: a SQL generator
such as DataFusion's unparser, a mutable visitor that renames tables, or
`Deserialize`. Such trees can be `'static`.

The costs are real but small:

- `Cow<str>` takes 24 bytes where `&str` takes 16, so `Ident` grows from 32 bytes
  to 40. Measured over the benchmark queries, the bytes allocated rose by 2% to
  5%. The number of allocations did not change.
- `Ident` and `Literal` are no longer `Copy`. Cloning a borrowed value is still a
  copy, and only cloning an owned one allocates.
- Parse times stayed within a few percent of the `&str` version in both
  directions, which is within the benchmarks' noise.

The trade-offs are real:

- The SQL text must outlive a parsed AST, so a function that owns the SQL cannot
  return a tree that borrows from it.

Strings are `Cow<'a, str>`, so the parser borrows every one of them, while code
that builds or rewrites a tree, such as a mutable visit or `Deserialize`, can
store owned text.

### Dialects are strict

The original crate has one shared parser, so much of its syntax is accepted in
every dialect, including by databases that would reject it. Here, a database
family's syntax stays in that family's dialects: the PostgreSQL dialect rejects
backtick-quoted names, and only PostgreSQL has `CREATE OPERATOR`. Related dialects
share rules on purpose, as Redshift does with PostgreSQL, and Databricks and Hive
do with Spark.

The reasons:

- A parser that accepts SQL the database rejects gives a false answer to anyone
  using it to validate SQL, and hides mistakes such as using the wrong dialect.
- Syntax that leaks between dialects takes words away from them. A shared grammar
  has to reserve `sort`, `anti` and `top` everywhere, so `SELECT 1 FROM t sort`
  fails even in databases that allow it.
- A dialect can change without any risk to the others.

Where the original crate and the real database disagree, this crate follows the
database. These cases are recorded in `tests/corpus/known_failures`.

The trade-offs:

- There is no `GenericDialect` for parsing SQL of unknown origin. You have to
  know which database the SQL is for.
- Code that relies on one dialect accepting another's syntax will see errors
  when it moves to this crate.
- Parity numbers per dialect look lower than the grammar really is: 54–78% per
  dialect, against 96.6% in any dialect. Some of that gap is this decision and
  some is missing syntax, and the two have not been separated.

A dialect can still accept more than its database does. It is a type in your own
crate, so you can forward to whichever rules you need.

## Writing your own dialect

A dialect is a type that implements `Dialect`, and it can live in your own crate
with no changes to this one. Override the rules you want to change and the rest
are inherited from the core.

```rust,ignore
#[derive(Debug, Clone, PartialEq)]
struct Acme;

impl Dialect for Acme {
    type Ext = Builtin; // Reuse the AST of the built-in dialects

    // Read `??` as one token and leave the rest to the core lexer.
    fn lex_token(rest: &str) -> Option<LexResult> {
        rest.starts_with("??").then_some(Ok(Lexeme { kind: TokenKind::Op, len: 2 }))
    }

    // Make `??` a binary operator that binds like `+`.
    fn infix_rule<'a>(p: &Parser<'a, Self>) -> Option<InfixRule<'a, Self>> {
        if p.at_op("??") {
            return Some(InfixRule::binary(prec::ADD, BinaryOp::Named("??")));
        }
        grammar::infix_rule(p)
    }
}

let statements = parse_sql::<Acme>("SELECT a ?? b FROM t")?;
```

To add your own AST nodes, implement `AstExt` and set `type Ext = Self`. It has
slots for expressions, statements, table factors, join conditions, order-by
items, the end of a query, and the data after `SELECT` and `INSERT`. Use `NoExt`
for any you do not need.

```rust,ignore
impl AstExt for Acme {
    type Statement<'a> = Ping<'a>; // Your own type
    type Expr<'a> = NoExt;
    // ... the other slots
}

impl Dialect for Acme {
    type Ext = Self;

    fn parse_statement<'a>(p: &mut Parser<'a, Self>) -> PResult<Statement<'a, Self>> {
        if p.eat_keyword("PING") {
            let host = p.parse_ident()?;
            return Ok(StatementKind::Ext(Ping { host }).into());
        }
        grammar::parse_statement(p)
    }
}
```

To build on another dialect, forward each hook you want to that dialect's public
rule, such as `spark::parse_join`. Dialects don't inherit from each other, so
you write one short method per rule, as `src/dialect/databricks.rs` does. Rules
tied to `Builtin`, such as `spark::parse_create`, need `type Ext = Builtin`.

[`examples/custom_dialect.rs`](examples/custom_dialect.rs) is a complete dialect
that adds a token, an operator and a statement. Run it with
`cargo run --example custom_dialect`. `tests/custom_dialect.rs` also adds an
expression, a table factor and a join.

## Working with the AST

### Source positions

Expressions, statements, select items and table references record which part of
the SQL they came from while parsing, so a span includes keywords and
parentheses. `Expr`, `Statement` and `SelectItem` are structs holding a `kind`
and a `span`. Spans are ignored by `==`.

```rust
use sqlparser::ast::{ExprKind, Location};
use sqlparser::dialect::{Builtin, Postgres};
use sqlparser::{ast::Expr, parse_expr};

let sql = "(a + b) * 2";
let expr: Expr<'_, Builtin> = parse_expr::<Postgres>(sql).unwrap();
assert_eq!(expr.span.text(sql), "(a + b) * 2");

let ExprKind::Binary { left, .. } = &expr.kind else { unreachable!() };
assert_eq!(left.span.text(sql), "(a + b)");
assert_eq!(left.span.start_location(sql), Location { line: 1, column: 1 });
```

### Comments

`parse_sql_with_comments` also returns every comment, with its position. They
are kept beside the AST rather than on its nodes, because which node a comment
belongs to depends on the tool. `before`, `within` and `after` find the ones
around a node. A comment trailing code on the same line goes with that code, not
with whatever comes next.

```rust
use sqlparser::dialect::Postgres;
use sqlparser::parse_sql_with_comments;

let sql = "-- totals\nSELECT sum(a) FROM t; -- done\n-- next\nSELECT 2";
let (statements, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();

assert_eq!(comments.before(statements[0].span)[0].comment.as_str(), " totals");
assert_eq!(comments.after(statements[0].span)[0].comment.as_str(), " done");
assert_eq!(comments.before(statements[1].span)[0].comment.as_str(), " next");
```

Finding comments costs nothing unless you ask for them. Rendering does not
restore them: `to_string()` and `{:#}` leave them out.

### Rendering SQL

Every node implements `Display`. `{}` renders on one line and `{:#}` indents the
SQL over several lines:

```rust
use sqlparser::dialect::Postgres;
use sqlparser::parse_sql;

let statements = parse_sql::<Postgres>("SELECT a, b FROM t WHERE a > 1").unwrap();
assert_eq!(statements[0].to_string(), "SELECT a, b FROM t WHERE a > 1");
assert_eq!(
    format!("{:#}", statements[0]),
    "SELECT\n  a,\n  b\nFROM\n  t\nWHERE\n  a > 1"
);
```

### Visiting nodes

The `visitor` feature adds traversal of the whole AST, including mutable
traversal, through the `Visitor` and `VisitorMut` traits and helpers such as
`visit_relations` and `visit_expressions`. A third-party dialect's own nodes
join in with `#[derive(Visit, VisitMut)]`.

```rust,ignore
let statements = parse_sql::<Postgres>("SELECT a FROM foo WHERE x IN (SELECT y FROM bar)")?;

let mut tables = vec![];
let _ = visit_relations(&statements, |relation| {
    tables.push(relation.to_string());
    ControlFlow::<()>::Continue(())
});
assert_eq!(tables, ["foo", "bar"]);
```

A mutable visit can store borrowed or owned text, such as
`ident.text = ident.text.to_uppercase().into()`.

### Serializing

The `serde` feature implements `Serialize` and `Deserialize` for every AST node.
It also pulls in `stacker`, to handle deeply nested SQL safely.

Deserialized nodes own their strings, so a tree read back is not tied to the
serialized text:

```rust,ignore
let json = serde_json::to_string(&statements)?;
let back: Vec<Statement<'static, Builtin>> = serde_json::from_str(&json)?;
assert_eq!(back, statements);
```

`serde_json` limits nesting to 128 levels, so very deep SQL needs
`disable_recursion_limit`. Only do that for JSON you trust, because a tree built
from it can be deeper than the parser would allow, and dropping such a tree can
overflow the stack.

The default build has no dependencies.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

See [AGENTS.md](AGENTS.md) for design rules and open questions.

## License

Licensed under the [Apache License, Version 2.0](LICENSE). See [NOTICE](NOTICE)
for attribution.
