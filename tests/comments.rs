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

//! Comments recovered from the gaps between tokens: the cases the original
//! crate tests, comment syntax per dialect, and lookup by AST node.

#[path = "common/corpus.rs"]
#[allow(dead_code)]
mod corpus;

use corpus::{Kind, V2_DIALECTS, parse_corpus};
use sqlparser::ast::comments::{Comment, Comments};
use sqlparser::ast::{ExprKind, Location, SelectItemKind, SetExpr, Span, StatementKind};
use sqlparser::dialect::builtin::{parse_sql_as, parse_sql_with_comments_as};
use sqlparser::dialect::{DialectKind, MySql, Postgres, Snowflake};
use sqlparser::parse_sql_with_comments;

fn kind_of(dialect: &str) -> DialectKind {
    if dialect == "ansi" {
        DialectKind::Core
    } else {
        DialectKind::from_name(dialect).unwrap_or_else(|| panic!("unknown {dialect}"))
    }
}

fn comments_in<'a>(sql: &'a str) -> Comments<'a> {
    parse_sql_with_comments::<Postgres>(sql).unwrap().1
}

fn text<'a>(comments: &[sqlparser::ast::comments::CommentWithSpan<'a>]) -> Vec<&'a str> {
    comments
        .iter()
        .map(|comment| comment.comment.as_str())
        .collect()
}

fn single(content: &str) -> Comment<'_> {
    Comment::SingleLine {
        content,
        prefix: "--",
    }
}

/// Where a position falls, as (line, column).
fn at(sql: &str, offset: u32) -> (u32, u32) {
    let Location { line, column } = Span {
        start: offset,
        end: offset,
    }
    .start_location(sql);
    (line, column)
}

#[test]
fn comments_are_found_throughout_a_script_in_every_dialect() {
    let sql = "\n-- second line comment\nselect * from /* inline comment after `from` */ dual;\n\n/*select\nsome\nmore*/\n\n  -- end-of-script-with-no-newline";
    for dialect in V2_DIALECTS {
        let (_, comments) = parse_sql_with_comments_as(kind_of(dialect), sql).unwrap();
        let found: Vec<_> = comments.iter().collect();
        assert_eq!(found.len(), 4, "{dialect}");
        assert_eq!(found[0].comment, single(" second line comment"));
        assert_eq!(at(sql, found[0].span.start), (2, 1));
        assert_eq!(at(sql, found[0].span.end), (2, 23));
        assert_eq!(found[0].prev_token_end, None);
        assert_eq!(at(sql, found[0].next_token_start.unwrap()), (3, 1));

        assert_eq!(
            found[1].comment,
            Comment::MultiLine(" inline comment after `from` ")
        );
        assert_eq!(at(sql, found[1].span.start), (3, 15));
        assert_eq!(at(sql, found[1].span.end), (3, 48));
        assert_eq!(at(sql, found[1].prev_token_end.unwrap()), (3, 14));
        assert_eq!(at(sql, found[1].next_token_start.unwrap()), (3, 49));

        assert_eq!(found[2].comment, Comment::MultiLine("select\nsome\nmore"));
        assert_eq!(at(sql, found[2].span.end), (7, 7));
        assert_eq!(found[2].next_token_start, None);

        assert_eq!(found[3].comment, single(" end-of-script-with-no-newline"));
        assert_eq!(at(sql, found[3].prev_token_end.unwrap()), (3, 54));
        assert_eq!(found[3].next_token_start, None);
    }
}

#[test]
fn comments_between_tokens_share_their_neighbours() {
    let sql = "SELECT 1; /* a */\n\n-- b\n/* c */ SELECT 2";
    let comments = comments_in(sql);
    assert_eq!(comments.len(), 3);
    for comment in comments.iter() {
        assert_eq!(at(sql, comment.prev_token_end.unwrap()), (1, 10));
        assert_eq!(at(sql, comment.next_token_start.unwrap()), (4, 9));
    }
    assert_eq!(text(&comments), [" a ", " b", " c "]);
}

