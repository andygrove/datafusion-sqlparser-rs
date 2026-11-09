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

# Features by dialect

A hand-written overview of the main syntax each dialect supports. It is not
complete and can lag behind the code. For measured results, see
[Parity with the original](v1-parity.md).

| | `Core` | `MySql` | `Postgres` | `Redshift` | `Snowflake` | `Spark` | `Databricks` |
|---|---|---|---|---|---|---|---|
| `SELECT` items, aliases, `FROM`, `WHERE`, `ORDER BY`, `LIMIT` | yes | yes | yes | yes | yes | yes | yes |
| Expressions: arithmetic, comparison, `AND`/`OR`/`NOT`, parentheses, `CAST` | yes | yes | yes | yes | yes | yes | yes |
| Qualified names, function calls, joins, `WITH`, `GROUP BY`, `HAVING` | yes | yes | yes | yes | yes | yes | yes |
| Subqueries, `IN`, `EXISTS`, `UNION`/`EXCEPT`/`INTERSECT`, `t.*` | yes | yes | yes | yes | yes | yes | yes |
| `NULL`, booleans, `IS`, `LIKE`, `BETWEEN`, `CASE`, `DISTINCT`, window functions | yes | yes | yes | yes | yes | yes | yes |
| `INTERVAL`, typed literals (`DATE '...'`), `ARRAY[...]`, subscripts, tuples | yes | yes | yes | yes | yes | yes | yes |
| `EXTRACT`, `POSITION`, `SUBSTRING`, `TRIM`, `OVERLAY`, `CEIL`/`FLOOR ... TO`, `TRY_CAST` | yes | yes | yes | yes | yes | yes | yes |
| `ANY`/`SOME`/`ALL`, `SIMILAR TO`, `\|\|`, `\|`, `&`, `^`, named arguments | yes | yes | yes | yes | yes | yes | yes |
| Placeholders `$1` and `:name` | yes | yes | yes | yes | yes | yes | yes |
| Placeholder `?` | yes | yes | | | yes | yes | yes |
| `VALUES` as a query, `OFFSET`, `FETCH`, `FOR UPDATE`/`SHARE` | yes | yes | yes | yes | yes | yes | yes |
| Parenthesized joins, table functions, `AS t (a, b)` | yes | yes | yes | yes | yes | yes | yes |
| `(expr).field`, hex strings, `LATERAL`, `WITH ORDINALITY`, `WINDOW` | yes | yes | yes | yes | yes | yes | yes |
| `WITHIN GROUP`, `IGNORE NULLS`, `AT TIME ZONE`, `COLLATE`, `OVERLAPS`, `INT ARRAY` | yes | yes | yes | yes | yes | yes | yes |
| `ILIKE` | | | yes | yes | yes | yes | yes |
| Regex and JSON operators (`~`, `@>`, ...) | | | yes | yes |  |  |  |
| `->`, `->>` | | yes | yes | yes |  |  |  |
| Dollar-quoted strings | | | yes | yes | yes |  |  |
| `DISTINCT ON`, `int[]`, `ISNULL`/`NOTNULL` | | | yes | |  |  |  |
| `GROUPING SETS`, and `ROLLUP`/`CUBE` as groupings rather than function calls | | | yes | |  | yes | yes |
| `<<`, `>>` | | yes | yes | yes |  |  |  |
| `<=>`, `@variables`, `REGEXP`, `XOR`, `MATCH ... AGAINST`, `HIGH_PRIORITY` and similar | | yes | | |  |  |  |
| `GROUP BY ... WITH ROLLUP` | | yes | | |  | yes | yes |
| `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`, `DROP`, `ALTER TABLE` | yes | yes | yes | yes | yes | yes | yes |
| `CREATE TABLE`, `VIEW`, `INDEX`, `SCHEMA`, `DATABASE`, table constraints | yes | yes | yes | yes | yes | yes | yes |
| `GRANT`, `REVOKE`, `MERGE`, `DROP FUNCTION`/`PROCEDURE`/`ROLE`/`USER` | yes | yes | yes | yes | yes | yes | yes |
| `SHOW`, transactions, `USE`, `EXPLAIN`/`DESCRIBE`, `SET`, `CALL` | yes | yes | yes | yes | yes | yes | yes |
| `GENERATED` columns, constraint attributes, `CREATE SEQUENCE`, index options | yes | yes | yes | yes | yes | yes | yes |
| `DROP OPERATOR`, `EXTENSION`, `TYPE`, `DOMAIN`, `COLLATION`, `POLICY` | | | yes | |  |  |  |
| `DROP TRIGGER` | | yes | yes | |  |  |  |
| Rendering back to SQL (`to_string`) | yes | yes | yes | yes | yes | yes | yes |
| Quoted identifiers | `"a"` | `` `a` `` | `"a"` | `"a"` | `"a"` | `` `a` `` | `` `a` `` |
| `a DIV b` | | yes | | |  | yes | yes |
| `x::int` | | | yes | yes | yes | yes | yes |
| `LIMIT offset, count` | | yes | | |  |  |  |
| `SELECT TOP n` | | | | yes |  |  |  |
| `INSERT IGNORE`, `AUTO_INCREMENT` | | yes | | |  |  |  |
| `RETURNING` | | | yes | |  |  |  |
| `REGEXP`, `a:b.c` paths, `@stage` tables, `* EXCLUDE`, `QUALIFY`, `MINUS`, trailing commas, `SAMPLE` methods and seeds, `PIVOT ... IN (ANY)`, time travel, `ASOF JOIN`, `CONNECT BY`, `MATCH_RECOGNIZE`, `INSERT ALL`, `COPY INTO`, `CREATE STAGE`/`FILE FORMAT`, `CREATE TABLE`/`DATABASE` options, `BEGIN ... END`, `DECLARE`, `LIST`, `PUT`, `USE`, `ALTER SESSION` | | | | | yes |  |  |
| `!`, lambdas, `* EXCEPT (...)`, `ARRAY<T>`, `MAP<K, V>`, `STRUCT<...>`, `FROM VALUES`, `CREATE TABLE ... USING`, `OPTIONS`, `PARTITIONED BY`, `TBLPROPERTIES`, `WITH CUBE`, CTEs without `AS` | | | | |  | yes | yes |
| Pipe syntax (`\|> WHERE`, `\|> AGGREGATE`, `\|> JOIN`, ...) | | | | |  | yes | yes |
| `LATERAL VIEW`, `CLUSTER`/`DISTRIBUTE`/`SORT BY`, `INSERT OVERWRITE`, `PARTITION`, `AS (a, b)`, `LEFT SEMI`/`ANTI JOIN`, `TABLESAMPLE (BUCKET ...)`, `GROUP BY ALL`, `CACHE`/`UNCACHE TABLE` | | | | |  | yes | yes |
| `PIVOT`/`UNPIVOT`, `TABLESAMPLE` on tables | | | | | yes | yes | yes |
| `OPTIMIZE`, `USE CATALOG`/`SCHEMA`, `TIMESTAMP`/`VERSION AS OF`, names starting with a digit, `* REPLACE`, `FROM` first, `exists(array, lambda)` |  |  |  |  |  |  | yes |

