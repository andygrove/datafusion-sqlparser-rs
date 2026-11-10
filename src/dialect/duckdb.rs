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

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use crate::ast::{AttachDuckDBDatabaseOption, Statement};
use crate::dialect::Dialect;
use crate::keywords::Keyword;
use crate::parser::{Parser, ParserError};
use crate::tokenizer::Token;

/// A [`Dialect`] for [DuckDB](https://duckdb.org/)
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DuckDbDialect;

// In most cases the redshift dialect is identical to [`PostgresSqlDialect`].
impl Dialect for DuckDbDialect {
    fn parse_statement(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        if parser.parse_keyword(Keyword::ATTACH) {
            return Some(parser.parse_attach_duckdb_database());
        }
        None
    }

    fn supports_cte_materialization(&self) -> bool {
        true
    }

    fn supports_string_literal_column_alias(&self, explicit: bool) -> bool {
        explicit
    }

    fn supports_string_literal_table_alias(&self, explicit: bool) -> bool {
        explicit
    }

    fn supports_trailing_commas(&self) -> bool {
        true
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        ch.is_alphabetic() || ch == '_'
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        ch.is_alphabetic() || ch.is_ascii_digit() || ch == '$' || ch == '_'
    }

    /// See <https://duckdb.org/docs/stable/sql/dialect/keywords_and_identifiers>
    fn identifier_quote_style(&self, _identifier: &str) -> Option<char> {
        Some('"')
    }

    fn supports_filter_during_aggregation(&self) -> bool {
        true
    }

    fn supports_group_by_expr(&self) -> bool {
        true
    }

    fn supports_bitwise_shift_operators(&self) -> bool {
        true
    }

    fn supports_tilde_like_match_operator(&self) -> bool {
        true
    }

    fn supports_aggregate_order_by_with_empty_args(&self) -> bool {
        true
    }

    fn supports_comparison_in_between_low_bound(&self) -> bool {
        true
    }

    fn supports_named_fn_args_with_eq_operator(&self) -> bool {
        true
    }

    fn supports_named_fn_args_with_assignment_operator(&self) -> bool {
        true
    }

    // DuckDB uses this syntax for `STRUCT`s.
    //
    // https://duckdb.org/docs/sql/data_types/struct.html#creating-structs
    fn supports_dictionary_syntax(&self) -> bool {
        true
    }

    // DuckDB uses this syntax for `MAP`s.
    //
    // https://duckdb.org/docs/sql/data_types/map.html#creating-maps
    fn support_map_literal_syntax(&self) -> bool {
        true
    }

    /// See <https://duckdb.org/docs/stable/sql/functions/lambda>
    fn supports_lambda_functions(&self) -> bool {
        true
    }

    /// Returns true if this dialect allows the `EXTRACT` function to use single quotes in the part being extracted.
    fn allow_extract_single_quotes(&self) -> bool {
        true
    }

    // DuckDB is compatible with PostgreSQL syntax for this statement,
    // although not all features may be implemented.
    fn supports_explain_with_utility_options(&self) -> bool {
        true
    }

    /// See DuckDB <https://duckdb.org/docs/sql/statements/load_and_install.html#load>
    fn supports_load_extension(&self) -> bool {
        true
    }

    // See DuckDB <https://duckdb.org/docs/sql/data_types/array.html#defining-an-array-field>
    fn supports_array_typedef_with_brackets(&self) -> bool {
        true
    }

    fn supports_from_first_select(&self) -> bool {
        true
    }

    /// See DuckDB <https://duckdb.org/docs/sql/query_syntax/orderby.html#order-by-all-examples>
    fn supports_order_by_all(&self) -> bool {
        true
    }

    fn supports_select_wildcard_exclude(&self) -> bool {
        true
    }

    /// DuckDB supports `NOTNULL` as an alias for `IS NOT NULL`,
    /// see DuckDB Comparisons <https://duckdb.org/docs/stable/sql/expressions/comparison_operators#between-and-is-not-null>
    fn supports_notnull_operator(&self) -> bool {
        true
    }

    /// See <https://duckdb.org/docs/extensions/overview>
    fn supports_install(&self) -> bool {
        true
    }

    /// See <https://duckdb.org/docs/sql/statements/attach#detach-syntax>
    fn supports_detach(&self) -> bool {
        true
    }

    /// See <https://duckdb.org/docs/sql/query_syntax/select#replace-clause>
    fn supports_select_wildcard_replace(&self) -> bool {
        true
    }

    fn supports_comma_separated_trim(&self) -> bool {
        true
    }

    fn supports_numeric_literal_underscores(&self) -> bool {
        true
    }

    fn supports_nested_comments(&self) -> bool {
        true
    }

    /// See <https://duckdb.org/docs/stable/sql/statements/alter_table>
    fn supports_add_column_if_not_exists(&self) -> bool {
        true
    }
}

impl Parser<'_> {
    /// Parse options for `ATTACH DUCKDB DATABASE` statement.
    pub fn parse_attach_duckdb_database_options(
        &mut self,
    ) -> Result<Vec<AttachDuckDBDatabaseOption>, ParserError> {
        if !self.consume_token(&Token::LParen) {
            return Ok(vec![]);
        }

        let mut options = vec![];
        loop {
            if self.parse_keyword(Keyword::READ_ONLY) {
                let boolean = if self.parse_keyword(Keyword::TRUE) {
                    Some(true)
                } else if self.parse_keyword(Keyword::FALSE) {
                    Some(false)
                } else {
                    None
                };
                options.push(AttachDuckDBDatabaseOption::ReadOnly(boolean));
            } else if self.parse_keyword(Keyword::TYPE) {
                let ident = self.parse_identifier()?;
                options.push(AttachDuckDBDatabaseOption::Type(ident));
            } else {
                return self
                    .expected_ref("expected one of: ), READ_ONLY, TYPE", self.peek_token_ref());
            };

            if self.consume_token(&Token::RParen) {
                return Ok(options);
            } else if self.consume_token(&Token::Comma) {
                continue;
            } else {
                return self.expected_ref("expected one of: ')', ','", self.peek_token_ref());
            }
        }
    }

    /// Parse `ATTACH DUCKDB DATABASE` statement.
    pub fn parse_attach_duckdb_database(&mut self) -> Result<Statement, ParserError> {
        let database = self.parse_keyword(Keyword::DATABASE);
        let if_not_exists = self.parse_keywords(&[Keyword::IF, Keyword::NOT, Keyword::EXISTS]);
        let database_path = self.parse_identifier()?;
        let database_alias = if self.parse_keyword(Keyword::AS) {
            Some(self.parse_identifier()?)
        } else {
            None
        };

        let attach_options = self.parse_attach_duckdb_database_options()?;
        Ok(Statement::AttachDuckDBDatabase {
            if_not_exists,
            database,
            database_path,
            database_alias,
            attach_options,
        })
    }
}
