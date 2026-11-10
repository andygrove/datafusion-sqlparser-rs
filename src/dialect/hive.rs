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

use crate::ast::{
    AddDropSync, ClusteredBy, CreateFunction, HiveDelimiter, HiveDistributionStyle, HiveFormat,
    HiveIOFormat, HiveLoadDataFormat, HiveRowDelimiter, HiveRowFormat, Msck, Statement, Use,
};
use crate::dialect::Dialect;
use crate::keywords::{Keyword, RESERVED_FOR_TABLE_FACTOR};
use crate::parser::{IsOptional::Mandatory, Parser, ParserError};
use crate::tokenizer::Token;

/// A [`Dialect`] for [Hive](https://hive.apache.org/).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HiveDialect {}

impl Dialect for HiveDialect {
    /// Hive accepts `USE DEFAULT` with no database named.
    fn parse_use(&self, parser: &mut Parser) -> Option<Result<Statement, ParserError>> {
        if parser.parse_keyword(Keyword::DEFAULT) {
            return Some(Ok(Statement::Use(Use::Default)));
        }
        None
    }

    fn parse_create_function(
        &self,
        parser: &mut Parser,
        _or_alter: bool,
        or_replace: bool,
        temporary: bool,
    ) -> Option<Result<Statement, ParserError>> {
        Some(
            parser
                .parse_hive_create_function(or_replace, temporary)
                .map(Into::into),
        )
    }

    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        (ch == '"') || (ch == '`')
    }

    fn supports_create_table_comment_after_columns(&self) -> bool {
        true
    }

    fn supports_create_table_plain_options(&self) -> bool {
        false
    }

    fn supports_create_table_clustered_by(&self) -> bool {
        true
    }

    fn supports_insert_columns_after_partition(&self) -> bool {
        true
    }

    fn supports_hash_comment(&self) -> bool {
        true
    }

    /// The alias is an `identifier`, see `selectItem` in
    /// <https://github.com/apache/hive/blob/master/parser/src/java/org/apache/hadoop/hive/ql/parse/SelectClauseParser.g>
    fn supports_string_literal_column_alias(&self, _explicit: bool) -> bool {
        false
    }

    /// The alias is an `identifier`, see `tableSource` in
    /// <https://github.com/apache/hive/blob/master/parser/src/java/org/apache/hadoop/hive/ql/parse/FromClauseParser.g>
    fn supports_string_literal_table_alias(&self, _explicit: bool) -> bool {
        false
    }

    /// See <https://cwiki.apache.org/confluence/display/Hive/LanguageManual+DDL#LanguageManualDDL-RulesforColumnNames>
    fn identifier_quote_style(&self, _identifier: &str) -> Option<char> {
        Some('`')
    }

    fn is_identifier_start(&self, ch: char) -> bool {
        ch.is_ascii_lowercase() || ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '$'
    }

    fn is_identifier_part(&self, ch: char) -> bool {
        ch.is_ascii_lowercase()
            || ch.is_ascii_uppercase()
            || ch.is_ascii_digit()
            || ch == '_'
            || ch == '$'
            || ch == '{'
            || ch == '}'
    }

    /// LIMIT is not reserved in Hive; identifiers may be nonReserved keywords.
    /// See <https://github.com/apache/hive/blob/4e9caba7c02a947a39c60de427acb7189c8c1fe8/parser/src/java/org/apache/hadoop/hive/ql/parse/IdentifiersParser.g#L945>
    fn is_table_factor(&self, kw: &Keyword, _parser: &mut Parser) -> bool {
        kw == &Keyword::LIMIT || !RESERVED_FOR_TABLE_FACTOR.contains(kw)
    }

    fn supports_filter_during_aggregation(&self) -> bool {
        true
    }

    fn supports_numeric_prefix(&self) -> bool {
        true
    }

    fn require_interval_qualifier(&self) -> bool {
        true
    }

    /// See <https://cwiki.apache.org/confluence/pages/viewpage.action?pageId=27362061#Tutorial-BuiltInOperators>
    fn supports_bang_not_operator(&self) -> bool {
        true
    }

    /// See <https://cwiki.apache.org/confluence/pages/viewpage.action?pageId=27362036#LanguageManualDML-Loadingfilesintotables>
    fn supports_load_data(&self) -> bool {
        true
    }

    /// See <https://cwiki.apache.org/confluence/display/hive/languagemanual+sampling>
    fn supports_table_sample_before_alias(&self) -> bool {
        true
    }

    /// See <https://cwiki.apache.org/confluence/pages/viewpage.action?pageId=30151323#EnhancedAggregation,Cube,GroupingandRollup-CubesandRollupsr>
    fn supports_group_by_with_modifier(&self) -> bool {
        true
    }

    // TODO: The parsing of the FROM keyword seems wrong, as it happens within the CTE.
    // See https://github.com/apache/datafusion-sqlparser-rs/issues/2236 for more details.
    /// See <https://hive.apache.org/docs/latest/language/common-table-expression/>
    fn supports_from_first_insert(&self) -> bool {
        true
    }

    /// See <https://hive.apache.org/docs/latest/language/languagemanual-types/>
    fn supports_map_literal_with_angle_brackets(&self) -> bool {
        true
    }
}

