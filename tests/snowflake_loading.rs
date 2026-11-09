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

//! Snowflake's hierarchical queries, pattern matching and data loading.

use sqlparser::ast::{
    BinaryOp, ConnectBy, ExprKind, ObjectName, SetExpr, SpecialExpr, StatementKind,
};
use sqlparser::dialect::builtin::{Builtin, BuiltinStatement};
use sqlparser::dialect::{Core, Dialect, Postgres, Snowflake};
use sqlparser::parser::Parser;
use sqlparser::{ParseError, parse_expr, parse_sql};

fn renders(sql: &str) -> String {
    let statements = parse_sql::<Snowflake>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips(sql: &str) {
    assert_eq!(renders(sql), sql);
}

fn rejected(sql: &str) -> bool {
    parse_sql::<Snowflake>(sql).is_err()
}

fn rejected_in<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

fn is_snowflake_statement(sql: &str) -> bool {
    matches!(
        parse_sql::<Snowflake>(sql).unwrap().remove(0).kind,
        StatementKind::Ext(BuiltinStatement::Snowflake(_))
    )
}

// Parser context

#[test]
fn context_is_set_only_inside_the_closure() {
    const FLAG: u32 = 1;
    let mut p = Parser::<Core>::new("a").unwrap();
    assert!(!p.has_context(FLAG));
    let inside = p.with_context(FLAG, |p| Ok::<_, ParseError>(p.has_context(FLAG)));
    assert!(inside.unwrap());
    assert!(!p.has_context(FLAG));
}

#[test]
fn context_is_restored_after_an_error() {
    const FLAG: u32 = 2;
    let mut p = Parser::<Core>::new("a").unwrap();
    let result = p.with_context(FLAG, |p| Err::<(), _>(p.expected("something")));
    assert!(result.is_err());
    assert!(!p.has_context(FLAG));
}

#[test]
fn contexts_nest() {
    let mut p = Parser::<Core>::new("a").unwrap();
    let both = p.with_context(1, |p| {
        p.with_context(2, |p| {
            Ok::<_, ParseError>(p.has_context(1) && p.has_context(2))
        })
    });
    assert!(both.unwrap());
}

// CONNECT BY

#[test]
fn connect_by_round_trips() {
    for sql in [
        "SELECT a FROM t CONNECT BY b = PRIOR c",
        "SELECT a FROM t CONNECT BY NOCYCLE b = PRIOR c",
        "SELECT a FROM t START WITH d = 'x' CONNECT BY b = PRIOR c ORDER BY a",
        "SELECT a FROM t CONNECT BY b = PRIOR c START WITH d = 1",
        "SELECT a FROM t WHERE e CONNECT BY b = PRIOR c GROUP BY a",
        "SELECT a FROM t CONNECT BY PRIOR b = c, d = PRIOR e",
    ] {
        round_trips(sql);
    }
}

#[test]
fn prior_is_an_operator_only_in_connect_by() {
    assert_eq!(renders("SELECT prior x FROM t"), "SELECT prior AS x FROM t");
    round_trips("SELECT prior FROM t");
    round_trips("SELECT a FROM t CONNECT BY b = PRIOR c + 1");
}

#[test]
fn prior_binds_tighter_than_addition() {
    let statements = parse_sql::<Snowflake>("SELECT a FROM t CONNECT BY PRIOR b + 1").unwrap();
    let StatementKind::Query(query) = &statements[0].kind else {
        panic!("expected a query");
    };
    let SetExpr::Select(select) = &query.body else {
        panic!("expected a select");
    };
    let ConnectBy::ConnectBy { relationships, .. } = &select.connect_by[0] else {
        panic!("expected CONNECT BY");
    };
    let ExprKind::Binary { left, op, .. } = &relationships[0].kind else {
        panic!("expected a binary expression");
    };
    assert_eq!(*op, BinaryOp::Add);
    assert!(matches!(
        left.kind,
        ExprKind::Special(ref special) if matches!(**special, SpecialExpr::Prior(_))
    ));
}

#[test]
fn connect_by_root_is_a_prefix_operator() {
    round_trips("SELECT CONNECT_BY_ROOT name FROM t");
    assert!(rejected("SELECT CONNECT_BY_ROOT FROM t"));
}

#[test]
fn connect_by_comes_before_where_is_an_error() {
    assert!(rejected("SELECT a FROM t CONNECT BY b WHERE c"));
}

#[test]
fn connect_by_is_not_in_other_dialects() {
    assert!(rejected_in::<Core>(
        "SELECT a FROM t CONNECT BY b = PRIOR c"
    ));
}

// MATCH_RECOGNIZE

fn pattern_round_trips(pattern: &str) {
    round_trips(&format!(
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN ({pattern}) DEFINE A AS 1 = 1)"
    ));
}