#[test]
fn a_script_of_only_comments_has_no_neighbours() {
    assert!(comments_in("").is_empty());
    let comments = comments_in("-- a\n/* b */");
    assert_eq!(text(&comments), [" a", " b "]);
    for comment in comments.iter() {
        assert_eq!(comment.prev_token_end, None);
        assert_eq!(comment.next_token_start, None);
    }
}

#[test]
fn a_line_comment_stops_before_its_line_ending() {
    let sql = "SELECT 1 -- a\r\n-- b\n";
    let comments = comments_in(sql);
    assert_eq!(text(&comments), [" a", " b"]);
    assert_eq!(comments[0].span.text(sql), "-- a");
}

#[test]
fn nothing_inside_a_string_or_identifier_is_a_comment() {
    assert!(comments_in("SELECT '-- no', \"/* no */\", $$-- no$$").is_empty());
}

#[test]
fn nested_block_comments_are_one_comment() {
    let comments = comments_in("SELECT /* a /* b */ c */ 1");
    assert_eq!(text(&comments), [" a /* b */ c "]);
}

#[test]
fn each_dialect_reports_its_own_comment_prefix() {
    let (_, comments) = parse_sql_with_comments::<MySql>("SELECT 1 # hash\n-- dash").unwrap();
    assert_eq!(
        comments[0].comment,
        Comment::SingleLine {
            content: " hash",
            prefix: "#"
        }
    );
    assert_eq!(comments[1].comment, single(" dash"));

    let (_, comments) = parse_sql_with_comments::<Snowflake>("SELECT 1 // slashes").unwrap();
    assert_eq!(
        comments[0].comment,
        Comment::SingleLine {
            content: " slashes",
            prefix: "//"
        }
    );
}

#[test]
fn comment_text_borrows_from_the_sql() {
    let sql = "SELECT 1 -- a";
    let comments = comments_in(sql);
    let content = comments[0].comment.as_str();
    assert!(sql.as_bytes().as_ptr_range().contains(&content.as_ptr()));
}

#[test]
fn find_selects_by_where_a_comment_starts() {
    let sql = "/* header */\n-- first\nSELECT 'hello' /* world */ FROM dual;\n-- second\nSELECT 123;\n-- trailing\n";
    let comments = comments_in(sql);
    let select = sql.find("SELECT").unwrap() as u32;
    let second = sql.rfind("SELECT").unwrap() as u32;
    assert_eq!(text(comments.find(..select)), [" header ", " first"]);
    assert_eq!(text(comments.find(select..second)), [" world ", " second"]);
    assert_eq!(text(comments.find(second..)), [" trailing"]);
    assert_eq!(comments.find(..).len(), 5);
    // A reversed range is empty rather than a panic.
    assert_eq!(comments.find(second..select).len(), 0);
}

