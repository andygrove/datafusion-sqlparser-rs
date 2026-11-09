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

//! Builds the shared corpus from SQL logged while running the original crate's
//! tests. Run through `dev/generate_corpus.sh`.

#[path = "../tests/common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use std::collections::{BTreeMap, BTreeSet};
use std::{env, fs, process};

use corpus::{Case, Expected, Kind, MARK, V2_DIALECTS, v1_outcome, write_corpus};

/// Longer inputs are generated stress tests, which would bloat the corpus.
const MAX_SQL_LEN: usize = 4096;

fn main() {
    let (Some(log), Some(output)) = (env::args().nth(1), env::args().nth(2)) else {
        eprintln!("usage: generate_corpus <log> <output>");
        process::exit(2);
    };
    let log = fs::read(log).expect("cannot read the log");

    let mut candidates = BTreeSet::new();
    for record in log.split(|&b| b == 0) {
        let sql = String::from_utf8_lossy(record);
        let sql = sql.trim_end_matches('\n');
        if !sql.is_empty() && sql.len() <= MAX_SQL_LEN && !has_marker_line(sql) {
            candidates.insert(sql.to_string());
        }
    }

    let cases: Vec<Case> = candidates.iter().map(|sql| evaluate(sql)).collect();
    let rendered = cases
        .iter()
        .filter(|c| {
            c.expected
                .values()
                .any(|e| matches!(e, Expected::Render(_)))
        })
        .count();
    println!(
        "{} cases: {rendered} parsed by the original in at least one dialect",
        cases.len()
    );
    fs::write(output, write_corpus(&cases)).expect("cannot write the corpus");
}

fn has_marker_line(sql: &str) -> bool {
    sql.split('\n').any(|line| line.starts_with(MARK))
}

/// Records the original's result in every dialect. SQL that is not a statement
/// in any dialect is tried as an expression, and otherwise is a rejection.
fn evaluate(sql: &str) -> Case {
    let as_statement = outcomes(Kind::Statement, sql);
    if accepted_somewhere(&as_statement) {
        return case(Kind::Statement, sql, as_statement);
    }
    let as_expr = outcomes(Kind::Expr, sql);
    if accepted_somewhere(&as_expr) {
        return case(Kind::Expr, sql, as_expr);
    }
    case(Kind::Statement, sql, as_statement)
}

fn outcomes(kind: Kind, sql: &str) -> BTreeMap<String, Expected> {
    V2_DIALECTS
        .iter()
        .map(|dialect| (dialect.to_string(), v1_outcome(kind, dialect, sql)))
        .collect()
}

fn accepted_somewhere(outcomes: &BTreeMap<String, Expected>) -> bool {
    outcomes.values().any(|e| matches!(e, Expected::Render(_)))
}

fn case(kind: Kind, sql: &str, expected: BTreeMap<String, Expected>) -> Case {
    Case {
        kind,
        sql: sql.to_string(),
        expected,
    }
}