#[test]
fn patterns_round_trip() {
    for pattern in [
        "FOO",
        "FOO | \"BAR\" | baz42",
        "S1 S2",
        "S1* S2+ S3?",
        "S2*?",
        "S1{1} | S2{2,3} | S3{4,} | S4{,5}",
        "^ FOO $",
        "{- FOO -}",
        "PERMUTE(A, B, C)",
        "PERMUTE(A, B){1,2}",
        "S1 ( S2 )",
        "( {- S3 -} S4 )+",
        "^ S1 S2*? ( {- S3 -} S4 )+ | PERMUTE(S1, S2){1,2} $",
    ] {
        pattern_round_trips(pattern);
    }
}

#[test]
fn match_recognize_clauses_round_trip() {
    for sql in [
        "SELECT * FROM t MATCH_RECOGNIZE(PARTITION BY a ORDER BY b PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(MEASURES FIRST(a) AS f, COUNT(*) AS n PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(ONE ROW PER MATCH PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(ALL ROWS PER MATCH PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(ALL ROWS PER MATCH SHOW EMPTY MATCHES PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(ALL ROWS PER MATCH OMIT EMPTY MATCHES PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(ALL ROWS PER MATCH WITH UNMATCHED ROWS PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(AFTER MATCH SKIP PAST LAST ROW PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(AFTER MATCH SKIP TO NEXT ROW PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(AFTER MATCH SKIP TO FIRST s PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(AFTER MATCH SKIP TO LAST s PATTERN (X) DEFINE X AS c)",
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN (X Y) DEFINE X AS a, Y AS b) AS m",
    ] {
        round_trips(sql);
    }
}

#[test]
fn match_recognize_needs_pattern_and_define() {
    assert!(rejected("SELECT * FROM t MATCH_RECOGNIZE(DEFINE X AS c)"));
    assert!(rejected("SELECT * FROM t MATCH_RECOGNIZE(PATTERN (X))"));
    assert!(rejected(
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN () DEFINE X AS c)"
    ));
    assert!(rejected(
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN (X{}) DEFINE X AS c)"
    ));
}

#[test]
fn deeply_nested_patterns_are_an_error() {
    let depth = 5000;
    pattern_is_rejected(&format!("{}X{}", "(".repeat(depth), ")".repeat(depth)));
}

fn pattern_is_rejected(pattern: &str) {
    assert!(rejected(&format!(
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN ({pattern}) DEFINE A AS 1)"
    )));
}

#[test]
fn match_recognize_alone_is_an_alias() {
    round_trips("SELECT * FROM t match_recognize");
}

#[test]
fn match_recognize_is_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>(
        "SELECT * FROM t MATCH_RECOGNIZE(PATTERN (X) DEFINE X AS c)"
    ));
}

// Columns of staged files

#[test]
fn staged_file_columns_can_be_qualified() {
    round_trips("SELECT t.$1, t.$2:a FROM @s AS t");
    assert!(matches!(
        parse_expr::<Snowflake>("t.$1").unwrap().kind,
        sqlparser::ast::ExprKind::<Builtin>::Compound(ObjectName { .. })
    ));
}

#[test]
fn only_dollar_placeholders_can_be_qualified_parts() {
    assert!(rejected("SELECT t.?"));
    assert!(rejected_in::<Postgres>("SELECT t.$1"));
}

// COPY INTO

#[test]
fn copy_into_round_trips() {
    for sql in [
        "COPY INTO t FROM 'gcs://bucket/a.csv'",
        "COPY INTO a.b FROM @ns.stage",
        "COPY INTO a.b FROM @stage/day=18/f.parquet",
        "COPY INTO @~/path FROM 'gcs://bucket/a.csv'",
        "COPY INTO 's3://a/b' FROM (SELECT * FROM t)",
        "COPY INTO a (b, c) FROM (SELECT $1:\"A\"::VARIANT, $1::TEXT FROM @s)",
        "COPY INTO a FROM (SELECT t.$1:b AS c, 4, '5' AS d FROM @s AS t)",
        "COPY INTO a FROM 'gcs://b' AS c",
        "COPY INTO 's3://a' FROM db.s.t PARTITION BY ('d=' || e)",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql));
    }
}