#[test]
fn a_trailing_comment_belongs_to_the_code_before_it() {
    let sql = "SELECT 1; -- one\n-- two\nSELECT 2";
    let (statements, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    assert_eq!(text(comments.after(statements[0].span)), [" one"]);
    assert_eq!(text(comments.before(statements[1].span)), [" two"]);
    assert!(comments.before(statements[0].span).is_empty());
    assert!(comments.after(statements[1].span).is_empty());
}

#[test]
fn a_comment_before_code_on_its_line_leads_into_that_code() {
    let sql = "SELECT 1, /* about b */ b";
    let (_, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    assert_eq!(comments[0].trails, None);
}

#[test]
fn a_comment_after_an_opening_bracket_does_not_trail_it() {
    let (_, comments) = parse_sql_with_comments::<Postgres>("SELECT f( -- x\n a)").unwrap();
    assert_eq!(comments[0].trails, None);
}

#[test]
fn a_comment_before_closing_punctuation_trails_the_code() {
    let sql = "SELECT a /* x */, f(b /* y */)";
    let (_, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    let a_end = sql.find('a').unwrap() as u32 + 1;
    assert_eq!(comments[0].trails, Some(a_end));
    assert!(comments[1].trails.is_some());
}

#[test]
fn a_comment_at_the_end_of_the_text_trails() {
    let sql = "SELECT 1 -- end";
    let (statements, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    assert_eq!(text(comments.after(statements[0].span)), [" end"]);
}

#[test]
fn a_comment_on_its_own_line_does_not_trail() {
    let sql = "SELECT 1\n-- own line\n";
    let (statements, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    assert!(comments.after(statements[0].span).is_empty());
    assert_eq!(comments[0].trails, None);
}

#[test]
fn comments_can_be_looked_up_by_ast_node() {
    let sql = "-- about the query\nSELECT /* about a */ a, -- trailing a\n  /* about f */ f(/* inside */ x) FROM t";
    let (statements, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    assert_eq!(
        text(comments.before(statements[0].span)),
        [" about the query"]
    );
    assert_eq!(comments.within(statements[0].span).len(), 4);

    let StatementKind::Query(query) = &statements[0].kind else {
        panic!("not a query");
    };
    let SetExpr::Select(select) = &query.body else {
        panic!("not a select");
    };
    let SelectItemKind::Expr { expr: a, .. } = &select.projection[0].kind else {
        panic!("not an expression");
    };
    assert_eq!(text(comments.before(a.span)), [" about a "]);
    assert_eq!(text(comments.after(a.span)), [" trailing a"]);

    let SelectItemKind::Expr { expr: f, .. } = &select.projection[1].kind else {
        panic!("not an expression");
    };
    assert!(matches!(f.kind, ExprKind::Function { .. }));
    assert_eq!(text(comments.before(f.span)), [" about f "]);
    assert_eq!(text(comments.within(f.span)), [" inside "]);
}

#[test]
fn a_comment_in_a_copy_payload_is_data() {
    let sql = "COPY t FROM stdin;\n-- data\n\\.\n-- real";
    let (_, comments) = parse_sql_with_comments::<Postgres>(sql).unwrap();
    assert_eq!(text(&comments), [" real"]);
}

/// Replacing every comment with spaces must leave the same statements, or a
/// comment was reported that was really part of a token, or the reverse.
#[test]
fn removing_every_reported_comment_changes_nothing_across_the_corpus() {
    let cases = parse_corpus(include_str!("corpus/v1.corpus"));
    let mut with_comments = 0usize;
    for dialect in V2_DIALECTS {
        let kind = kind_of(dialect);
        for case in cases.iter().filter(|case| case.kind == Kind::Statement) {
            let sql = case.sql.as_str();
            let Ok((statements, comments)) = parse_sql_with_comments_as(kind, sql) else {
                continue;
            };
            let mut blanked = sql.to_string().into_bytes();
            let mut previous_end = 0;
            for comment in comments.iter() {
                let (start, end) = (comment.span.start as usize, comment.span.end as usize);
                assert!(start >= previous_end, "[{dialect}] overlap in {sql}");
                previous_end = end;
                let raw = comment.span.text(sql);
                assert!(
                    raw.contains(comment.comment.as_str()),
                    "[{dialect}] {raw:?} in {sql}"
                );
                // An optimizer hint is a comment that is also syntax, and
                // Oracle allows text before its `+`.
                if !comment.comment.as_str().contains('+') {
                    blanked[start..end].fill(b' ');
                }
                with_comments += 1;
            }
            let blanked = String::from_utf8(blanked).unwrap();
            let without = parse_sql_as(kind, &blanked)
                .unwrap_or_else(|e| panic!("[{dialect}] {sql}\n{blanked}\n{e}"));
            let render = |list: &[sqlparser::ast::Statement<'_, _>]| {
                list.iter().map(ToString::to_string).collect::<Vec<_>>()
            };
            assert_eq!(render(&without), render(&statements), "[{dialect}] {sql}");
        }
    }
    assert!(with_comments > 100, "only {with_comments} comments");
}
