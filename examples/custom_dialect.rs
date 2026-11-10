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

//! A dialect defined outside the crate, using only the public API. It adds a
//! token, an operator and a statement, and inherits everything else.
//!
//! ```sh
//! cargo run --example custom_dialect
//! ```

use std::fmt;

use sqlparser::ast::{BinaryOp, Ident, Statement, StatementKind};
use sqlparser::dialect::{AstExt, Core, Dialect, NoExt};
use sqlparser::grammar::{self, prec};
use sqlparser::lexer::{LexResult, Lexeme, TokenKind};
use sqlparser::parser::{InfixRule, Parser};
use sqlparser::{PResult, parse_sql};

#[derive(Debug, Clone, PartialEq)]
struct Acme;

/// `PING host`, a statement only this dialect has.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    feature = "visitor",
    derive(sqlparser::ast::Visit, sqlparser::ast::VisitMut)
)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct Ping<'a> {
    host: Ident<'a>,
}

impl fmt::Display for Ping<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PING {}", self.host)
    }
}

/// The AST nodes this dialect adds. `NoExt` marks the slots it does not use.
impl AstExt for Acme {
    type Expr<'a> = NoExt;
    type Select<'a> = NoExt;
    type Statement<'a> = Ping<'a>;
    type Insert<'a> = NoExt;
    type TableFactor<'a> = NoExt;
    type JoinConstraint<'a> = NoExt;
    type OrderByItem<'a> = NoExt;
    type QueryTail<'a> = NoExt;
}

impl Dialect for Acme {
    type Ext = Self;

    /// Reads `??` as one token and leaves the rest to the core lexer.
    fn lex_token(rest: &str) -> Option<LexResult> {
        rest.starts_with("??").then_some(Ok(Lexeme {
            kind: TokenKind::Op,
            len: 2,
        }))
    }

    /// Makes `??` a binary operator that binds like `+`.
    fn infix_rule<'a>(p: &Parser<'a, Self>) -> Option<InfixRule<'a, Self>> {
        if p.at_op("??") {
            return Some(InfixRule::binary(prec::ADD, BinaryOp::Named("??".into())));
        }
        grammar::infix_rule(p)
    }

    /// Adds `PING host`, and otherwise parses statements as the core does.
    fn parse_statement<'a>(p: &mut Parser<'a, Self>) -> PResult<Statement<'a, Self>> {
        if p.eat_keyword("PING") {
            let host = p.parse_ident()?;
            return Ok(StatementKind::Ext(Ping { host }).into());
        }
        grammar::parse_statement(p)
    }
}

fn main() {
    for sql in ["PING db1", "SELECT a ?? b FROM t WHERE c > 1"] {
        let statements = parse_sql::<Acme>(sql).expect("valid Acme SQL");
        assert_eq!(statements[0].to_string(), sql);
        println!("{}", statements[0]);
    }

    // The new syntax exists only in this dialect.
    assert!(parse_sql::<Core>("PING db1").is_err());
    assert!(parse_sql::<Core>("SELECT a ?? b").is_err());
}
