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

//! Builders for the expected syntax trees.
#![allow(dead_code)]

use sqlparser::ast::{
    ArrayType, BinaryOp, CastStyle, DataType, DataTypeSuffix, Expr, ExprKind, FunctionArg, Ident,
    Literal, ObjectName, Query, Select, SetExpr, Span, TableAlias, TableFactor, TableRef, UnaryOp,
};
use sqlparser::dialect::AstExt;

pub fn ident(text: &str) -> Ident<'_> {
    Ident {
        text: text.into(),
        quote: None,
        span: Span::default(),
    }
}

pub fn quoted(text: &str, quote: char) -> Ident<'_> {
    Ident {
        text: text.into(),
        quote: Some(quote),
        span: Span::default(),
    }
}

pub fn col<X: AstExt>(name: &str) -> Expr<'_, X> {
    ExprKind::Ident(ident(name)).into()
}

pub fn num<X: AstExt>(text: &str) -> Expr<'_, X> {
    ExprKind::Literal(Literal::Number(text.into())).into()
}

pub fn bin<'a, X: AstExt>(left: Expr<'a, X>, op: BinaryOp<'a>, right: Expr<'a, X>) -> Expr<'a, X> {
    ExprKind::Binary {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }
    .into()
}

pub fn unary<X: AstExt>(op: UnaryOp, expr: Expr<'_, X>) -> Expr<'_, X> {
    ExprKind::Unary {
        op,
        expr: Box::new(expr),
    }
    .into()
}

pub fn cast<'a, X: AstExt>(expr: Expr<'a, X>, ty: &'a str, style: CastStyle) -> Expr<'a, X> {
    ExprKind::Cast {
        expr: Box::new(expr),
        ty: Box::new(DataType {
            qualifier: Vec::new(),
            angle: None,
            name: ident(ty),
            args: Vec::new(),
            suffix: DataTypeSuffix::None,
            array: ArrayType::None,
        }),
        style,
    }
    .into()
}

pub fn object_name<'a>(parts: &[&'a str]) -> ObjectName<'a> {
    ObjectName {
        parts: parts.iter().map(|part| ident(part)).collect(),
    }
}

pub fn call<'a, X: AstExt>(name: &[&'a str], args: Vec<FunctionArg<'a, X>>) -> Expr<'a, X> {
    ExprKind::Function {
        name: object_name(name),
        args,
        options: None,
    }
    .into()
}

/// The `SELECT` of a query that has no set operations.
pub fn body<'q, 'a, X: AstExt>(query: &'q Query<'a, X>) -> &'q Select<'a, X> {
    match &query.body {
        SetExpr::Select(select) => select,
        other => panic!("expected a plain SELECT, found {other:?}"),
    }
}

pub fn into_select<X: AstExt>(query: Query<'_, X>) -> Select<'_, X> {
    match query.body {
        SetExpr::Select(select) => *select,
        other => panic!("expected a plain SELECT, found {other:?}"),
    }
}

pub fn named<'a, X: AstExt>(parts: &[&'a str]) -> TableFactor<'a, X> {
    TableFactor::Named(object_name(parts))
}

pub fn table_name<'q, 'a, X: AstExt>(table: &'q TableRef<'a, X>) -> &'q ObjectName<'a> {
    match &table.factor {
        TableFactor::Named(name) => name,
        other => panic!("expected a named table, found {other:?}"),
    }
}

pub fn parens<X: AstExt>(expr: Expr<'_, X>) -> Expr<'_, X> {
    ExprKind::Nested(Box::new(expr)).into()
}

pub fn implicit_alias(name: &str) -> TableAlias<'_> {
    TableAlias {
        name: ident(name),
        explicit: false,
        columns: Vec::new(),
        column_types: Vec::new(),
    }
}

pub fn explicit_alias(name: &str) -> TableAlias<'_> {
    TableAlias {
        name: ident(name),
        explicit: true,
        columns: Vec::new(),
        column_types: Vec::new(),
    }
}
