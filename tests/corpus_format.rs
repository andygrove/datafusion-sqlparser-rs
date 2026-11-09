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

//! The file format of the shared corpus.

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use std::collections::BTreeMap;

use corpus::{Case, Expected, Kind, case_id, parse_corpus, write_corpus};

fn render(text: &str) -> Expected {
    Expected::Render(text.to_string())
}

fn case(kind: Kind, sql: &str, expected: &[(&str, Expected)]) -> Case {
    Case {
        kind,
        sql: sql.to_string(),
        expected: expected
            .iter()
            .map(|(dialect, expected)| (dialect.to_string(), expected.clone()))
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn dialects_with_the_same_result_share_a_line() {
    let cases = [case(
        Kind::Statement,
        "SELECT 1",
        &[
            ("ansi", render("SELECT 1")),
            ("mysql", render("SELECT 1")),
            ("hive", Expected::Reject),
        ],
    )];
    assert_eq!(
        write_corpus(&cases),
        "%%%% stmt\nSELECT 1\n%%%% render ansi mysql\nSELECT 1\n%%%% reject hive\n"
    );
}

#[test]
fn different_renderings_get_separate_groups() {
    let text = write_corpus(&[case(
        Kind::Statement,
        "select 1",
        &[("ansi", render("SELECT 1")), ("hive", render("SELECT 2"))],
    )]);
    assert_eq!(
        text,
        "%%%% stmt\nselect 1\n%%%% render ansi\nSELECT 1\n%%%% render hive\nSELECT 2\n"
    );
}

#[test]
fn written_corpus_parses_back_unchanged() {
    let cases = vec![
        case(
            Kind::Statement,
            "SELECT 1",
            &[("ansi", render("SELECT 1")), ("hive", Expected::Reject)],
        ),
        case(Kind::Expr, "a + b", &[("mysql", render("a + b"))]),
        case(
            Kind::Statement,
            "SELECT\n  'two\nlines'",
            &[("ansi", render("SELECT 'two\nlines'"))],
        ),
        case(Kind::Statement, ";", &[("ansi", render(""))]),
    ];
    assert_eq!(parse_corpus(&write_corpus(&cases)), cases);
}

#[test]
fn empty_corpus_has_no_cases() {
    assert_eq!(parse_corpus(""), Vec::<Case>::new());
}

#[test]
#[should_panic(expected = "corpus")]
fn malformed_corpus_is_reported() {
    parse_corpus("not a corpus\n");
}

#[test]
fn case_ids_are_stable() {
    assert_eq!(case_id(Kind::Statement, "SELECT 1"), "cc43b965621874ae");
}

#[test]
fn case_ids_depend_on_kind_and_sql() {
    assert_ne!(case_id(Kind::Statement, "a"), case_id(Kind::Expr, "a"));
    assert_ne!(case_id(Kind::Statement, "a"), case_id(Kind::Statement, "b"));
}