#[test]
fn copy_into_options_round_trip() {
    for sql in [
        "COPY INTO a FROM 'b' FILES = ('c', 'd')",
        "COPY INTO a FROM 'b' PATTERN = '.*'",
        "COPY INTO a FROM 'b' VALIDATION_MODE = RETURN_ERRORS",
        "COPY INTO a FROM 'b' FILE_FORMAT=(COMPRESSION=AUTO NULL_IF=('x', 'y'))",
        "COPY INTO a FROM 'b' COPY_OPTIONS=(ON_ERROR=CONTINUE FORCE=true)",
        "COPY INTO a FROM 's3://b' STORAGE_INTEGRATION=i ENDPOINT='e' CREDENTIALS=(K='v') ENCRYPTION=(T='u')",
        "COPY INTO 's3://b' STORAGE_INTEGRATION=i FROM t",
        "COPY INTO a FROM 's3://b' STORAGE_INTEGRATION=i AS c",
        "COPY INTO a FROM 'b' FILES = ('c') PATTERN = 'd' FILE_FORMAT=(E=F) COPY_OPTIONS=(G=1)",
        "COPY INTO a FROM 'b' ON_ERROR=CONTINUE",
    ] {
        round_trips(sql);
    }
}

#[test]
fn copy_options_can_be_separated_by_commas() {
    assert_eq!(
        renders("COPY INTO a FROM 'b' FILE_FORMAT=(C=D, E=F)"),
        "COPY INTO a FROM 'b' FILE_FORMAT=(C=D E=F)"
    );
    assert_eq!(
        renders("COPY INTO a FROM 'b' FILES = ('c'), PATTERN = 'd'"),
        "COPY INTO a FROM 'b' FILES = ('c') PATTERN = 'd'"
    );
}

#[test]
fn copy_into_needs_a_source() {
    assert!(rejected("COPY INTO a"));
    assert!(rejected("COPY INTO a FROM"));
    assert!(rejected("COPY INTO a FROM 'b' FILES = 'c'"));
    assert!(rejected("COPY INTO a FROM 'b' PATTERN"));
}

#[test]
fn copy_into_is_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("COPY INTO a FROM 'b'"));
}

// CREATE STAGE and FILE FORMAT

#[test]
fn create_stage_round_trips() {
    for sql in [
        "CREATE STAGE s1.s2",
        "CREATE OR REPLACE STAGE s URL='s3://a' COPY_OPTIONS=(ON_ERROR=CONTINUE FORCE=true)",
        "CREATE OR REPLACE TEMPORARY STAGE IF NOT EXISTS s COMMENT='c'",
        "CREATE STAGE s DIRECTORY=(ENABLE=true REFRESH_ON_CREATE=false)",
    ] {
        round_trips(sql);
        assert!(is_snowflake_statement(sql));
    }
}

#[test]
fn create_file_format_round_trips() {
    for sql in [
        "CREATE FILE FORMAT f",
        "CREATE FILE FORMAT f TYPE=CSV FIELD_DELIMITER='|' SKIP_HEADER=1",
        "CREATE OR REPLACE TEMPORARY FILE FORMAT IF NOT EXISTS f COMMENT='c'",
        "CREATE VOLATILE FILE FORMAT f TYPE=JSON STRIP_OUTER_ARRAY=true",
    ] {
        round_trips(sql);
    }
    assert_eq!(
        renders("CREATE TEMP FILE FORMAT f TYPE=PARQUET"),
        "CREATE TEMPORARY FILE FORMAT f TYPE=PARQUET"
    );
}

#[test]
fn other_create_statements_are_unchanged() {
    round_trips("CREATE TABLE t (a INT)");
    round_trips("CREATE OR REPLACE VIEW v AS SELECT 1");
    assert!(!is_snowflake_statement("CREATE TABLE t (a INT)"));
}

#[test]
fn create_stage_is_not_in_other_dialects() {
    assert!(rejected_in::<Postgres>("CREATE STAGE s"));
    assert!(rejected_in::<Core>("CREATE FILE FORMAT f"));
}
