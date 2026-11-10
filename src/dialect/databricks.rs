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

use crate::ast::Statement;
use crate::dialect::spark::spark_family_rules;
use crate::dialect::Dialect;
use crate::keywords::Keyword;
use crate::parser::{Parser, ParserError};
use crate::tokenizer::{Token, Word};

/// A [`Dialect`] for [Databricks SQL](https://www.databricks.com/)
///
/// See <https://docs.databricks.com/en/sql/language-manual/index.html>.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DatabricksDialect;

impl Dialect for DatabricksDialect {
    spark_family_rules!();

    /// Databricks has functions named `exists` and `array`, as well as the
    /// `EXISTS (SELECT ...)` predicate.
    fn is_keyword_a_function_name(&self, keyword: Keyword, parser: &Parser) -> bool {
        match keyword {
            Keyword::EXISTS => !matches!(
                parser.peek_nth_token_ref(1).token,
                Token::Word(Word {
                    keyword: Keyword::SELECT | Keyword::WITH,
                    ..
                })
            ),
            Keyword::ARRAY => parser.peek_token_ref().token == Token::LParen,
            _ => false,
        }
    }

    /// See <https://docs.databricks.com/en/sql/language-manual/sql-ref-syntax-ddl-use-catalog.html>
    fn parse_use(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        let keyword =
            parser.parse_one_of_keywords(&[Keyword::CATALOG, Keyword::DATABASE, Keyword::SCHEMA]);
        Some(parser.parse_use_target(keyword))
    }

    /// Databricks permits [keywords](https://docs.databricks.com/aws/en/sql/language-manual/sql-ref-reserved-words) as table names.
    fn is_table_factor(&self, _kw: &Keyword, _parser: &mut Parser) -> bool {
        true
    }

    fn supports_numeric_prefix(&self) -> bool {
        true
    }

    /// <https://docs.databricks.com/gcp/en/delta/history#delta-time-travel-syntax>
    fn supports_table_versioning(&self) -> bool {
        true
    }

    /// See <https://docs.databricks.com/en/sql/language-manual/delta-optimize.html>
    fn supports_optimize_table(&self) -> bool {
        true
    }

    fn supports_data_type_collation(&self) -> bool {
        true
    }

    fn supports_string_literal_backslash_escape(&self) -> bool {
        true
    }

    fn supports_select_wildcard_replace(&self) -> bool {
        true
    }

    fn supports_from_first_select(&self) -> bool {
        true
    }
}
