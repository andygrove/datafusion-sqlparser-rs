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

//! SQL this crate renders must parse back to the same text.

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{V2_DIALECTS, parse_corpus, v2_result};

#[test]
fn rendered_sql_parses_back_to_itself() {
    let cases = parse_corpus(include_str!("corpus/v1.corpus"));
    let mut failures = Vec::new();
    for dialect in V2_DIALECTS {
        for case in &cases {
            let Ok(first) = v2_result(case.kind, dialect, &case.sql) else {
                continue;
            };
            match v2_result(case.kind, dialect, &first) {
                Ok(second) if second == first => {}
                Ok(second) => failures.push(format!("[{dialect}] {first}\n  became: {second}")),
                Err(error) => failures.push(format!("[{dialect}] {first}\n  failed: {error}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} rendered statements did not parse back:\n{}",
        failures.len(),
        failures[..failures.len().min(20)].join("\n")
    );
}
