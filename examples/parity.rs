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

//! Measures how much of the original crate's behavior this crate reproduces,
//! using the corpus generated from the original's test suite.
//!
//! ```sh
//! cargo run --release --example parity
//! ```

#[path = "../tests/common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::panic::{self, AssertUnwindSafe};

use corpus::{Case, Expected, Kind, V2_DIALECTS, parse_corpus, v2_result};
use sqlparser::ParseError;

/// How this crate did on one case.
enum Outcome {
    Exact,
    /// Parsed, but rendered differently from the original.
    Different {
        actual: String,
    },
    /// The original accepts this but this crate does not.
    WronglyRejected {
        message: String,
    },
    /// The original rejects this but this crate accepts it.
    WronglyAccepted {
        actual: String,
    },
    CorrectlyRejected,
    Panicked,
}

struct Evaluated<'c> {
    case: &'c Case,
    expected: &'c Expected,
    outcome: Outcome,
}

struct Report<'c> {
    dialect: &'static str,
    results: Vec<Evaluated<'c>>,
}

impl Report<'_> {
    fn count(&self, matches: impl Fn(&Evaluated<'_>) -> bool) -> usize {
        self.results.iter().filter(|r| matches(r)).count()
    }
}

fn main() {
    let cases = parse_corpus(include_str!("../tests/corpus/v1.corpus"));
    println!("Corpus cases: {}\n", cases.len());

    // A panic is reported as an outcome, so keep the output readable.
    panic::set_hook(Box::new(|_| {}));
    let reports: Vec<Report<'_>> = V2_DIALECTS.iter().map(|d| evaluate(d, &cases)).collect();
    let _ = panic::take_hook();

    print_summary(&reports);
    print_any_dialect(&reports);
    print_blockers(&reports);
    print_differences(&reports);
    write_failures(&reports);
}

fn evaluate<'c>(dialect: &'static str, cases: &'c [Case]) -> Report<'c> {
    let results = cases
        .iter()
        .map(|case| {
            let expected = &case.expected[dialect];
            let actual = panic::catch_unwind(AssertUnwindSafe(|| {
                v2_result(case.kind, dialect, &case.sql)
            }));
            Evaluated {
                case,
                expected,
                outcome: classify(&case.sql, expected, actual),
            }
        })
        .collect();
    Report { dialect, results }
}

fn classify(
    sql: &str,
    expected: &Expected,
    actual: std::thread::Result<Result<String, ParseError>>,
) -> Outcome {
    let Ok(actual) = actual else {
        return Outcome::Panicked;
    };
    match (expected, actual) {
        (Expected::Render(text), Ok(rendered)) if *text == rendered => Outcome::Exact,
        (Expected::Render(_), Ok(actual)) => Outcome::Different { actual },
        (Expected::Render(_), Err(error)) => Outcome::WronglyRejected {
            message: describe_error(sql, error),
        },
        (Expected::Reject, Ok(actual)) => Outcome::WronglyAccepted { actual },
        (Expected::Reject, Err(_)) => Outcome::CorrectlyRejected,
    }
}

/// The error message, naming the character for lexer errors.
fn describe_error(sql: &str, error: ParseError) -> String {
    if error.message == "unexpected character" {
        let found = sql[error.span.start as usize..].chars().next();
        return format!("unexpected character {}", found.unwrap_or_default());
    }
    error.message
}

/// True if two renderings differ only in letter case or whitespace.
fn is_cosmetic(expected: &str, actual: &str) -> bool {
    fn letters(text: &str) -> impl Iterator<Item = char> + '_ {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_lowercase())
    }
    letters(expected).eq(letters(actual))
}

fn is_cosmetic_difference(result: &Evaluated<'_>) -> bool {
    match (&result.outcome, result.expected) {
        (Outcome::Different { actual }, Expected::Render(text)) => is_cosmetic(text, actual),
        _ => false,
    }
}