## Other dialects

The table above covers the first seven dialects. The others add their own
syntax on top of the core:

- `Hive` (built on Spark's rules): `ROW FORMAT`, `STORED AS`, `CLUSTERED BY`,
  partition operations in `ALTER TABLE`, `MSCK`, `ANALYZE`, `LOAD DATA`,
  `SET HIVEVAR:`.
- `BigQuery`: hyphenated table names, triple-quoted and `b`/`r` strings,
  `STRUCT` and `ARRAY` literals, `SELECT AS STRUCT`, `UNNEST`, `SAFE_CAST`,
  `FOR SYSTEM_TIME AS OF`, scripting statements, pipe syntax.
- `ClickHouse`: `PREWHERE`, `SETTINGS`, `FORMAT`, `LIMIT n BY`, `WITH FILL`,
  `ARRAY JOIN`, table engines, parametric calls, its data types.
- `DuckDb`: struct and list literals, lambdas, slices, `* EXCLUDE`, `BY NAME`
  set operations, `CREATE MACRO`, `CREATE SECRET`, `ATTACH`.
- `MsSql`: `[name]` quoting, `TOP`, `APPLY`, `OUTPUT`, `FOR XML` and `FOR JSON`,
  table hints, `CONVERT`, procedures, triggers and control-of-flow statements.
- `Oracle`: `q'...'` strings, optimizer hints, `CONNECT BY`, `(+)` outer joins,
  `MATCH_RECOGNIZE`, `MINUS`, `EXECUTE IMMEDIATE`.
- `Sqlite`: `PRAGMA`, `ATTACH`, virtual tables, triggers, `INSERT OR ...`,
  upserts, `INDEXED BY`, `GLOB`, `MATCH`, `REGEXP`.
- `Teradata`: table options, `TOP`, `EXECUTE`, adjacent string joining.
