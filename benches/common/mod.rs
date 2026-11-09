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

//! The queries compared against the original crate.
//!
//! `simple_select`, `with_select`, `select_100_columns`,
//! `select_100_qualified_columns` and `large_statement` are taken unchanged
//! from the original's `sqlparser_bench`.

use sqlparser::dialect::Postgres;
use sqlparser_v1::dialect::PostgreSqlDialect;

pub struct Query {
    pub name: &'static str,
    pub sql: String,
}

fn join(count: usize, separator: &str, item: impl Fn(usize) -> String) -> String {
    (0..count).map(item).collect::<Vec<_>>().join(separator)
}

pub fn queries() -> Vec<Query> {
    let columns = join(100, ", ", |n| format!("column_{n}"));
    let qualified_columns = join(100, ", ", |n| format!("t{}.column_{n}", n % 5));
    let expressions = join(100, ", ", |n| {
        format!("(a_{n} + b_{n} * 2)::int AS c_{n}, CAST(d_{n} - 1 AS text) e_{n}")
    });
    let large_expressions = join(1000, ", ", |n| format!("FN_{n}(COL_{n})"));
    let large_tables = join(1000, " CROSS JOIN ", |n| format!("TABLE_{n}"));
    let large_where = join(1000, " OR ", |n| format!("COL_{n} = {n}"));
    let large_order = join(1000, ", ", |n| format!("COL_{n} DESC"));

    vec![
        Query {
            name: "simple_select",
            sql: "SELECT * FROM my_table WHERE 1 = 1".to_string(),
        },
        Query {
            name: "with_select",
            sql: "
                WITH derived AS (
                    SELECT MAX(a) AS max_a,
                           COUNT(b) AS b_num,
                           user_id
                    FROM MY_TABLE
                    GROUP BY user_id
                )
                SELECT * FROM my_table
                LEFT JOIN derived USING (user_id)
            "
            .to_string(),
        },
        Query {
            name: "select_100_columns",
            sql: format!("SELECT {columns} FROM my_table"),
        },
        Query {
            name: "select_100_qualified_columns",
            sql: format!("SELECT {qualified_columns} FROM t0, t1, t2, t3, t4"),
        },
        Query {
            name: "subqueries_and_union",
            sql: "
                WITH c AS (SELECT a FROM t1 WHERE a IN (SELECT b FROM t2))
                SELECT t.*, (SELECT MAX(x) FROM t3) AS m
                FROM (SELECT * FROM t4) t
                WHERE EXISTS (SELECT 1 FROM t5)
                GROUP BY t.a
                HAVING COUNT(*) > 1
                UNION ALL
                SELECT a, b FROM c
                ORDER BY 1
                LIMIT 10
            "
            .to_string(),
        },
        Query {
            name: "insert_update_delete_create",
            sql: "
                CREATE TABLE IF NOT EXISTS orders (
                    id int PRIMARY KEY,
                    customer varchar(255) NOT NULL,
                    total decimal(10, 2) DEFAULT 0,
                    UNIQUE (customer)
                );
                INSERT INTO orders (id, customer, total) VALUES (1, 'a', 10), (2, 'b', 20);
                UPDATE orders SET total = total + 1, customer = 'c' WHERE id = 1;
                DELETE FROM orders WHERE total > 100
            "
            .to_string(),
        },
        Query {
            name: "expressions_and_casts",
            sql: format!("SELECT {expressions} FROM t LIMIT 10 OFFSET 5"),
        },
        Query {
            name: "large_statement",
            sql: format!(
                "SELECT {large_expressions} FROM {large_tables} WHERE {large_where} ORDER BY {large_order}"
            ),
        },
    ]
}

/// Parses with the original crate, panicking if the query is rejected.
pub fn parse_v1(sql: &str) -> impl Sized {
    sqlparser_v1::parser::Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap()
}

/// Parses with this crate, panicking if the query is rejected.
pub fn parse_v2(sql: &str) -> impl Sized {
    sqlparser::parse_sql::<Postgres>(sql).unwrap()
}