fn percent(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

fn print_summary(reports: &[Report<'_>]) {
    println!(
        "| Dialect | Should parse | Same rendering | Same ignoring case and spacing | Parsed but different | Wrongly rejected | Should reject | Wrongly accepted | Panics |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for report in reports {
        let should_parse = report.count(|r| matches!(r.expected, Expected::Render(_)));
        let exact = report.count(|r| matches!(r.outcome, Outcome::Exact));
        let cosmetic = report.count(is_cosmetic_difference);
        let different = report.count(|r| matches!(r.outcome, Outcome::Different { .. })) - cosmetic;
        let rejected = report.count(|r| matches!(r.outcome, Outcome::WronglyRejected { .. }));
        let should_reject = report.count(|r| matches!(r.expected, Expected::Reject));
        let accepted = report.count(|r| matches!(r.outcome, Outcome::WronglyAccepted { .. }));
        let panics = report.count(|r| matches!(r.outcome, Outcome::Panicked));
        let close = exact + cosmetic;
        println!(
            "| {} | {should_parse} | {exact} ({:.1}%) | {close} ({:.1}%) | {different} | {rejected} | {should_reject} | {accepted} | {panics} |",
            report.dialect,
            percent(exact, should_parse),
            percent(close, should_parse),
        );
    }
    println!();
}

/// How much of what the original accepts in some dialect this crate handles in
/// some dialect. Unlike the per-dialect figures, this measures the grammar and
/// does not count the syntax that the original accepts in every dialect but
/// this crate deliberately keeps to the databases that have it.
fn print_any_dialect(reports: &[Report<'_>]) {
    let (mut accepted, mut handled) = (0, 0);
    let mut by_statement: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for index in 0..reports[0].results.len() {
        let is_accepted = reports
            .iter()
            .any(|r| matches!(r.results[index].expected, Expected::Render(_)));
        if !is_accepted {
            continue;
        }
        let is_handled = reports.iter().any(|r| {
            let result = &r.results[index];
            matches!(result.outcome, Outcome::Exact) || is_cosmetic_difference(result)
        });
        accepted += 1;
        handled += usize::from(is_handled);
        let case = reports[0].results[index].case;
        if case.kind == Kind::Statement {
            let entry = by_statement.entry(leading_keyword(&case.sql)).or_default();
            entry.0 += 1;
            entry.1 += usize::from(is_handled);
        }
    }
    println!("### Coverage in any dialect\n");
    println!(
        "{handled} of the {accepted} cases the original accepts in some dialect ({:.1}%) parse here in some dialect, with the same rendering apart from case and spacing.\n",
        percent(handled, accepted)
    );
    println!(
        "| Statement | Accepted by the original | Handled here | Coverage |\n|---|---:|---:|---:|"
    );
    let mut ranked: Vec<_> = by_statement.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
    for (keyword, (total, done)) in ranked.into_iter().take(20) {
        println!(
            "| `{keyword}` | {total} | {done} | {:.1}% |",
            percent(done, total)
        );
    }
    println!();
}

/// What an error says it found, which is usually the missing syntax.
fn blocker(message: &str) -> String {
    match message.rsplit_once(", found ") {
        Some((_, found)) => found.to_ascii_uppercase(),
        None => format!("({message})"),
    }
}

fn leading_keyword(sql: &str) -> String {
    sql.split_whitespace()
        .next()
        .unwrap_or("")
        .trim_start_matches('(')
        .to_ascii_uppercase()
}

/// Prints the entries with the most distinct SQL strings.
fn print_ranked(title: &str, groups: &BTreeMap<String, BTreeSet<&str>>, limit: usize) {
    let mut ranked: Vec<_> = groups.iter().map(|(k, v)| (v.len(), k)).collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    println!("### {title}\n\n| Count | Token |\n|---:|---|");
    for (count, key) in ranked.into_iter().take(limit) {
        println!("| {count} | `{}` |", key.replace('|', "\\|"));
    }
    println!();
}

fn print_blockers(reports: &[Report<'_>]) {
    let mut by_blocker: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut by_statement: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for result in reports.iter().flat_map(|r| &r.results) {
        if let Outcome::WronglyRejected { message } = &result.outcome {
            by_blocker
                .entry(blocker(message))
                .or_default()
                .insert(&result.case.sql);
            by_statement
                .entry(leading_keyword(&result.case.sql))
                .or_default()
                .insert(&result.case.sql);
        }
    }
    print_ranked("Wrongly rejected SQL by statement type", &by_statement, 15);
    print_ranked(
        "Wrongly rejected SQL by the token v2 stopped at",
        &by_blocker,
        30,
    );
}

fn print_differences(reports: &[Report<'_>]) {
    println!("### Sample SQL that v2 accepts but understands differently\n");
    let mut seen = BTreeSet::new();
    for result in reports.iter().flat_map(|r| &r.results) {
        if let Outcome::Different { actual } = &result.outcome
            && !is_cosmetic_difference(result)
            && let Expected::Render(text) = result.expected
            && seen.len() < 15
            && seen.insert(&result.case.sql)
        {
            println!("- original: `{text}`\n  v2:       `{actual}`");
        }
    }
    println!("\n### Sample SQL that v2 accepts but the original rejects\n");
    for report in reports {
        let mut shown = 0;
        for result in &report.results {
            if matches!(result.outcome, Outcome::WronglyAccepted { .. }) && shown < 12 {
                let sql = result.case.sql.split_whitespace().collect::<Vec<_>>();
                println!("- [{}] `{}`", report.dialect, sql.join(" "));
                shown += 1;
            }
        }
    }
    println!();
}

/// Writes every case that did not match, for browsing.
fn write_failures(reports: &[Report<'_>]) {
    let mut out = String::new();
    for report in reports {
        for result in &report.results {
            let detail = match &result.outcome {
                Outcome::Exact | Outcome::CorrectlyRejected => continue,
                Outcome::Different { actual } => format!("DIFFERENT: {actual}"),
                Outcome::WronglyRejected { message } => format!("REJECTED: {message}"),
                Outcome::WronglyAccepted { actual } => format!("ACCEPTED: {actual}"),
                Outcome::Panicked => "PANICKED".to_string(),
            };
            let _ = writeln!(
                out,
                "[{}] {}\n  {detail}\n",
                report.dialect, result.case.sql
            );
        }
    }
    let path = "target/parity-failures.txt";
    fs::write(path, out).expect("run from the repository root");
    println!("All failures written to {path}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_and_spacing_differences_are_cosmetic() {
        assert!(is_cosmetic(
            "CREATE TABLE t (a INT)",
            "CREATE TABLE t (a int)"
        ));
        assert!(is_cosmetic("t JOIN u USING(a)", "t JOIN u USING (a)"));
        assert!(is_cosmetic("DOUBLE(11,0)", "DOUBLE(11, 0)"));
    }

    #[test]
    fn different_tokens_are_not_cosmetic() {
        assert!(!is_cosmetic("t NATURAL JOIN u", "t AS NATURAL JOIN u"));
        assert!(!is_cosmetic("a = b", "a = b - -1"));
    }
}
