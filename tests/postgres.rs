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

//! PostgreSQL's object DDL and utility statements.

use sqlparser::ast::ExprKind;
use sqlparser::ast::StatementKind;
use sqlparser::ast::*;
use sqlparser::dialect::builtin::{Builtin, BuiltinExpr, BuiltinStatement};
use sqlparser::dialect::postgres::{
    CopyTarget, DefArg, FunctionBody, PartitionBound, PostgresExpr, PostgresStatement, TriggerEvent,
};
use sqlparser::dialect::{Core, MySql, Postgres};
use sqlparser::{parse_expr, parse_sql};

/// Parses `sql` and checks that it renders as `sql` and reparses identically.
fn roundtrip(sql: &str) -> Statement<'_, Builtin> {
    let mut statements = parse_sql::<Postgres>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    let statement = statements.remove(0);
    assert_eq!(statement.to_string(), sql);
    statement
}

fn rejected(sql: &str) {
    assert!(parse_sql::<Postgres>(sql).is_err(), "accepted: {sql}");
}

fn pg_statement(statement: Statement<'_, Builtin>) -> PostgresStatement<'_> {
    match statement.kind {
        StatementKind::Ext(BuiltinStatement::Postgres(statement)) => statement,
        other => panic!("not a Postgres statement: {other:?}"),
    }
}

#[test]
fn operators_round_trip() {
    for sql in [
        "CREATE OPERATOR + (FUNCTION = f, COMMUTATOR = >, NEGATOR = >=)",
        "CREATE OPERATOR s1.+ (FUNCTION = f)",
        "CREATE OPERATOR s1.s3.* (FUNCTION = f)",
        "CREATE OPERATOR ! (FUNCTION = f)",
        "CREATE OPERATOR # (FUNCTION = f)",
        "CREATE OPERATOR @ (FUNCTION = f)",
        "CREATE OPERATOR <> (FUNCTION = func)",
        "CREATE OPERATOR myschema.@@ (PROCEDURE = myschema.my_proc, LEFTARG = TIMESTAMP WITH TIME ZONE, RIGHTARG = VARCHAR(255), COMMUTATOR = schema.>, NEGATOR = schema.<=, RESTRICT = myschema.sel_func, JOIN = myschema.join_func, HASHES, MERGES)",
        "CREATE OPERATOR FAMILY my_family USING btree",
        "CREATE OPERATOR FAMILY myschema.test_family USING gist",
        "CREATE OPERATOR CLASS int4_ops DEFAULT FOR TYPE INT4 USING btree FAMILY int4_family AS OPERATOR 1 <",
        "CREATE OPERATOR CLASS gist_ops FOR TYPE geometry USING gist AS OPERATOR 1 <<, FUNCTION 1 gist_consistent(internal, geometry, INT4), STORAGE box",
        "CREATE OPERATOR CLASS test_ops FOR TYPE INT4 USING btree AS FUNCTION 1 (INT4, INT4) btcmp(INT4, INT4)",
        "CREATE OPERATOR CLASS test_ops FOR TYPE INT4 USING btree AS FUNCTION 1 my_func",
        "CREATE OPERATOR CLASS test_ops FOR TYPE INT4 USING gist AS OPERATOR 1 < (INT4, INT4)",
        "CREATE OPERATOR CLASS test_ops FOR TYPE INT4 USING gist AS OPERATOR 1 < FOR SEARCH",
        "CREATE OPERATOR CLASS test_ops FOR TYPE INT4 USING gist AS OPERATOR 2 <<-> FOR ORDER BY float_ops",
        "ALTER OPERATOR + (INTEGER, INTEGER) OWNER TO CURRENT_USER",
        "ALTER OPERATOR ~ (NONE, BIT) OWNER TO SESSION_USER",
        "ALTER OPERATOR myschema.@@ (TEXT, TEXT) SET SCHEMA other_schema",
        "ALTER OPERATOR && (INTEGER, INTEGER) SET (COMMUTATOR = &&, NEGATOR = >)",
        "ALTER OPERATOR && (TEXT, TEXT) SET (RESTRICT = NONE, JOIN = my_joinsel)",
        "ALTER OPERATOR < (INTEGER, INTEGER) SET (MERGES)",
        "ALTER OPERATOR <= (INTEGER, INTEGER) SET (HASHES, MERGES)",
        "ALTER OPERATOR CLASS int_ops USING btree OWNER TO joe",
        "ALTER OPERATOR CLASS int_ops USING brin RENAME TO integer_ops",
        "ALTER OPERATOR FAMILY integer_ops USING btree ADD OPERATOR 1 < (INT4, INT2)",
        "ALTER OPERATOR FAMILY integer_ops USING btree ADD FUNCTION 1 btint42cmp(INT4, INT2)",
        "ALTER OPERATOR FAMILY integer_ops USING btree DROP OPERATOR 1 (INT4, INT2)",
        "ALTER OPERATOR FAMILY integer_ops USING btree DROP FUNCTION 1 (INT4, INT2)",
        "ALTER OPERATOR FAMILY my_ops USING gist ADD OPERATOR 1 < (INT4, INT4) FOR ORDER BY sort_schema.\"sort family\", OPERATOR 2 > (INT4, INT4) FOR SEARCH",
        "ALTER OPERATOR FAMILY old_ops USING btree RENAME TO new_ops",
        "DROP OPERATOR CLASS IF EXISTS myschema.int4_ops USING btree CASCADE",
        "DROP OPERATOR CLASS a USING btree",
        "DROP OPERATOR FAMILY IF EXISTS float_ops USING brin RESTRICT",
        "DROP OPERATOR ~ (NONE, BIT)",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE OPERATOR (FUNCTION = f)");
    rejected("CREATE OPERATOR + FUNCTION = f");
    rejected("DROP OPERATOR CLASS a");
    rejected("CREATE OPERATOR FAMILY a");
    rejected("ALTER OPERATOR + (INTEGER) OWNER TO x");
}

#[test]
fn aggregates_round_trip() {
    for sql in [
        "CREATE AGGREGATE my_count (*) (SFUNC = int8inc, STYPE = INT8, INITCOND = '0')",
        "CREATE AGGREGATE my_avg (BASETYPE = INT, SFUNC = my_sfunc, STYPE = INT)",
        "CREATE AGGREGATE my_agg (input INT, VARIADIC tail TEXT) (SFUNC = my_sfunc, STYPE = INT)",
        "CREATE AGGREGATE my_mode (ORDER BY anyelement) (SFUNC = ordered_set_transition, STYPE = internal, FINALFUNC = mode_final, FINALFUNC_EXTRA)",
        "CREATE AGGREGATE percentile (FLOAT8 ORDER BY FLOAT8) (SFUNC = ordered_set_transition, STYPE = internal)",
        "CREATE AGGREGATE my_rank (VARIADIC \"any\" ORDER BY VARIADIC \"any\") (SFUNC = t, STYPE = internal, FINALFUNC = rank_final, FINALFUNC_EXTRA, HYPOTHETICAL)",
        "CREATE AGGREGATE my_min (INT) (SFUNC = my_sfunc, STYPE = INT, SSPACE = 128, SORTOP = <)",
        "CREATE AGGREGATE my_min2 (INT) (SFUNC = my_sfunc, STYPE = INT, SORTOP = OPERATOR(pg_catalog.<))",
        "CREATE OR REPLACE AGGREGATE sum2 (INT4, INT4) (SFUNC = int4pl, STYPE = INT4, PARALLEL = SAFE)",
        "CREATE AGGREGATE my_numeric (NUMERIC(10,2)) (SFUNC = my_sfunc, STYPE = NUMERIC(10,2))",
        "ALTER AGGREGATE alt_agg1(INT) RENAME TO alt_agg2",
        "ALTER AGGREGATE alt_func1(INT) OWNER TO someone",
        "ALTER AGGREGATE alt_func1(INT) SET SCHEMA alt_nsp2",
        "ALTER AGGREGATE alt_order(INT ORDER BY TEXT) RENAME TO alt_order2",
        "ALTER AGGREGATE alt_order_only(ORDER BY INT) SET SCHEMA alt_nsp2",
        "ALTER AGGREGATE alt_star(*) OWNER TO other",
        "ALTER AGGREGATE my_agg(\"A\" INT ORDER BY \"B\" INT) OWNER TO some_role",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE AGGREGATE a (SFUNC = f");
    rejected("ALTER AGGREGATE a(INT)");
}

#[test]
fn functions_round_trip() {
    for sql in [
        "CREATE FUNCTION add(INTEGER, INTEGER = 1) RETURNS INTEGER AS 'select $1 + $2;'",
        "CREATE FUNCTION add(INTEGER, INTEGER) RETURNS INTEGER LANGUAGE SQL IMMUTABLE STRICT PARALLEL SAFE AS 'select $1 + $2;'",
        "CREATE FUNCTION f(IN \"A\" INT = 1, OUT \"B\" TEXT, INOUT \"C\" BOOLEAN, VARIADIC \"D\" INT[]) RETURNS INT LANGUAGE SQL AS 'x'",
        "CREATE FUNCTION get_all() RETURNS SETOF my_schema.\"MyType\" LANGUAGE sql AS 'SELECT * FROM t'",
        "CREATE FUNCTION get_ids() RETURNS SETOF INTEGER LANGUAGE sql AS 'SELECT id FROM users'",
        "CREATE FUNCTION test_fn() RETURNS void LANGUAGE sql SECURITY DEFINER AS $$ SELECT 1 $$",
        "CREATE FUNCTION test_fn() RETURNS void LANGUAGE sql SET search_path = auth, pg_temp, public AS $$ SELECT 1 $$",
        "CREATE FUNCTION test_fn() RETURNS void LANGUAGE sql SET search_path FROM CURRENT AS $$ SELECT 1 $$",
        "CREATE FUNCTION f(INT) RETURNS BOOLEAN LANGUAGE sql IMMUTABLE NOT LEAKPROOF RETURN true",
        "CREATE FUNCTION f(INT) RETURNS INT LANGUAGE c WINDOW IMMUTABLE AS 'obj', 'f'",
        "CREATE FUNCTION f(INT) RETURNS INT COST 1 SUPPORT f_support AS 'obj', 'f'",
        "CREATE FUNCTION f() RETURNS INT LANGUAGE sql BEGIN ATOMIC SELECT 1; END",
        "CREATE OR REPLACE FUNCTION return_table(i INTEGER) RETURNS TABLE(id UUID, is_active BOOLEAN) LANGUAGE plpgsql AS $$ BEGIN RETURN QUERY SELECT NULL::UUID, NULL::BOOLEAN; END; $$",
        "CREATE OR REPLACE FUNCTION add(a INTEGER, IN b INTEGER = 1) RETURNS INTEGER LANGUAGE SQL STABLE CALLED ON NULL INPUT PARALLEL UNSAFE RETURN a + b",
        "CREATE OR REPLACE FUNCTION public.f(a INTEGER) RETURNS INTEGER LANGUAGE sql BEGIN ATOMIC INSERT INTO t VALUES (a); RETURN a; END",
        "ALTER FUNCTION alt_func2 IMMUTABLE",
        "ALTER FUNCTION alt_func2(INT) IMMUTABLE STRICT PARALLEL SAFE RESTRICT",
        "ALTER FUNCTION alt_func2(INT) COST 3.5",
        "ALTER FUNCTION alt_func2(INT) DEPENDS ON EXTENSION ext1",
        "ALTER FUNCTION alt_func2(INT) NO DEPENDS ON EXTENSION ext1",
        "ALTER FUNCTION alt_func2(INT) RENAME TO other",
        "ALTER FUNCTION alt_func2(INT) OWNER TO other",
        "ALTER FUNCTION alt_func2(INT) SET SCHEMA other",
        "ALTER FUNCTION alt_func2(INT) RESET ALL",
        "ALTER FUNCTION alt_func2(INT) RESET work_mem",
        "ALTER FUNCTION alt_func2(INT) SET work_mem FROM CURRENT",
        "ALTER FUNCTION alt_func2(INT) SET work_mem = DEFAULT",
        "ALTER FUNCTION alt_func2(INT) SET search_path = pg_catalog, public",
        "ALTER FUNCTION alt_func2(INT) EXTERNAL SECURITY INVOKER",
        "ALTER FUNCTION alt_func2(INT) SECURITY DEFINER",
        "ALTER FUNCTION alt_func2(INT) ROWS 42",
        "ALTER FUNCTION alt_func2(INT) SUPPORT pg_catalog.alt_support",
        "ALTER FUNCTION alt_variadic(VARIADIC INT[]) STABLE",
        "ALTER FUNCTION f(\"Role\" TEXT) RENAME TO g",
    ] {
        roundtrip(sql);
    }
    let statement = pg_statement(roundtrip(
        "CREATE FUNCTION f(INT) RETURNS INT LANGUAGE c AS 'obj', 'f'",
    ));
    let PostgresStatement::CreateFunction(function) = statement else {
        panic!("expected CREATE FUNCTION");
    };
    assert_eq!(function.name.to_string(), "f");
    assert_eq!(function.language.unwrap().text, "c");
    assert!(
        matches!(
            function.body,
            Some(FunctionBody::As {
                link_symbol: Some(_),
                ..
            })
        ),
        "{:?}",
        function.body
    );
    // Reordered clauses render in one canonical order.
    assert_eq!(
        parse_sql::<Postgres>(
            "CREATE FUNCTION f() RETURNS INT STABLE PARALLEL SAFE LANGUAGE sql RETURN 1"
        )
        .unwrap()[0]
            .to_string(),
        "CREATE FUNCTION f() RETURNS INT LANGUAGE sql STABLE PARALLEL SAFE RETURN 1"
    );
    rejected("CREATE FUNCTION f() RETURNS INT LANGUAGE sql LANGUAGE c RETURN 1");
    rejected("CREATE FUNCTION f() RETURNS INT STABLE IMMUTABLE RETURN 1");
    rejected("CREATE FUNCTION f(INT");
    rejected("ALTER FUNCTION f(INT) SET");
}

#[test]
fn roles_round_trip() {
    for sql in [
        "CREATE ROLE staff",
        "CREATE ROLE abc LOGIN PASSWORD NULL",
        "CREATE ROLE abc ROLE baz USER foo, bar",
        "CREATE ROLE a CONNECTION LIMIT -1",
        "CREATE ROLE magician SUPERUSER NOCREATEDB CREATEROLE INHERIT LOGIN REPLICATION BYPASSRLS PASSWORD 'abcdef' VALID UNTIL '2025-01-01' IN ROLE role1, role2 ROLE role3 ADMIN role4, role5",
        "CREATE GROUP staff",
        "CREATE GROUP staff SUPERUSER LOGIN CONNECTION LIMIT 5 USER karl, john",
        "CREATE USER mapping",
        "ALTER ROLE a WITH CONNECTION LIMIT 5",
        "ALTER ROLE old_name RENAME TO new_name",
        "ALTER ROLE role_name RESET ALL",
        "ALTER ROLE role_name IN DATABASE database_name RESET maintenance_work_mem",
        "ALTER ROLE role_name IN DATABASE database_name SET maintenance_work_mem TO 100000",
        "ALTER ROLE role_name IN DATABASE database_name SET maintenance_work_mem TO DEFAULT",
        "ALTER ROLE role_name SET maintenance_work_mem FROM CURRENT",
        "ALTER ROLE bob SET search_path TO public",
        "ALTER ROLE bob WITH SUPERUSER PASSWORD 'x' CONNECTION LIMIT 5",
        "ALTER ROLE role_name WITH NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOLOGIN NOREPLICATION NOBYPASSRLS PASSWORD NULL",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE ROLE");
    rejected("ALTER ROLE a IN DATABASE");
}

#[test]
fn types_extensions_and_collations_round_trip() {
    for sql in [
        "CREATE EXTENSION extension_name",
        "CREATE EXTENSION IF NOT EXISTS extension_name SCHEMA schema_name VERSION version CASCADE",
        "CREATE EXTENSION extension_name WITH SCHEMA schema_name VERSION version CASCADE",
        "CREATE EXTENSION extension_name WITH CASCADE",
        "CREATE EXTENSION \"uuid-ossp\" VERSION '1.1'",
        "CREATE EXTENSION pgcrypto WITH SCHEMA exts VERSION '1.3'",
        "CREATE DOMAIN my_domain AS INTEGER CHECK (VALUE > 0)",
        "CREATE DOMAIN my_domain AS INTEGER COLLATE \"en_US\" DEFAULT 1 CHECK (VALUE > 0)",
        "CREATE DOMAIN my_domain AS INTEGER CONSTRAINT my_constraint CHECK (VALUE > 0)",
        "CREATE TYPE public.my_type AS ENUM ('label1', 'label2', 'label3', 'label4')",
        "ALTER TYPE public.my_type ADD VALUE 'label3.5' AFTER 'label3'",
        "ALTER TYPE public.my_type ADD VALUE IF NOT EXISTS 'label3.5' BEFORE 'label4'",
        "ALTER TYPE public.my_type ADD VALUE IF NOT EXISTS 'label5'",
        "ALTER TYPE public.my_type RENAME TO my_new_type",
        "CREATE COLLATION test3 (provider = icu, lc_collate = 'en_US.utf8')",
        "CREATE COLLATION test4 FROM nonsense",
        "ALTER COLLATION \"en-x-icu\" REFRESH VERSION",
        "ALTER COLLATION test1 RENAME TO test11",
        "ALTER COLLATION test11 OWNER TO regress_test_role",
        "ALTER COLLATION test11 SET SCHEMA test_schema",
        "CREATE TEXT SEARCH CONFIGURATION c (copy = english)",
        "CREATE TEXT SEARCH DICTIONARY d (template)",
        "CREATE TEXT SEARCH PARSER p (start = prsd_start, gettoken = prsd_nexttoken, end = prsd_end, lextypes = prsd_lextype)",
        "CREATE TEXT SEARCH TEMPLATE t (\"Init\" = init_function)",
        "ALTER TEXT SEARCH CONFIGURATION c OWNER TO some_user",
        "ALTER TEXT SEARCH DICTIONARY d (opt = val)",
        "ALTER TEXT SEARCH DICTIONARY d (opt)",
        "ALTER TEXT SEARCH PARSER p RENAME TO p2",
        "ALTER TEXT SEARCH TEMPLATE t SET SCHEMA s",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE EXTENSION");
    rejected("CREATE DOMAIN d");
    rejected("CREATE TYPE t AS ENUM");
    rejected("ALTER COLLATION test1 FROZEN");
    rejected("CREATE COLLATION test3");
}

#[test]
fn foreign_data_round_trip() {
    for sql in [
        "CREATE FOREIGN DATA WRAPPER myfdw",
        "CREATE FOREIGN DATA WRAPPER myfdw HANDLER myhandler VALIDATOR myvalidator OPTIONS (debug 'true')",
        "CREATE FOREIGN DATA WRAPPER myfdw NO HANDLER NO VALIDATOR",
        "CREATE SERVER myserver FOREIGN DATA WRAPPER postgres_fdw",
        "CREATE SERVER IF NOT EXISTS myserver TYPE 'server_type' VERSION 'server_version' FOREIGN DATA WRAPPER postgres_fdw",
        "CREATE SERVER myserver2 FOREIGN DATA WRAPPER postgres_fdw OPTIONS (host 'foo', dbname 'foodb', port '5432')",
        "CREATE USER MAPPING FOR PUBLIC SERVER s",
        "CREATE USER MAPPING FOR CURRENT_USER SERVER s",
        "CREATE USER MAPPING IF NOT EXISTS FOR USER SERVER s",
        "CREATE USER MAPPING FOR app SERVER s OPTIONS (\"user\" 'remote', password 'secret')",
        "CREATE FOREIGN TABLE ft1 (id INTEGER, name TEXT) SERVER myserver",
        "CREATE FOREIGN TABLE IF NOT EXISTS ft2 (col INTEGER) SERVER remoteserver",
        "CREATE FOREIGN TABLE ft3 (col INTEGER) SERVER remoteserver OPTIONS (schema_name 'public')",
        "CREATE FOREIGN TABLE child (id INT) INHERITS (parent, public.other) SERVER s OPTIONS (schema_name 'public')",
        "CREATE FOREIGN TABLE child () INHERITS (parent) SERVER s",
        "CREATE FOREIGN TABLE ft (CONSTRAINT c CHECK (id > 0)) SERVER s",
        "CREATE FOREIGN TABLE part PARTITION OF root FOR VALUES IN (1, 2) SERVER s",
        "CREATE FOREIGN TABLE part PARTITION OF root (id NOT NULL) DEFAULT SERVER s",
        "ALTER FOREIGN TABLE ft ADD COLUMN note TEXT",
        "ALTER FOREIGN TABLE IF EXISTS ONLY public.ft ADD COLUMN a INT, DROP COLUMN b",
        "ALTER FOREIGN TABLE ft DROP COLUMN IF EXISTS note CASCADE",
        "ALTER FOREIGN TABLE ft ADD CONSTRAINT positive CHECK (id > 0) NOT VALID",
        "ALTER FOREIGN TABLE ft VALIDATE CONSTRAINT positive",
        "ALTER FOREIGN TABLE ft OWNER TO app",
        "ALTER FOREIGN TABLE ft RENAME COLUMN note TO memo",
        "ALTER FOREIGN TABLE ft RENAME TO archive",
        "ALTER FOREIGN TABLE ft ALTER COLUMN id OPTIONS (SET column_name 'remote_id', DROP other)",
        "ALTER FOREIGN TABLE ft OPTIONS (ADD fetch_size '100', SET batch_size '10', DROP updatable, use_remote_estimate 'true')",
        "ALTER FOREIGN TABLE ft INHERIT p",
        "ALTER FOREIGN TABLE ft NO INHERIT p",
        "DROP FOREIGN TABLE IF EXISTS ft, public.other CASCADE",
        "COMMENT ON FOREIGN TABLE public.ft IS 'remote orders'",
        "CREATE TABLE t (id INT OPTIONS (column_name 'remote_id'))",
        "ALTER TABLE t ADD COLUMN note TEXT OPTIONS (column_name 'memo')",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE SERVER s");
    rejected("CREATE FOREIGN TABLE ft (a INT)");
    rejected("CREATE USER MAPPING SERVER s");
}

#[test]
fn table_clauses_round_trip() {
    for sql in [
        "CREATE TABLE t ()",
        "CREATE TABLE IF NOT EXISTS uk_cities ()",
        "CREATE TABLE t (CONSTRAINT positive CHECK (2 > 1))",
        "CREATE TEMPORARY TABLE table (COL INT) ON COMMIT DELETE ROWS",
        "CREATE TEMPORARY TABLE table (COL INT) ON COMMIT PRESERVE ROWS",
        "CREATE TEMPORARY TABLE table (COL INT) ON COMMIT DROP",
        "CREATE LOCAL TEMPORARY TABLE table (COL INT)",
        "CREATE UNLOGGED TABLE public.unlogged2 (a INT PRIMARY KEY)",
        "CREATE UNLOGGED TABLE unlogged1 (a INT) PARTITION BY RANGE(a)",
        "CREATE TABLE t1 (a INT, b TEXT) PARTITION BY RANGE(a)",
        "CREATE TABLE child_table (child_column INT) INHERITS (public.parent_table, pg_catalog.pg_settings)",
        "CREATE TABLE new (LIKE old INCLUDING DEFAULTS)",
        "CREATE TABLE new (LIKE old EXCLUDING DEFAULTS)",
        "CREATE TABLE t (c INT) WITH (foo = 'bar', a = 123)",
        "CREATE TABLE t (a INT) WITH (toast.autovacuum_enabled = false)",
        "CREATE TABLE t (a INT) WITH (security_barrier)",
        "CREATE TABLE t (c public.currency ARRAY)",
        "CREATE TABLE t (a INT, NOT NULL a)",
        "CREATE TABLE t (a INT, CONSTRAINT nn NOT NULL a NO INHERIT)",
        "CREATE TABLE t (u INT UNIQUE NULLS NOT DISTINCT)",
        "CREATE TABLE t (u INT CONSTRAINT k UNIQUE NULLS NOT DISTINCT DEFERRABLE NOT NULL)",
        "CREATE TABLE t (col INT, EXCLUDE (col WITH =))",
        "CREATE TABLE t (col INT, EXCLUDE USING gist (col WITH =) WHERE (col > 0))",
        "CREATE TABLE t (col INT, EXCLUDE USING gist (col WITH OPERATOR(pg_catalog.=)))",
        "CREATE TABLE t (col INT, EXCLUDE USING btree (col DESC NULLS FIRST WITH =))",
        "CREATE TABLE t (col TEXT, EXCLUDE USING gist (col text_pattern_ops WITH =))",
        "CREATE TABLE t (col INT, EXCLUDE USING gist (col WITH =) DEFERRABLE INITIALLY DEFERRED)",
        "CREATE TABLE t (name TEXT, EXCLUDE USING gist ((lower(name)) text_pattern_ops WITH =))",
        "CREATE TABLE t (name TEXT, EXCLUDE USING btree (name COLLATE \"C\" WITH =))",
        "CREATE TABLE t (CONSTRAINT no_overlap EXCLUDE USING gist (room WITH =, during WITH &&) INCLUDE (id) WHERE (active = true))",
        "CREATE TABLE orders_default PARTITION OF orders DEFAULT",
        "CREATE TABLE orders_us PARTITION OF orders FOR VALUES IN ('US', 'CA', 'MX')",
        "CREATE TABLE orders_old PARTITION OF orders FOR VALUES FROM (MINVALUE) TO ('2020-01-01')",
        "CREATE TABLE orders_new PARTITION OF orders FOR VALUES FROM ('2024-01-01') TO (MAXVALUE)",
        "CREATE TABLE sales_2023_q1 PARTITION OF sales FOR VALUES FROM ('2023-01-01', 1) TO ('2023-04-01', 1)",
        "CREATE TABLE orders_p0 PARTITION OF orders FOR VALUES WITH (MODULUS 4, REMAINDER 0)",
        "CREATE TABLE orders_2023 PARTITION OF orders (CONSTRAINT check_date CHECK (order_date >= '2023-01-01')) FOR VALUES FROM ('2023-01-01') TO ('2024-01-01')",
        "CREATE TABLE part PARTITION OF root (id CONSTRAINT id_present NOT NULL, CONSTRAINT positive CHECK (id > 0)) FOR VALUES IN (1)",
        "CREATE TABLE part PARTITION OF root (id NOT NULL, note DEFAULT 'x', flag) DEFAULT",
        "CREATE TABLE part PARTITION OF root (id NOT NULL, flag) DEFAULT",
        "ALTER TABLE c INHERIT public.p",
        "ALTER TABLE c NO INHERIT public.p",
        "ALTER TABLE IF EXISTS ONLY c INHERIT public.p, ADD COLUMN x INT",
        "ALTER TABLE root ATTACH PARTITION part DEFAULT",
        "ALTER TABLE root ATTACH PARTITION part FOR VALUES IN (1, 2)",
        "ALTER TABLE ONLY public.root ATTACH PARTITION public.part FOR VALUES FROM (0) TO (10)",
        "ALTER TABLE ONLY public.hroot ATTACH PARTITION public.part4 FOR VALUES WITH (MODULUS 4, REMAINDER 0)",
        "ALTER TABLE IF EXISTS ONLY public.root DETACH PARTITION public.part",
        "ALTER TABLE IF EXISTS ONLY public.root DETACH PARTITION public.part CONCURRENTLY",
        "ALTER TABLE IF EXISTS ONLY public.root DETACH PARTITION public.part FINALIZE",
        "ALTER TABLE t ADD CONSTRAINT nn NOT NULL a NO INHERIT NOT VALID",
        "ALTER TABLE t ADD NOT NULL a NOT VALID",
        "ALTER TABLE t ADD CONSTRAINT no_overlap EXCLUDE USING gist (room WITH =)",
        "ALTER MATERIALIZED VIEW m RENAME TO m2",
        "ALTER MATERIALIZED VIEW IF EXISTS s.m OWNER TO alice",
        "ALTER MATERIALIZED VIEW m ALTER COLUMN c SET DEFAULT 1",
        "ALTER MATERIALIZED VIEW m RESET (fillfactor, autovacuum_enabled)",
        "ALTER MATERIALIZED VIEW m SET (fillfactor = 70)",
        "ALTER MATERIALIZED VIEW m SET SCHEMA s",
        "ALTER SCHEMA foo OWNER TO bar",
        "ALTER SCHEMA foo RENAME TO bar",
        "CREATE MATERIALIZED VIEW m WITH (autovacuum_enabled) AS SELECT 1",
        "CREATE VIEW v WITH (security_invoker = true, security_barrier) AS SELECT 1",
    ] {
        roundtrip(sql);
    }
    rejected("CREATE TABLE t PARTITION OF");
    rejected("CREATE TABLE p PARTITION OF t FOR VALUES");
    rejected("ALTER TABLE t ATTACH PARTITION");
    rejected("CREATE TABLE t (a INT) ON COMMIT");
    rejected("CREATE UNLOGGED VIEW v AS SELECT 1");
    rejected("CREATE TABLE t (LIKE)");
}

#[test]
fn utility_statements_round_trip() {
    for sql in [
        "ANALYZE t (col1, col2)",
        "ANALYZE",
        "COPY users FROM 'data.csv'",
        "COPY users TO 'data.csv' DELIMITER ','",
        "COPY (SELECT 42 AS a, 'hello' AS b) TO 'query.csv'",
        "COPY country TO STDOUT (DELIMITER '|')",
        "COPY country TO PROGRAM 'gzip > x.gz'",
        "COPY t FROM STDIN CSV HEADER",
        "COPY copytest4 FROM STDIN (HEADER)",
        "COPY bitwise_test FROM STDIN NULL 'null'",
        "COPY t (a, b) FROM 'file.csv' (FORMAT CSV, FREEZE, DELIMITER ',', NULL '', HEADER, QUOTE '\"', ESCAPE '\\', FORCE_QUOTE (a), FORCE_NOT_NULL (b), FORCE_NULL (c), ENCODING 'utf8')",
        "COPY users FROM 'data.csv' BINARY DELIMITER ',' NULL 'null' CSV HEADER QUOTE '\"' ESCAPE '\\' FORCE NOT NULL column",
        "COPY users TO 'data.csv' BINARY DELIMITER ',' NULL 'null' CSV HEADER QUOTE '\"' ESCAPE '\\' FORCE QUOTE column",
        "DEALLOCATE a",
        "DEALLOCATE PREPARE ALL",
        "DECLARE \"SQL_CUR\" BINARY INSENSITIVE SCROLL CURSOR WITH HOLD FOR SELECT * FROM table_name LIMIT 2222",
        "DECLARE c CURSOR WITHOUT HOLD FOR SELECT 1",
        "DECLARE c NO SCROLL CURSOR FOR SELECT 1",
        "DECLARE c ASENSITIVE CURSOR FOR SELECT 1",
        "EXECUTE a USING CAST(1337 AS SMALLINT), CAST(7331 AS SMALLINT)",
        "EXECUTE a(1, 't')",
        "EXECUTE a",
        "FETCH 2048 IN c",
        "FETCH 2048 IN c INTO \"new_table\"",
        "FETCH ALL IN c INTO \"new_table\"",
        "FETCH ABSOLUTE 2048 IN c",
        "FETCH BACKWARD ALL IN c INTO t",
        "FETCH FORWARD 2048 IN c",
        "FETCH FIRST IN c",
        "FETCH LAST IN c",
        "FETCH NEXT IN c",
        "FETCH PRIOR IN c",
        "FETCH RELATIVE 2048 IN c",
        "LOCK TABLE public.widgets IN ACCESS EXCLUSIVE MODE",
        "LOCK TABLE ONLY public.widgets, analytics.events * IN SHARE ROW EXCLUSIVE MODE NOWAIT",
        "LOCK TABLE public.widgets IN EXCLUSIVE MODE",
        "PREPARE a (INT, TEXT) AS SELECT * FROM customers WHERE customers.id = a1",
        "PREPARE a AS INSERT INTO customers VALUES (a1, a2, a3)",
        "DO LANGUAGE plpgsql LANGUAGE sql $$BEGIN NULL; END$$",
        "DO $$BEGIN NULL; END$$",
        "TRUNCATE users, orders RESTART IDENTITY",
        "TRUNCATE TABLE ONLY parent, child *, grandchild RESTART IDENTITY",
        "TRUNCATE TABLE \"users\", \"orders\" RESTART IDENTITY RESTRICT",
        "TRUNCATE TABLE db.table_name, db.other_table_name RESTART IDENTITY CASCADE",
        "TRUNCATE TABLE t *",
        "TRUNCATE t CONTINUE IDENTITY",
        "ALTER DEFAULT PRIVILEGES GRANT SELECT ON TABLES TO app",
        "ALTER DEFAULT PRIVILEGES FOR ROLE r IN SCHEMA s GRANT SELECT ON TABLES TO g WITH GRANT OPTION",
        "ALTER DEFAULT PRIVILEGES FOR ROLE bob GRANT SELECT ON TABLES TO app",
        "ALTER DEFAULT PRIVILEGES IN SCHEMA s FOR ROLE r GRANT SELECT ON TABLES TO app",
        "ALTER DEFAULT PRIVILEGES GRANT EXECUTE ON FUNCTIONS TO app",
        "ALTER DEFAULT PRIVILEGES GRANT USAGE, CREATE ON SCHEMAS TO app",
        "ALTER DEFAULT PRIVILEGES GRANT SELECT, UPDATE ON LARGE OBJECTS TO app",
        "ALTER DEFAULT PRIVILEGES REVOKE GRANT OPTION FOR ALL ON TABLES FROM app CASCADE",
        "ALTER DEFAULT PRIVILEGES REVOKE SELECT ON TABLES FROM g",
    ] {
        roundtrip(sql);
    }
    let statement = pg_statement(roundtrip("COPY country TO PROGRAM 'gzip > x.gz'"));
    let PostgresStatement::Copy(copy) = statement else {
        panic!("expected COPY");
    };
    assert!(copy.to);
    assert!(matches!(copy.target, CopyTarget::Program(_)));
    rejected("COPY users");
    rejected("COPY (SELECT 1) FROM 'x'");
    rejected("DECLARE c FOR SELECT 1");
    rejected("LOCK TABLE");
    rejected("TRUNCATE t RESTART");
    rejected("FETCH IN");
    rejected("DEALLOCATE");
}

#[test]
fn triggers_and_rules_round_trip() {
    for sql in [
        "CREATE TRIGGER check_insert BEFORE INSERT ON accounts FOR EACH ROW EXECUTE FUNCTION check_account_insert",
        "CREATE TRIGGER check_delete INSTEAD OF DELETE ON accounts FOR EACH ROW EXECUTE FUNCTION check_account_deletes",
        "CREATE TRIGGER check_update AFTER UPDATE ON accounts FOR EACH ROW WHEN (NEW.balance > 10000) EXECUTE FUNCTION check_account_update",
        "CREATE TRIGGER check_referencing BEFORE INSERT ON accounts REFERENCING NEW TABLE AS new_accounts OLD TABLE AS old_accounts FOR EACH ROW EXECUTE FUNCTION check_account_referencing",
        "CREATE CONSTRAINT TRIGGER check_multiple_events BEFORE INSERT OR UPDATE OR DELETE ON accounts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION check_account_changes",
        "CREATE RULE notify_me AS ON UPDATE TO mytable DO ALSO NOTIFY mytable",
        "CREATE RULE r AS ON UPDATE TO t WHERE old.balance <> new.balance DO INSTEAD UPDATE shadow SET balance = new.balance WHERE id = old.id",
    ] {
        roundtrip(sql);
    }
    let statement = pg_statement(roundtrip(
        "CREATE TRIGGER t BEFORE INSERT OR UPDATE OF a, b ON x EXECUTE FUNCTION f()",
    ));
    let PostgresStatement::CreateTrigger(trigger) = statement else {
        panic!("expected CREATE TRIGGER");
    };
    assert_eq!(trigger.events.len(), 2);
    assert!(matches!(trigger.events[0], TriggerEvent::Insert));
    rejected("CREATE TRIGGER t ON x EXECUTE FUNCTION f()");
    rejected("CREATE RULE r AS ON UPDATE TO t");
}

#[test]
fn statement_shapes() {
    let statement = pg_statement(roundtrip(
        "CREATE AGGREGATE a (BASETYPE = INT, SFUNC = f, STYPE = INT)",
    ));
    let PostgresStatement::CreateAggregate(aggregate) = statement else {
        panic!("expected CREATE AGGREGATE");
    };
    assert!(aggregate.args.is_none());
    assert_eq!(aggregate.definition.len(), 3);
    assert_eq!(aggregate.definition[0].name.text, "BASETYPE");
    assert!(matches!(
        aggregate.definition[1].value,
        Some(DefArg::Type(_))
    ));

    let statement = roundtrip("CREATE TABLE p PARTITION OF t FOR VALUES IN (1, 2)");
    let PostgresStatement::CreateTable(table) = pg_statement(statement) else {
        panic!("expected a Postgres CREATE TABLE");
    };
    assert_eq!(table.partition_of.as_ref().unwrap().to_string(), "t");
    assert!(matches!(table.bound, Some(PartitionBound::In(ref v)) if v.len() == 2));

    let PostgresStatement::Insert(insert) = pg_statement(roundtrip(
        "INSERT INTO t AS x (a) VALUES (1) ON CONFLICT DO NOTHING",
    )) else {
        panic!("expected a Postgres INSERT");
    };
    assert_eq!(insert.insert.table.to_string(), "t");
    assert_eq!(insert.alias.unwrap().text, "x");
}

#[test]
fn expression_shapes() {
    let ExprKind::Ext(BuiltinExpr::Postgres(PostgresExpr::Postfix { op, .. })) =
        parse_expr::<Postgres>("a !").unwrap().kind
    else {
        panic!("expected a postfix operator");
    };
    assert_eq!(op, "!");
    let ExprKind::TypedString(typed) = parse_expr::<Postgres>("mood 'happy'").unwrap().kind else {
        panic!("expected a typed string");
    };
    assert_eq!(typed.ty.name.text, "mood");
    assert_eq!(typed.value.text, "happy");
    let ExprKind::Literal(Literal::String(s)) = parse_expr::<Postgres>(r"E'a\n'").unwrap().kind
    else {
        panic!("expected a string");
    };
    assert_eq!(s.prefix, StringPrefix::Escape);
    assert_eq!(s.value(), "a\n");
}

#[test]
fn expressions_round_trip() {
    for (sql, rendered) in [
        ("SELECT FROM t", "SELECT FROM t"),
        ("SELECT a!", "SELECT a!"),
        ("SELECT a! !", "SELECT a! !"),
        ("SELECT (a!)!", "SELECT (a!)!"),
        ("SELECT !!a", "SELECT !!a"),
        ("SELECT @ -2", "SELECT @ -2"),
        ("SELECT @a", "SELECT @ a"),
        ("SELECT - ~1", "SELECT - ~ 1"),
        ("SELECT ~ #x", "SELECT ~ # x"),
        ("SELECT |/ a", "SELECT |/ a"),
        ("SELECT ||/ a", "SELECT ||/ a"),
        (
            "SELECT info #- ARRAY['a', 'b'] FROM orders",
            "SELECT info #- ARRAY['a', 'b'] FROM orders",
        ),
        (
            "SELECT * FROM events WHERE relname OPERATOR(pg_catalog.~) '^(table)$'",
            "SELECT * FROM events WHERE relname OPERATOR(pg_catalog.~) '^(table)$'",
        ),
        ("SELECT E'a''b'", "SELECT E'a''b'"),
        (r"SELECT E'\''", r"SELECT E'\''"),
        ("SELECT B'111'", "SELECT B'111'"),
        ("SELECT U&'\\0441'", "SELECT U&'\\0441'"),
        ("SELECT xml '<foo/>'", "SELECT xml '<foo/>'"),
        ("SELECT mood $$happy$$", "SELECT mood $$happy$$"),
        ("SELECT pg_catalog.mood 'x'", "SELECT pg_catalog.mood 'x'"),
        ("SELECT f(x := 1, y => 2)", "SELECT f(x := 1, y => 2)"),
        ("SELECT XMLFOREST(a AS x, b)", "SELECT XMLFOREST(a AS x, b)"),
        (
            "SELECT XMLPARSE(CONTENT '<a/>')",
            "SELECT XMLPARSE(CONTENT '<a/>')",
        ),
        (
            "SELECT ARRAY(SELECT 1 UNION SELECT 2)",
            "SELECT ARRAY(SELECT 1 UNION SELECT 2)",
        ),
        ("SELECT TRIM(FROM ' x ')", "SELECT TRIM(' x ')"),
        (
            "SELECT 0x1F, 0o17, 0b101, 0xFF_FF",
            "SELECT 0x1F, 0o17, 0b101, 0xFF_FF",
        ),
        ("SELECT 1_000, 1_0.5", "SELECT 1_000, 1_0.5"),
        ("SELECT INTERVAL '1' DAYS", "SELECT INTERVAL '1' AS DAYS"),
        (
            "WITH c AS MATERIALIZED (SELECT 1) SELECT 1",
            "WITH c AS MATERIALIZED (SELECT 1) SELECT 1",
        ),
        (
            "WITH c AS NOT MATERIALIZED (SELECT 1) SELECT 1",
            "WITH c AS NOT MATERIALIZED (SELECT 1) SELECT 1",
        ),
        (
            "WITH c AS (DELETE FROM t RETURNING a) SELECT a FROM c",
            "WITH c AS (DELETE FROM t RETURNING a) SELECT a FROM c",
        ),
        (
            "WITH c AS (INSERT INTO t VALUES (1) RETURNING a) SELECT a FROM c",
            "WITH c AS (INSERT INTO t VALUES (1) RETURNING a) SELECT a FROM c",
        ),
        (
            "WITH c AS (UPDATE t SET a = 1 RETURNING a) SELECT a FROM c",
            "WITH c AS (UPDATE t SET a = 1 RETURNING a) SELECT a FROM c",
        ),
        ("SELECT * FROM ONLY t AS x", "SELECT * FROM ONLY t AS x"),
        ("SELECT * FROM ONLY(t)", "SELECT * FROM ONLY (t)"),
        (
            "SELECT * FROM ONLY (app.t) x",
            "SELECT * FROM ONLY (app.t) x",
        ),
        ("SELECT * FROM t *", "SELECT * FROM t *"),
        ("SELECT * FROM app.t * AS x", "SELECT * FROM app.t * AS x"),
        (
            "SELECT * FROM ONLY a JOIN b * ON a.id = b.id",
            "SELECT * FROM ONLY a JOIN b * ON a.id = b.id",
        ),
        ("UPDATE ONLY t SET id = 1", "UPDATE ONLY t SET id = 1"),
        ("UPDATE t * SET id = 1", "UPDATE t * SET id = 1"),
        ("DELETE FROM ONLY t", "DELETE FROM ONLY t"),
        ("DELETE FROM t*", "DELETE FROM t *"),
        (
            "SELECT * FROM t FOR UPDATE LIMIT 5",
            "SELECT * FROM t LIMIT 5 FOR UPDATE",
        ),
        (
            "SELECT * FROM t0 INNER JOIN t1 INNER JOIN t2 ON true ON true",
            "SELECT * FROM t0 INNER JOIN (t1 INNER JOIN t2 ON true) ON true",
        ),
        (
            "SELECT * FROM t1 JOIN t2 USING (id) AS joined_cols",
            "SELECT * FROM t1 JOIN t2 USING(id) AS joined_cols",
        ),
        (
            "INSERT INTO t OVERRIDING SYSTEM VALUE VALUES (1, 'x')",
            "INSERT INTO t OVERRIDING SYSTEM VALUE VALUES (1, 'x')",
        ),
        (
            "INSERT INTO t (id) OVERRIDING USER VALUE SELECT 1",
            "INSERT INTO t (id) OVERRIDING USER VALUE SELECT 1",
        ),
        (
            "INSERT INTO test_tables AS test_table (id, a) VALUES (DEFAULT, 123)",
            "INSERT INTO test_tables AS test_table (id, a) VALUES (DEFAULT, 123)",
        ),
        (
            "INSERT INTO t (a) VALUES (1) ON CONFLICT DO NOTHING",
            "INSERT INTO t (a) VALUES (1) ON CONFLICT DO NOTHING",
        ),
        (
            "INSERT INTO t (a) VALUES (1) ON CONFLICT(a) WHERE b > 0 DO UPDATE SET b = EXCLUDED.b WHERE t.b < 10",
            "INSERT INTO t (a) VALUES (1) ON CONFLICT(a) WHERE b > 0 DO UPDATE SET b = EXCLUDED.b WHERE t.b < 10",
        ),
        (
            "SELECT JSON_OBJECT('name' VALUE 'value' NULL ON NULL)",
            "SELECT JSON_OBJECT('name' VALUE 'value' NULL ON NULL)",
        ),
    ] {
        let statements = parse_sql::<Postgres>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
        assert_eq!(statements.len(), 1, "{sql}");
        assert_eq!(statements[0].to_string(), rendered, "{sql}");
    }
    assert!(parse_sql::<Postgres>("SELECT 0x").is_err());
    assert!(parse_sql::<Postgres>("SELECT 1 WHERE x > INTERVAL '1' WEEK").is_err());
    assert!(parse_sql::<Postgres>("SELECT CASE 1 WHEN 1 THEN 1 END END FROM t").is_err());
    assert!(parse_sql::<Postgres>("GRANT MAINTAIN (a) ON t TO r").is_err());
    assert!(parse_sql::<Postgres>("SELECT 1__0").is_err());
    assert!(parse_sql::<Postgres>("SELECT * FROM ONLY").is_err());
}

#[test]
fn other_dialects_stay_out() {
    for sql in [
        "CREATE OPERATOR + (FUNCTION = f)",
        "CREATE AGGREGATE a (*) (SFUNC = f)",
        "CREATE EXTENSION x",
        "ALTER ROLE a RENAME TO b",
        "COPY t FROM STDIN",
        "CREATE FUNCTION f() RETURNS INT RETURN 1",
        "SELECT a!",
    ] {
        assert!(parse_sql::<MySql>(sql).is_err(), "MySQL accepted {sql}");
        assert!(parse_sql::<Core>(sql).is_err(), "Core accepted {sql}");
    }
    // In MySQL a string after a name is an alias.
    for sql in ["SELECT xml 'x'", "SELECT E'x'"] {
        assert!(parse_sql::<Core>(sql).is_err(), "Core accepted {sql}");
    }
}
