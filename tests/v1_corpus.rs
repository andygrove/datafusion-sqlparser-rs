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

//! Runs the corpus from the original crate's test suite through both parsers.
//!
//! The original must match the corpus exactly, which proves the expectations
//! are right. This crate must match it except for a list of known failures,
//! which can only shrink: a new failure is a regression, and a known failure
//! that now passes must be removed from the list.
//!
//! After intentionally changing behavior:
//!
//! ```sh
//! UPDATE_KNOWN_FAILURES=1 cargo test --test v1_corpus
//! ```

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{env, fs};

use corpus::{
    Case, Expected, V2_DIALECTS, case_id, parse_corpus, v1_outcome, v2_matches, v2_result,
};

const SHOWN: usize = 20;

fn corpus() -> Vec<Case> {
    parse_corpus(include_str!("corpus/v1.corpus"))
}

fn known_failures_path(dialect: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/known_failures")
        .join(format!("{dialect}.txt"))
}

/// Maps the id of each known failure to its line in the file.
fn load_known_failures(dialect: &str) -> BTreeMap<String, String> {
    let text = fs::read_to_string(known_failures_path(dialect)).unwrap_or_default();
    text.lines()
        .map(|line| {
            let id = line.split(' ').next().unwrap_or_default();
            (id.to_string(), line.to_string())
        })
        .collect()
}

fn snippet(sql: &str) -> String {
    let single_line = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    single_line.chars().take(100).collect()
}

fn describe(expected: &Expected) -> &str {
    match expected {
        Expected::Render(text) => text,
        Expected::Reject => "<rejected>",
    }
}

#[test]
fn every_case_records_every_dialect() {
    for case in corpus() {
        assert_eq!(case.expected.len(), V2_DIALECTS.len(), "{}", case.sql);
    }
}

#[test]
fn original_crate_matches_the_corpus() {
    let mut mismatches = Vec::new();
    for case in corpus() {
        for (dialect, expected) in &case.expected {
            let actual = v1_outcome(case.kind, dialect, &case.sql);
            if actual != *expected {
                mismatches.push(format!(
                    "[{dialect}] {}\n  corpus:   {}\n  original: {}",
                    snippet(&case.sql),
                    describe(expected),
                    describe(&actual),
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} corpus entries disagree with the original:\n{}",
        mismatches.len(),
        mismatches[..mismatches.len().min(SHOWN)].join("\n")
    );
}

#[test]
fn this_crate_matches_the_corpus_apart_from_known_failures() {
    let cases = corpus();
    let update = env::var_os("UPDATE_KNOWN_FAILURES").is_some();
    let mut problems = Vec::new();

    for dialect in V2_DIALECTS {
        let mut known = load_known_failures(dialect);
        let mut failing = BTreeMap::new();
        for case in &cases {
            let expected = &case.expected[dialect];
            let actual = v2_result(case.kind, dialect, &case.sql);
            if v2_matches(expected, &actual) {
                continue;
            }
            let id = case_id(case.kind, &case.sql);
            let line = format!("{id} {}", snippet(&case.sql));
            if known.remove(&id).is_none() && !update {
                let actual = match &actual {
                    Ok(rendered) => rendered.clone(),
                    Err(error) => format!("<rejected: {error}>"),
                };
                problems.push(format!(
                    "regression [{dialect}] {}\n  expected: {}\n  actual:   {actual}",
                    snippet(&case.sql),
                    describe(expected),
                ));
            }
            failing.insert(id, line);
        }

        if update {
            let mut text = failing.into_values().collect::<Vec<_>>().join("\n");
            text.push('\n');
            let path = known_failures_path(dialect);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        } else {
            for line in known.values() {
                problems.push(format!("now passing [{dialect}] {line}"));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "{} differences from tests/corpus/known_failures. If they are intended, \
         run UPDATE_KNOWN_FAILURES=1 cargo test --test v1_corpus\n{}",
        problems.len(),
        problems[..problems.len().min(SHOWN)].join("\n")
    );
}