impl Parser<'_> {
    /// Parse `CREATE FUNCTION` for [Hive]
    ///
    /// [Hive]: https://cwiki.apache.org/confluence/display/hive/languagemanual+ddl#LanguageManualDDL-Create/Drop/ReloadFunction
    fn parse_hive_create_function(
        &mut self,
        or_replace: bool,
        temporary: bool,
    ) -> Result<CreateFunction, ParserError> {
        let name = self.parse_object_name(false)?;
        self.expect_keyword_is(Keyword::AS)?;

        let body = self.parse_create_function_body_string()?;
        let using = self.parse_optional_create_function_using()?;

        Ok(CreateFunction {
            or_alter: false,
            or_replace,
            temporary,
            name,
            function_body: Some(body),
            using,
            if_not_exists: false,
            args: None,
            return_type: None,
            behavior: None,
            called_on_null: None,
            parallel: None,
            security: None,
            leakproof: None,
            window: false,
            cost: None,
            rows: None,
            support: None,
            set_params: vec![],
            language: None,
            determinism_specifier: None,
            options: None,
            remote_connection: None,
        })
    }

    /// Parse `MSCK` statement.
    pub fn parse_msck(&mut self) -> Result<Msck, ParserError> {
        let repair = self.parse_keyword(Keyword::REPAIR);
        self.expect_keyword_is(Keyword::TABLE)?;
        let table_name = self.parse_object_name(false)?;
        let partition_action = self
            .maybe_parse(|parser| {
                let pa = match parser.parse_one_of_keywords(&[
                    Keyword::ADD,
                    Keyword::DROP,
                    Keyword::SYNC,
                ]) {
                    Some(Keyword::ADD) => Some(AddDropSync::ADD),
                    Some(Keyword::DROP) => Some(AddDropSync::DROP),
                    Some(Keyword::SYNC) => Some(AddDropSync::SYNC),
                    _ => None,
                };
                parser.expect_keyword_is(Keyword::PARTITIONS)?;
                Ok(pa)
            })?
            .unwrap_or_default();
        Ok(Msck {
            repair,
            table_name,
            partition_action,
        })
    }

    /// Parse Hive distribution style.
    ///
    /// TODO: Support parsing for `SKEWED` distribution style.
    pub fn parse_hive_distribution(&mut self) -> Result<HiveDistributionStyle, ParserError> {
        if self.parse_keywords(&[Keyword::PARTITIONED, Keyword::BY]) {
            self.expect_token(&Token::LParen)?;
            let columns =
                self.parse_comma_separated(|parser| parser.parse_column_def_inner(true))?;
            self.expect_token(&Token::RParen)?;
            Ok(HiveDistributionStyle::PARTITIONED { columns })
        } else {
            Ok(HiveDistributionStyle::NONE)
        }
    }

    /// Parse Hive formats.
    pub fn parse_hive_formats(&mut self) -> Result<Option<HiveFormat>, ParserError> {
        let mut hive_format: Option<HiveFormat> = None;
        loop {
            match self.parse_one_of_keywords(&[
                Keyword::ROW,
                Keyword::STORED,
                Keyword::LOCATION,
                Keyword::WITH,
                Keyword::USING,
            ]) {
                Some(Keyword::ROW) => {
                    hive_format
                        .get_or_insert_with(HiveFormat::default)
                        .row_format = Some(self.parse_row_format()?);
                }
                Some(Keyword::STORED) => {
                    self.expect_keyword_is(Keyword::AS)?;
                    if self.parse_keyword(Keyword::INPUTFORMAT) {
                        let input_format = self.parse_expr()?;
                        self.expect_keyword_is(Keyword::OUTPUTFORMAT)?;
                        let output_format = self.parse_expr()?;
                        hive_format.get_or_insert_with(HiveFormat::default).storage =
                            Some(HiveIOFormat::IOF {
                                input_format,
                                output_format,
                            });
                    } else {
                        let format = self.parse_file_format()?;
                        hive_format.get_or_insert_with(HiveFormat::default).storage =
                            Some(HiveIOFormat::FileFormat { format });
                    }
                }
                Some(Keyword::LOCATION) => {
                    hive_format.get_or_insert_with(HiveFormat::default).location =
                        Some(self.parse_literal_string()?);
                }
                Some(Keyword::WITH) => {
                    self.prev_token();
                    let properties = self
                        .parse_options_with_keywords(&[Keyword::WITH, Keyword::SERDEPROPERTIES])?;
                    if !properties.is_empty() {
                        hive_format
                            .get_or_insert_with(HiveFormat::default)
                            .serde_properties = Some(properties);
                    } else {
                        break;
                    }
                }
                Some(Keyword::USING) if self.dialect.supports_create_table_using() => {
                    let format = self.parse_identifier()?;
                    hive_format.get_or_insert_with(HiveFormat::default).storage =
                        Some(HiveIOFormat::Using { format });
                }
                Some(Keyword::USING) => {
                    // USING is not a table format keyword in this dialect; put it back
                    self.prev_token();
                    break;
                }
                None => break,
                _ => break,
            }
        }

        Ok(hive_format)
    }

    /// Parse Hive row format.
    pub fn parse_row_format(&mut self) -> Result<HiveRowFormat, ParserError> {
        self.expect_keyword_is(Keyword::FORMAT)?;
        match self.parse_one_of_keywords(&[Keyword::SERDE, Keyword::DELIMITED]) {
            Some(Keyword::SERDE) => {
                let class = self.parse_literal_string()?;
                Ok(HiveRowFormat::SERDE { class })
            }
            _ => {
                let mut row_delimiters = vec![];

                loop {
                    match self.parse_one_of_keywords(&[
                        Keyword::FIELDS,
                        Keyword::COLLECTION,
                        Keyword::MAP,
                        Keyword::LINES,
                        Keyword::NULL,
                    ]) {
                        Some(Keyword::FIELDS)
                            if self.parse_keywords(&[Keyword::TERMINATED, Keyword::BY]) =>
                        {
                            row_delimiters.push(HiveRowDelimiter {
                                delimiter: HiveDelimiter::FieldsTerminatedBy,
                                char: self.parse_identifier()?,
                            });

                            if self.parse_keywords(&[Keyword::ESCAPED, Keyword::BY]) {
                                row_delimiters.push(HiveRowDelimiter {
                                    delimiter: HiveDelimiter::FieldsEscapedBy,
                                    char: self.parse_identifier()?,
                                });
                            }
                        }
                        Some(Keyword::COLLECTION)
                            if self.parse_keywords(&[
                                Keyword::ITEMS,
                                Keyword::TERMINATED,
                                Keyword::BY,
                            ]) =>
                        {
                            row_delimiters.push(HiveRowDelimiter {
                                delimiter: HiveDelimiter::CollectionItemsTerminatedBy,
                                char: self.parse_identifier()?,
                            });
                        }
                        Some(Keyword::MAP)
                            if self.parse_keywords(&[
                                Keyword::KEYS,
                                Keyword::TERMINATED,
                                Keyword::BY,
                            ]) =>
                        {
                            row_delimiters.push(HiveRowDelimiter {
                                delimiter: HiveDelimiter::MapKeysTerminatedBy,
                                char: self.parse_identifier()?,
                            });
                        }
                        Some(Keyword::LINES)
                            if self.parse_keywords(&[Keyword::TERMINATED, Keyword::BY]) =>
                        {
                            row_delimiters.push(HiveRowDelimiter {
                                delimiter: HiveDelimiter::LinesTerminatedBy,
                                char: self.parse_identifier()?,
                            });
                        }
                        Some(Keyword::NULL)
                            if self.parse_keywords(&[Keyword::DEFINED, Keyword::AS]) =>
                        {
                            row_delimiters.push(HiveRowDelimiter {
                                delimiter: HiveDelimiter::NullDefinedAs,
                                char: self.parse_identifier()?,
                            });
                        }
                        Some(Keyword::NULL) => break,
                        _ => {
                            break;
                        }
                    }
                }

                Ok(HiveRowFormat::DELIMITED {
                    delimiters: row_delimiters,
                })
            }
        }
    }

    /// Parse optional `CLUSTERED BY` clause for Hive/Generic dialects.
    pub fn parse_optional_clustered_by(&mut self) -> Result<Option<ClusteredBy>, ParserError> {
        let clustered_by = if self.dialect.supports_create_table_clustered_by()
            && self.parse_keywords(&[Keyword::CLUSTERED, Keyword::BY])
        {
            let columns = self.parse_parenthesized_column_list(Mandatory, false)?;

            let sorted_by = if self.parse_keywords(&[Keyword::SORTED, Keyword::BY]) {
                self.expect_token(&Token::LParen)?;
                let sorted_by_columns = self.parse_comma_separated(|p| p.parse_order_by_expr())?;
                self.expect_token(&Token::RParen)?;
                Some(sorted_by_columns)
            } else {
                None
            };

            self.expect_keyword_is(Keyword::INTO)?;
            let num_buckets = self.parse_number_value()?.value;
            self.expect_keyword_is(Keyword::BUCKETS)?;
            Some(ClusteredBy {
                columns,
                sorted_by,
                num_buckets,
            })
        } else {
            None
        };
        Ok(clustered_by)
    }

    /// Parse optional Hive `INPUTFORMAT ... SERDE ...` clause used by LOAD DATA.
    pub fn parse_load_data_table_format(
        &mut self,
    ) -> Result<Option<HiveLoadDataFormat>, ParserError> {
        if self.parse_keyword(Keyword::INPUTFORMAT) {
            let input_format = self.parse_expr()?;
            self.expect_keyword_is(Keyword::SERDE)?;
            let serde = self.parse_expr()?;
            Ok(Some(HiveLoadDataFormat {
                input_format,
                serde,
            }))
        } else {
            Ok(None)
        }
    }
}
