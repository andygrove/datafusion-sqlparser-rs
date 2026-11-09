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

//! Comments in parsed SQL, kept beside the AST rather than in it.
//!
//! Which node a comment belongs to is a judgement that differs between tools,
//! so the parser does not decide. [`Comments`] records every comment with its
//! position, and [`Comments::before`], [`Comments::within`] and
//! [`Comments::after`] find the ones around a node.
//!
//! The lexer drops comments, and they are recovered from the gaps between
//! tokens only on request, so parsing without them costs nothing.

use std::ops::{Bound, Deref, RangeBounds};

use crate::ast::Span;
use crate::dialect::Dialect;
use crate::lexer::{Token, TokenKind};

/// The text of a comment, without its markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Comment<'a> {
    /// A comment running to the end of its line. `content` excludes the line
    /// ending.
    SingleLine {
        content: &'a str,
        /// `--`, `#` or `//`. Empty for a marker of a third-party dialect.
        prefix: &'a str,
    },
    /// A comment between `/*` and `*/`.
    MultiLine(&'a str),
}

impl<'a> Comment<'a> {
    pub fn as_str(&self) -> &'a str {
        match self {
            Comment::SingleLine { content, .. } => content,
            Comment::MultiLine(content) => content,
        }
    }
}

/// A comment and its position. Offsets are bytes into the SQL text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentWithSpan<'a> {
    pub comment: Comment<'a>,
    /// The comment including its markers, but not a line comment's line ending.
    pub span: Span,
    /// The end of the previous token, or `None` before the first token.
    pub prev_token_end: Option<u32>,
    /// The start of the next token, or `None` after the last token.
    pub next_token_start: Option<u32>,
    /// If this comment trails code on its own line, the end of that code. A
    /// `,` or `;` before the comment does not count as the code, so for
    /// `SELECT 1; -- one` this is the end of `1`.
    pub trails: Option<u32>,
}

/// Every comment in some SQL text, in source order.
///
/// It dereferences to a slice, for iterating over all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comments<'a>(Vec<CommentWithSpan<'a>>);

impl<'a> Deref for Comments<'a> {
    type Target = [CommentWithSpan<'a>];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> Comments<'a> {
    /// The comments that start within `range` of byte offsets.
    pub fn find<R: RangeBounds<u32>>(&self, range: R) -> &[CommentWithSpan<'a>] {
        let start = self.index_of(match range.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(&offset) => u64::from(offset),
            Bound::Excluded(&offset) => u64::from(offset) + 1,
        });
        let end = self.index_of(match range.end_bound() {
            Bound::Unbounded => u64::MAX,
            Bound::Included(&offset) => u64::from(offset) + 1,
            Bound::Excluded(&offset) => u64::from(offset),
        });
        self.0.get(start..end).unwrap_or_default()
    }

    /// The comments that lead into the node at `span`: those directly before
    /// it, apart from any that trail the code before them.
    pub fn before(&self, span: Span) -> &[CommentWithSpan<'a>] {
        let end = self.index_of(u64::from(span.start));
        let in_gap = self.0[..end]
            .iter()
            .rev()
            .take_while(|comment| comment.next_token_start == Some(span.start))
            .count();
        let gap = &self.0[end - in_gap..end];
        let trailing = gap.iter().take_while(|c| c.trails.is_some()).count();
        &gap[trailing..]
    }

    /// The comments that start inside the node at `span`.
    pub fn within(&self, span: Span) -> &[CommentWithSpan<'a>] {
        self.find(span.start..span.end)
    }

    /// The comments that trail the node at `span` on its last line.
    pub fn after(&self, span: Span) -> &[CommentWithSpan<'a>] {
        let start = self.index_of(u64::from(span.end));
        let count = self.0[start..]
            .iter()
            .take_while(|comment| comment.trails == Some(span.end))
            .count();
        &self.0[start..start + count]
    }

    /// The index of the first comment starting at or after `offset`.
    fn index_of(&self, offset: u64) -> usize {
        self.0
            .partition_point(|comment| u64::from(comment.span.start) < offset)
    }
}

/// Finds the comments in the gaps between `tokens`.
pub(crate) fn collect<'a, D: Dialect>(source: &'a str, mut tokens: Vec<Token<'a>>) -> Comments<'a> {
    // A dialect can insert a payload token out of source order.
    tokens.sort_by_key(|token| token.span.start);

    let mut comments = Vec::new();
    let mut cursor = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        let gap_end = token.span.start as usize;
        let mut pos = cursor.min(gap_end);
        loop {
            let rest = &source[pos..gap_end];
            let trimmed = rest.trim_ascii_start();
            pos += rest.len() - trimmed.len();
            if trimmed.is_empty() {
                break;
            }
            let Some(Ok(len)) = D::lex_comment(trimmed) else {
                break;
            };
            let Some(raw) = trimmed.get(..len).filter(|raw| !raw.is_empty()) else {
                break;
            };
            let raw = raw.trim_end_matches(['\n', '\r']);
            let span = Span {
                start: pos as u32,
                end: (pos + raw.len()) as u32,
            };
            comments.push(CommentWithSpan {
                comment: parse_comment(raw),
                span,
                prev_token_end: index.checked_sub(1).map(|prev| tokens[prev].span.end),
                next_token_start: (token.kind != TokenKind::Eof).then_some(token.span.start),
                trails: trails(source, &tokens, index, span),
            });
            pos += len;
        }
        cursor = token.span.end as usize;
    }
    Comments(comments)
}

/// Splits a comment into its markers and text.
fn parse_comment(raw: &str) -> Comment<'_> {
    if let Some(block) = raw.strip_prefix("/*") {
        return Comment::MultiLine(block.strip_suffix("*/").unwrap_or(block));
    }
    for prefix in ["--", "//", "#"] {
        if let Some(content) = raw.strip_prefix(prefix) {
            return Comment::SingleLine { content, prefix };
        }
    }
    Comment::SingleLine {
        content: raw,
        prefix: "",
    }
}

/// The end of the code that the comment at `span`, in the gap before
/// `tokens[next]`, trails. A comment trails when it follows code on the same
/// line and no more code follows it there, apart from closing punctuation.
fn trails(source: &str, tokens: &[Token<'_>], next: usize, span: Span) -> Option<u32> {
    let has_newline =
        |start: u32, end: u32| source[start as usize..end as usize].contains(['\n', '\r']);
    let is_separator = |token: &Token<'_>| matches!(token.text, "," | ";");

    let previous = tokens[..next].last()?;
    if matches!(previous.text, "(" | "[") || has_newline(previous.span.end, span.start) {
        return None;
    }
    let following = &tokens[next];
    let ends_line = following.kind == TokenKind::Eof
        || has_newline(span.end, following.span.start)
        || is_separator(following)
        || matches!(following.text, ")" | "]");
    if !ends_line {
        return None;
    }
    let code = tokens[..next]
        .iter()
        .rev()
        .find(|token| !is_separator(token))?;
    Some(code.span.end)
}
