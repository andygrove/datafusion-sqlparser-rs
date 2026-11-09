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

//! `GRANT`, `REVOKE`, `MERGE` and the `DROP` forms beyond tables.

use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinStatement};
use sqlparser::dialect::{Core, Dialect, MySql, Postgres, Redshift, Snowflake};
use sqlparser::parse_sql;

fn renders<D: Dialect>(sql: &str) -> String {
    let statements = parse_sql::<D>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    statements[0].to_string()
}

fn round_trips<D: Dialect>(sql: &str) {
    assert_eq!(renders::<D>(sql), sql);
}

fn round_trips_everywhere(sql: &str) {
    round_trips::<Core>(sql);
    round_trips::<MySql>(sql);
    round_trips::<Postgres>(sql);
    round_trips::<Redshift>(sql);
    round_trips::<Snowflake>(sql);
}

fn rejected<D: Dialect>(sql: &str) -> bool {
    parse_sql::<D>(sql).is_err()
}

fn first<'a>(sql: &'a str) -> Statement<'a, Builtin> {
    parse_sql::<Postgres>(sql).unwrap().remove(0)
}

// GRANT

#[test]
fn grant_privileges_round_trip() {
    for sql in [
        "GRANT ALL ON t TO z",
        "GRANT ALL PRIVILEGES ON a, b TO z",
        "GRANT SELECT ON t TO z",
        "GRANT USAGE, SELECT, INSERT ON t TO z",
        "GRANT SELECT, UPDATE (a, b), DELETE ON t TO z",
        "GRANT APPLY MASKING POLICY ON ACCOUNT TO ROLE r",
        "GRANT CREATE DATA EXCHANGE LISTING ON ACCOUNT TO ROLE r",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn grant_privileges_are_upper_cased() {
    assert_eq!(
        renders::<Core>("grant select, usage on t to z"),
        "GRANT SELECT, USAGE ON t TO z"
    );
}

#[test]
fn grant_objects_round_trip() {
    for sql in [
        "GRANT ALL ON DATABASE d TO ROLE r",
        "GRANT ALL ON COMPUTE POOL p TO ROLE r",
        "GRANT ALL ON SCHEMA a, b TO z",
        "GRANT SELECT ON ALL TABLES IN SCHEMA a.b TO ROLE r",
        "GRANT SELECT ON FUTURE VIEWS IN SCHEMA a.b TO ROLE r",
        "GRANT USAGE ON FUNCTION a.b.c(INT) TO ROLE r",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn quoted_names_work_where_double_quotes_are_identifiers() {
    round_trips::<Core>("GRANT SELECT ON \"t\" TO z");
    round_trips::<Postgres>("DROP FUNCTION f(\"Role\" TEXT)");
}

#[test]
fn all_is_its_own_kind_of_privilege() {
    let StatementKind::Grant(grant) = first("GRANT ALL ON t TO z").kind else {
        panic!("expected GRANT");
    };
    assert_eq!(grant.privileges, Privileges::All { privileges: false });
}

#[test]
fn role_can_be_a_privilege_when_on_follows() {
    round_trips::<Core>("GRANT ROLE ON t TO z");
}

#[test]
fn revoke_records_grant_option_for() {
    let StatementKind::Revoke(revoke) = first("REVOKE GRANT OPTION FOR SELECT ON t FROM r").kind
    else {
        panic!("expected REVOKE");
    };
    assert!(revoke.grant_option_for);
}

#[test]
fn a_multi_word_type_is_not_a_name_and_a_type() {
    let StatementKind::DropRoutine(drop) = first("DROP FUNCTION f(DOUBLE PRECISION)").kind else {
        panic!("expected DROP FUNCTION");
    };
    assert_eq!(drop.routines[0].args.as_ref().unwrap()[0].name, None);
}

#[test]
fn grant_records_what_the_object_kind_is() {
    let StatementKind::Grant(grant) = first("GRANT ALL ON COMPUTE POOL p TO z").kind else {
        panic!("expected GRANT");
    };
    assert_eq!(grant.objects.unwrap().kind, ["COMPUTE", "POOL"]);
}

#[test]
fn grantees_round_trip() {
    for sql in [
        "GRANT ALL ON t TO a, b",
        "GRANT ALL ON t TO ROLE r",
        "GRANT ALL ON t TO USER u",
        "GRANT ALL ON t TO GROUP g",
        "GRANT ALL ON t TO SHARE s",
        "GRANT ALL ON t TO DATABASE ROLE r",
        "GRANT ALL ON t TO APPLICATION ROLE r",
        "GRANT ALL ON t TO PUBLIC",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn public_stands_alone() {
    assert!(rejected::<Core>("GRANT SELECT ON t TO public, a"));
}

#[test]
fn grant_options_round_trip() {
    for sql in [
        "GRANT ALL ON t TO ROLE r WITH GRANT OPTION",
        "GRANT ALL ON t TO ROLE r COPY CURRENT GRANTS",
        "GRANT ALL ON t TO ROLE r REVOKE CURRENT GRANTS",
        "GRANT ALL ON t TO ROLE r WITH GRANT OPTION COPY CURRENT GRANTS",
        "GRANT ALL ON t TO a AS b",
        "GRANT ALL ON t TO a, b WITH GRANT OPTION GRANTED BY c",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn granting_roles_round_trips() {
    for sql in [
        "GRANT ROLE r TO ROLE s",
        "GRANT ROLE r TO USER u",
        "GRANT DATABASE ROLE a.b.c TO ROLE a.b.d",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn grant_needs_its_parts() {
    assert!(rejected::<Core>("GRANT"));
    assert!(rejected::<Core>("GRANT ON t TO z"));
    assert!(rejected::<Core>("GRANT SELECT ON t"));
    assert!(rejected::<Core>("GRANT SELECT TO z"));
    assert!(rejected::<Core>("GRANT USAGE, SELECT, ON t TO z"));
}

// REVOKE

#[test]
fn revoke_round_trips() {
    for sql in [
        "REVOKE ALL PRIVILEGES ON a, b FROM c",
        "REVOKE ALL PRIVILEGES ON a, b FROM c CASCADE",
        "REVOKE SELECT ON t FROM r RESTRICT",
        "REVOKE GRANT OPTION FOR SELECT ON t FROM r",
        "REVOKE GRANT OPTION FOR ALL ON t FROM r CASCADE",
        "REVOKE SELECT ON t FROM r GRANTED BY s",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn revoke_uses_from() {
    assert!(rejected::<Core>("REVOKE SELECT ON t TO r"));
    assert!(rejected::<Core>("GRANT SELECT ON t FROM r"));
}

// MERGE

#[test]
fn merge_round_trips() {
    for sql in [
        "MERGE INTO a USING b ON c WHEN MATCHED THEN DELETE",
        "MERGE a USING b ON c WHEN MATCHED THEN DELETE",
        "MERGE INTO a AS x USING (SELECT 1) AS y ON x.i = y.i WHEN MATCHED THEN DELETE",
        "MERGE INTO a USING b ON c WHEN MATCHED AND d > 1 THEN UPDATE SET e = 1, a.f = 2",
        "MERGE INTO a USING b ON c WHEN MATCHED THEN UPDATE SET *",
        "MERGE INTO a USING b ON c WHEN MATCHED THEN UPDATE SET e = 1 WHERE f = 2 DELETE WHERE g = 3",
        "MERGE INTO a USING b ON c WHEN NOT MATCHED THEN INSERT VALUES (1, 'x')",
        "MERGE INTO a USING b ON c WHEN NOT MATCHED THEN INSERT (d, a.e) VALUES (1, 2)",
        "MERGE INTO a USING b ON c WHEN NOT MATCHED THEN INSERT VALUES (1) WHERE d = 2",
        "MERGE INTO a USING b ON c WHEN NOT MATCHED THEN INSERT *",
        "MERGE INTO a USING b ON c WHEN NOT MATCHED BY TARGET THEN INSERT VALUES (1)",
        "MERGE INTO a USING b ON c WHEN NOT MATCHED BY SOURCE THEN DELETE",
        "MERGE INTO a USING b ON c WHEN MATCHED THEN DO NOTHING",
        "MERGE INTO a USING b ON c WHEN MATCHED THEN DELETE WHEN NOT MATCHED THEN INSERT VALUES (1)",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn merge_actions_must_suit_the_clause() {
    assert!(rejected::<Core>(
        "MERGE INTO a USING b ON c WHEN NOT MATCHED THEN DELETE"
    ));
    assert!(rejected::<Core>(
        "MERGE INTO a USING b ON c WHEN NOT MATCHED THEN UPDATE SET d = 1"
    ));
    assert!(rejected::<Core>(
        "MERGE INTO a USING b ON c WHEN MATCHED THEN INSERT VALUES (1)"
    ));
    assert!(rejected::<Core>(
        "MERGE INTO a USING b ON c WHEN NOT MATCHED BY SOURCE THEN INSERT VALUES (1)"
    ));
}

#[test]
fn merge_needs_using_and_on() {
    assert!(rejected::<Core>(
        "MERGE INTO a ON c WHEN MATCHED THEN DELETE"
    ));
    assert!(rejected::<Core>(
        "MERGE INTO a USING b WHEN MATCHED THEN DELETE"
    ));
    assert!(rejected::<Core>(
        "MERGE INTO a USING b ON c WHEN MATCHED DELETE"
    ));
}

// DROP

#[test]
fn drop_role_and_user_round_trip() {
    round_trips_everywhere("DROP ROLE IF EXISTS a, b");
    round_trips_everywhere("DROP USER u");
}

#[test]
fn drop_functions_round_trip() {
    for sql in [
        "DROP FUNCTION f",
        "DROP FUNCTION IF EXISTS f(a INTEGER, IN b INTEGER = 1)",
        "DROP FUNCTION f(INT), g(TEXT, INT) CASCADE",
        "DROP PROCEDURE IF EXISTS p(a VARCHAR, IN b INTEGER = 1), q(OUT c INT)",
        "DROP FUNCTION f(DOUBLE PRECISION)",
        "DROP FUNCTION f()",
    ] {
        round_trips_everywhere(sql);
    }
}

#[test]
fn drop_function_arguments_can_have_defaults() {
    round_trips::<Core>("DROP FUNCTION f(a INT DEFAULT 1)");
}

#[test]
fn drop_index_can_name_its_table() {
    round_trips_everywhere("DROP INDEX a ON b");
    round_trips_everywhere("DROP INDEX a");
}

#[test]
fn postgres_family_objects_are_in_postgres() {
    for sql in [
        "DROP EXTENSION IF EXISTS a, b CASCADE",
        "DROP TYPE IF EXISTS a RESTRICT",
        "DROP COLLATION a, b.c",
        "DROP DOMAIN IF EXISTS a CASCADE",
    ] {
        round_trips::<Postgres>(sql);
        assert!(rejected::<Core>(sql), "{sql}");
        assert!(rejected::<MySql>(sql), "{sql}");
    }
}

#[test]
fn drop_policy_needs_a_table() {
    round_trips::<Postgres>("DROP POLICY IF EXISTS p ON t RESTRICT");
    round_trips::<Postgres>("DROP POLICY p ON t");
    assert!(rejected::<Postgres>("DROP POLICY p"));
    assert!(rejected::<Core>("DROP POLICY p ON t"));
}

#[test]
fn drop_operator_round_trips_in_postgres() {
    for sql in [
        "DROP OPERATOR && (INTEGER, INTEGER)",
        "DROP OPERATOR IF EXISTS ~ (NONE, BIT) CASCADE",
        "DROP OPERATOR < (INT, INT), - (INT, INT) RESTRICT",
        "DROP OPERATOR myschema.@@ (TEXT, TEXT)",
    ] {
        round_trips::<Postgres>(sql);
    }
    assert!(matches!(
        first("DROP OPERATOR + (INT, INT)").kind,
        StatementKind::Ext(BuiltinStatement::Postgres(_))
    ));
    assert!(rejected::<Core>("DROP OPERATOR + (INT, INT)"));
    assert!(rejected::<Postgres>("DROP OPERATOR (INT, INT)"));
    assert!(rejected::<Postgres>("DROP OPERATOR ((INT, INT)"));
    assert!(rejected::<Postgres>("DROP OPERATOR + INT"));
    assert!(rejected::<Postgres>("DROP OPERATOR + (INT)"));
}

#[test]
fn drop_trigger_is_in_mysql_and_postgres() {
    for sql in [
        "DROP TRIGGER t ON a",
        "DROP TRIGGER IF EXISTS t ON a CASCADE",
        "DROP TRIGGER IF EXISTS t",
    ] {
        round_trips::<Postgres>(sql);
        round_trips::<MySql>(sql);
        assert!(rejected::<Core>(sql), "{sql}");
        assert!(rejected::<Snowflake>(sql), "{sql}");
    }
}

#[test]
fn snowflake_objects_are_in_snowflake() {
    for sql in [
        "DROP STAGE IF EXISTS s",
        "DROP STREAM s",
        "DROP WAREHOUSE IF EXISTS w",
    ] {
        round_trips::<Snowflake>(sql);
        assert!(rejected::<Core>(sql), "{sql}");
    }
}

#[test]
fn drop_still_needs_a_known_object() {
    assert!(rejected::<Core>("DROP WIDGET a"));
}
