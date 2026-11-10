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

//! Microsoft SQL Server: T-SQL syntax, and what other dialects keep out.

use sqlparser::dialect::builtin::{DialectKind, parse_sql_as};
use sqlparser::dialect::{Core, MsSql};
use sqlparser::{parse_expr, parse_sql};

fn round_trips(sql: &str) {
    let statements = parse_sql::<MsSql>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(statements.len(), 1, "{sql}");
    assert_eq!(statements[0].to_string(), sql);
}

fn parses_to(sql: &str, expected: &str) {
    let statements = parse_sql::<MsSql>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    let rendered: Vec<String> = statements.iter().map(ToString::to_string).collect();
    assert_eq!(rendered.join("; "), expected, "{sql}");
}

fn rejected(sql: &str) {
    assert!(parse_sql::<MsSql>(sql).is_err(), "accepted: {sql}");
}

fn expr_round_trips(sql: &str) {
    let expr = parse_expr::<MsSql>(sql).unwrap_or_else(|e| panic!("{sql}: {e:?}"));
    assert_eq!(expr.to_string(), sql);
}

#[test]
fn can_be_chosen_at_runtime() {
    for name in ["MsSql", "mssql", "SqlServer", "tsql"] {
        let kind = DialectKind::from_name(name).unwrap();
        assert_eq!(parse_sql_as(kind, "SELECT 1").unwrap().len(), 1);
    }
}

#[test]
fn identifiers() {
    round_trips("SELECT @@version, _foo$123 FROM ##temp");
    round_trips("SELECT #t, a$b, x@y FROM t");
    round_trips("SELECT [a column] FROM [a schema].[a table]");
    round_trips("SELECT [a.b!] AS [FROM] FROM foo [WHERE]");
    parses_to(
        "SELECT [a.b!] [FROM] FROM foo [WHERE]",
        "SELECT [a.b!] AS [FROM] FROM foo [WHERE]",
    );
    round_trips(r#"SELECT "a""b" FROM "foo""#);
    // `=>` is two tokens here, so it is not a named-argument arrow.
    rejected("SELECT f(a => 1)");
}

#[test]
fn escaped_bracket_identifier_keeps_its_escape() {
    round_trips("SELECT [a]]b] FROM t");
}

#[test]
fn at_signs_need_a_name() {
    for sql in ["SELECT @'1'", "SELECT a &@* b", "SELECT a <@@> b"] {
        rejected(sql);
    }
    round_trips("SELECT @@version");
}

#[test]
fn parameters_and_whitespace() {
    round_trips("SELECT :fooBar");
    expr_round_trips(":a + :b");
    rejected("SELECT : foo");
    parses_to("\u{a0}", "");
}

#[test]
fn method_calls() {
    round_trips(
        "SELECT CONVERT(XML, '<Book>abc</Book>').value('.', 'NVARCHAR(MAX)').value('.', 'NVARCHAR(MAX)') AS T",
    );
    round_trips("SELECT CAST(column AS XML).value('.', 'NVARCHAR(MAX)') AS T");
    expr_round_trips(
        "(SELECT ',' + name FROM sys.objects FOR XML PATH(''), TYPE).value('.', 'NVARCHAR(MAX)')",
    );
    round_trips(
        "SELECT STUFF((SELECT ',' + name FROM sys.objects FOR XML PATH(''), TYPE).value('.', 'NVARCHAR(MAX)'), 1, 1, '') AS T",
    );
}

#[test]
fn true_and_false_are_columns() {
    round_trips("SELECT true, false FROM t");
    expr_round_trips("true AND false");
}

#[test]
fn money_and_national_strings() {
    round_trips("SELECT $123.45");
    round_trips("SELECT $0.99");
    round_trips("SELECT CEILING($123.45)");
    round_trips("SELECT $1");
    parses_to("SELECT n'x'", "SELECT N'x'");
    round_trips("PRINT N'Hello, ⛄️!'");
    round_trips(
        "IF DATENAME(weekday, GETDATE()) IN (N'Saturday', N'Sunday') SELECT 'Weekend'; ELSE SELECT 'Weekday';",
    );
}

#[test]
fn nested_comments() {
    parses_to("/* a /* nested */ comment */ SELECT 1", "SELECT 1");
}

#[test]
fn aliases() {
    parses_to("SELECT foo 'alias'", "SELECT foo AS 'alias'");
    round_trips("SELECT foo AS 'alias'");
    parses_to("SELECT a = 1 FROM t", "SELECT 1 AS a FROM t");
    parses_to("SELECT a = [from] FROM t", "SELECT [from] AS a FROM t");
    parses_to(
        "SELECT some_alias = (a*b) FROM t",
        "SELECT (a * b) AS some_alias FROM t",
    );
    parses_to(
        "SELECT @a = 1 + 2 FROM t WHERE x = 1",
        "SELECT 1 + 2 AS @a FROM t WHERE x = 1",
    );
    rejected("SELECT a = FROM t");
    for sql in [
        "SELECT col IF FROM tbl",
        "SELECT col AS IF FROM tbl",
        "SELECT col ELSE FROM tbl",
        "SELECT col AS ELSE FROM tbl",
        "SELECT * FROM tbl IF",
        "SELECT * FROM tbl AS IF",
        "SELECT * FROM tbl ELSE",
        "SELECT * FROM tbl AS ELSE",
    ] {
        rejected(sql);
    }
}

#[test]
fn table_names() {
    round_trips("SELECT * FROM mydatabase..MyTable");
    rejected("SELECT * FROM a.b..c");
    rejected("SELECT * FROM X..Y..Z");
    round_trips("SELECT * FROM limit");
    round_trips("SELECT * FROM t JOIN limit ON true");
    round_trips("SELECT proc FROM jobs");
}

#[test]
fn table_hints_time_travel_and_apply() {
    round_trips("SELECT * FROM t WITH (NOLOCK)");
    round_trips("SELECT * FROM t AS x WITH (NOLOCK, INDEX(i))");
    round_trips("SELECT 1 FROM t1 FOR SYSTEM_TIME AS OF '2024-01-01'");
    round_trips("SELECT * FROM t FOR SYSTEM_TIME AS OF '2024-01-01' AS x");
    rejected("SELECT 1 FROM t1 FOR SYSTEM TIME AS OF 'x'");
    round_trips(
        "SELECT * FROM sys.dm_exec_query_stats AS deqs CROSS APPLY sys.dm_exec_query_plan(deqs.plan_handle)",
    );
    round_trips(
        "SELECT * FROM sys.dm_exec_query_stats AS deqs OUTER APPLY sys.dm_exec_query_plan(deqs.plan_handle)",
    );
    round_trips("SELECT * FROM foo OUTER APPLY (SELECT foo.x + 1) AS bar");
}

#[test]
fn openjson() {
    round_trips(
        "SELECT B.kind, B.id_list FROM t_test_table AS A CROSS APPLY OPENJSON(A.param, '$.config') WITH (kind VARCHAR(20) '$.kind', [id_list] NVARCHAR(MAX) '$.id_list' AS JSON) AS B",
    );
    round_trips(
        "SELECT B.kind, B.id_list FROM t_test_table AS A CROSS APPLY OPENJSON(A.param) WITH (kind VARCHAR(20) '$.kind', [id_list] NVARCHAR(MAX) '$.id_list' AS JSON) AS B",
    );
    round_trips(
        "SELECT B.kind, B.id_list FROM t_test_table AS A CROSS APPLY OPENJSON(A.param) WITH (kind VARCHAR(20), [id_list] NVARCHAR(MAX)) AS B",
    );
    round_trips(
        "SELECT B.kind, B.id_list FROM t_test_table AS A CROSS APPLY OPENJSON(A.param, '$.config') AS B",
    );
    round_trips(
        "SELECT B.kind, B.id_list FROM t_test_table AS A CROSS APPLY OPENJSON(A.param) AS B",
    );
}

#[test]
fn top() {
    round_trips("SELECT TOP (5) * FROM foo");
    round_trips("SELECT TOP (5) PERCENT * FROM foo");
    round_trips("SELECT TOP (5) WITH TIES * FROM foo");
    round_trips("SELECT TOP (10) PERCENT WITH TIES * FROM foo");
    round_trips("SELECT TOP 5 bar, baz FROM foo");
    round_trips("SELECT DISTINCT TOP 3 a FROM foo");
    round_trips("SELECT TOP (@n + 1) a FROM foo");
    rejected("SELECT TOP @n a FROM foo");
}

#[test]
fn for_clauses() {
    round_trips("SELECT a FROM t FOR JSON PATH");
    round_trips("SELECT b FROM t FOR JSON AUTO");
    round_trips("SELECT c FROM t FOR JSON AUTO, WITHOUT_ARRAY_WRAPPER");
    round_trips("SELECT 1 FROM t FOR JSON PATH, ROOT('x'), INCLUDE_NULL_VALUES");
    round_trips("SELECT * FROM t FOR JSON PATH, ROOT('root')");
    round_trips("SELECT 2 FROM t FOR XML AUTO");
    round_trips("SELECT 3 FROM t FOR XML AUTO, TYPE, ELEMENTS");
    round_trips("SELECT * FROM t FOR XML RAW('x')");
    round_trips("SELECT y FROM t FOR XML PATH('x'), ROOT('y'), ELEMENTS");
    round_trips("SELECT z FROM t FOR XML EXPLICIT, BINARY BASE64");
    round_trips("SELECT x FROM t ORDER BY y FOR XML AUTO, ELEMENTS");
    round_trips("SELECT * FROM t WHERE x FOR XML AUTO, ELEMENTS");
    round_trips("SELECT * FROM t FOR BROWSE");
    parses_to(
        "SELECT a FROM t FOR JSON PATH, ROOT('x')",
        "SELECT a FROM t FOR JSON PATH, ROOT('x')",
    );
    rejected("SELECT a FROM t FOR CSV");
}

#[test]
fn convert() {
    expr_round_trips("CONVERT(DECIMAL(10,5), 12.55)");
    expr_round_trips("TRY_CONVERT(INT, 1)");
    expr_round_trips("CONVERT(VARCHAR(10), x, 120)");
    expr_round_trips("CAST('foo' AS VARCHAR(MAX))");
    expr_round_trips("CAST('foo' AS NVARCHAR(MAX))");
}

#[test]
fn json_functions() {
    round_trips("SELECT JSON_ARRAY(NULL ON NULL)");
    round_trips("SELECT JSON_ARRAY(ABSENT ON NULL)");
    round_trips("SELECT JSON_ARRAY('a', 1, NULL, 2 NULL ON NULL)");
    round_trips("SELECT JSON_ARRAY('a', 1, NULL, 2 ABSENT ON NULL)");
    round_trips("SELECT JSON_ARRAY('a', JSON_OBJECT('name' : 'value', 'type' : 1) NULL ON NULL)");
    round_trips(
        "SELECT JSON_ARRAY('a', JSON_OBJECT('name' : 'value', 'type' : 1), JSON_ARRAY(1, NULL, 2 NULL ON NULL))",
    );
    round_trips("SELECT JSON_ARRAY(1, @id_value, (SELECT @@SPID))");
    round_trips(
        "SELECT JSON_OBJECT('user_name' : USER_NAME(), LOWER(@id_key) : @id_value, 'sid' : (SELECT @@SPID) ABSENT ON NULL)",
    );
    round_trips(
        "SELECT s.session_id, JSON_OBJECT('security_id' : s.security_id, 'login' : s.login_name, 'status' : s.status) AS info FROM sys.dm_exec_sessions AS s WHERE s.is_user_process = 1",
    );
    round_trips(
        "SELECT s.session_id, JSON_ARRAY(s.host_name, s.program_name, s.client_interface_name NULL ON NULL) AS info FROM sys.dm_exec_sessions AS s WHERE s.is_user_process = 1",
    );
}

#[test]
fn control_of_flow() {
    round_trips("BEGIN SELECT 1; END");
    round_trips("BEGIN SELECT 1; SELECT 2; END");
    round_trips("BEGIN INSERT INTO t VALUES (1); UPDATE t SET x = 2; END");
    round_trips("IF 1 = 1 SELECT '1'; ELSE SELECT '2';");
    parses_to(
        "IF 1 = 1 SELECT '1' ELSE SELECT '2'",
        "IF 1 = 1 SELECT '1'; ELSE SELECT '2';",
    );
    round_trips("IF 1 = 1 BEGIN SET @A = 1; END ELSE SET @A = 2;");
    parses_to(
        "IF 1 = 1 BEGIN SET @A = 1; END ELSE BEGIN SET @A = 2 END",
        "IF 1 = 1 BEGIN SET @A = 1; END ELSE BEGIN SET @A = 2; END",
    );
    round_trips(
        "IF (SELECT COUNT(*) FROM a.b WHERE c LIKE 'x%') > 1 SELECT 'yes'; ELSE SELECT 'No';",
    );
    round_trips("WHILE 1 = 0 PRINT 'Hello World';");
    round_trips("WHILE @@FETCH_STATUS = 0 BEGIN FETCH NEXT FROM Employee_Cursor; END");
    round_trips(
        "WHILE @@FETCH_STATUS = 0 BEGIN FETCH NEXT FROM Employee_Cursor; PRINT 'Hello World'; END",
    );
    rejected("WHILE 1 = 0 PRINT 'x'");
    parses_to(
        "DECLARE @A INT; IF 1=1 BEGIN SET @A = 1 END ELSE SET @A = 2",
        "DECLARE @A INT; IF 1 = 1 BEGIN SET @A = 1; END ELSE SET @A = 2;",
    );
    parses_to(
        "BEGIN TRY; SELECT 1/0; END TRY; BEGIN CATCH; EXECUTE foo; END CATCH;",
        "BEGIN TRY; SELECT 1 / 0; END TRY; BEGIN CATCH; EXECUTE foo; END CATCH",
    );
    round_trips("BEGIN TRANSACTION");
    round_trips("COMMIT");
}

#[test]
fn procedural_statements() {
    round_trips("PRINT 'Hello, world!'");
    round_trips("PRINT @my_variable");
    round_trips("RAISERROR('This is a test', 16, 1)");
    round_trips("RAISERROR('This is a test', 16, 1) WITH NOWAIT");
    round_trips("RAISERROR('This is a test', 16, 1, 'ARG') WITH SETERROR, LOG");
    round_trips("RAISERROR(@ErrorMessage, @ErrorSeverity, @ErrorState)");
    round_trips("RAISERROR(N'This is message %s %d.', 10, 1, N'number', 5)");
    round_trips("RAISERROR(N'<<%*.*s>>', 10, 1, 7, 3, N'abcde')");
    round_trips("THROW");
    round_trips("THROW 51000, 'Record does not exist.', 1");
    round_trips("THROW @ErrorNumber, @ErrorMessage, @ErrorState");
    rejected("THROW 51000, 'x'");
    round_trips("WAITFOR DELAY '00:00:05'");
    round_trips("WAITFOR TIME '14:30:00'");
    round_trips("WAITFOR DELAY @WaitTime");
    rejected("WAITFOR '00:00:05'");
    round_trips("RETURN");
    round_trips("RETURN 1");
    round_trips("OPEN Employee_Cursor");
    round_trips("CLOSE Employee_Cursor");
    round_trips("DEALLOCATE Employee_Cursor");
    round_trips("FETCH NEXT FROM Employee_Cursor");
    round_trips("FETCH ABSOLUTE 5 FROM c INTO @x");
    round_trips("FETCH FORWARD ALL FROM c");
}

#[test]
fn execute() {
    parses_to("EXEC (@sql)", "EXECUTE (@sql)");
    parses_to(
        "EXEC ('SELECT * FROM ' + @TableName + ' WHERE 1=1')",
        "EXECUTE ('SELECT * FROM ' + @TableName + ' WHERE 1=1')",
    );
    round_trips("EXECUTE foo");
    round_trips("EXECUTE foo.bar 1, 'a'");
    round_trips("EXECUTE foo(@a, 1)");
    round_trips("EXECUTE foo @a = 1, @b = 2 OUTPUT");
    round_trips("EXECUTE foo DEFAULT");
    round_trips("EXECUTE 'SELECT 1'");
}

#[test]
fn declare() {
    round_trips("DECLARE @bar INT");
    round_trips("DECLARE @a INT, @b INT");
    round_trips("DECLARE vend_cursor CURSOR FOR SELECT * FROM Purchasing.Vendor");
    parses_to(
        "DECLARE @foo CURSOR, @bar INT, @baz AS TEXT = 'foobar';",
        "DECLARE @foo CURSOR, @bar INT, @baz TEXT = 'foobar'",
    );
    parses_to(
        "DECLARE @bar INT;SET @bar = 2;SELECT @bar * 4",
        "DECLARE @bar INT; SET @bar = 2; SELECT @bar * 4",
    );
    rejected("DECLARE foo INT");
    let cursor_script = "DECLARE Employee_Cursor CURSOR FOR SELECT LastName, FirstName FROM AdventureWorks2022.HumanResources.vEmployee WHERE LastName LIKE 'B%'; OPEN Employee_Cursor; FETCH NEXT FROM Employee_Cursor; WHILE @@FETCH_STATUS = 0 BEGIN FETCH NEXT FROM Employee_Cursor; END; CLOSE Employee_Cursor; DEALLOCATE Employee_Cursor";
    parses_to(cursor_script, cursor_script);
}

#[test]
fn set_session_params() {
    round_trips("SET QUOTED_IDENTIFIER ON");
    round_trips("SET NOCOUNT OFF");
    round_trips("SET DEADLOCK_PRIORITY -8");
    round_trips("SET DEADLOCK_PRIORITY @xxx");
    round_trips("SET DEADLOCK_PRIORITY LOW");
    round_trips("SET DEADLOCK_PRIORITY 'LOW'");
    round_trips("SET LANGUAGE Italian");
    round_trips("SET ROWCOUNT 4");
    round_trips("SET ROWCOUNT @@xxx");
    round_trips("SET LOCK_TIMEOUT 1800");
    round_trips("SET TEXTSIZE 11");
    round_trips("SET DATEFIRST 7");
    round_trips("SET DATEFORMAT dmy");
    round_trips("SET DATEFORMAT @datevar");
    round_trips("SET ANSI_NULLS, ANSI_PADDING ON");
    round_trips("SET STATISTICS IO ON");
    round_trips("SET STATISTICS PROFILE OFF");
    round_trips("SET IDENTITY_INSERT dbo.Tool ON");
    round_trips("SET OFFSETS SELECT, FROM, ORDER, TABLE, PROCEDURE, STATEMENT, PARAM, EXECUTE ON");
    round_trips("SET @A = 1");
    round_trips("SET a.b = 1");
    round_trips("SET TIME ZONE 'UTC'");
    rejected("SET STATISTICS IO 1");
    rejected("SET IDENTITY_INSERT t MAYBE");
}

#[test]
fn procedures() {
    round_trips("CREATE OR ALTER PROCEDURE foo AS SELECT 1;");
    round_trips("CREATE OR ALTER PROCEDURE foo AS BEGIN SELECT 1; END");
    round_trips("CREATE PROCEDURE foo AS BEGIN SELECT 1; END");
    round_trips("CREATE PROCEDURE foo AS BEGIN SELECT [myColumn] FROM [myschema].[mytable]; END");
    round_trips(
        "CREATE OR ALTER PROCEDURE test (@foo INT, @bar VARCHAR(256)) AS BEGIN SELECT 1; END",
    );
    round_trips(
        "CREATE PROCEDURE foo (IN @a INTEGER = 1, OUT @b TEXT = '2', INOUT @c DATETIME = NULL, @d BOOL = 0) AS BEGIN SELECT 1; END",
    );
    round_trips(
        "CREATE PROCEDURE [foo] AS BEGIN UPDATE bar SET col = 'test'; SELECT [foo] FROM BAR WHERE [FOO] > 10; END",
    );
    round_trips("CREATE PROCEDURE test_proc LANGUAGE sql AS BEGIN SELECT 1; END");
    parses_to(
        "CREATE PROC test AS BEGIN SELECT 1; END",
        "CREATE PROCEDURE test AS BEGIN SELECT 1; END",
    );
    parses_to(
        "CREATE OR ALTER PROC test AS BEGIN SELECT 1; END",
        "CREATE OR ALTER PROCEDURE test AS BEGIN SELECT 1; END",
    );
    rejected("CREATE PROCEDURE foo");
    rejected("CREATE OR ALTER PROCEDURE foo AS SELECT 1");
    round_trips("DROP FUNCTION f([Role] INT)");
    round_trips("CREATE OR ALTER VIEW v AS SELECT 1");
}

#[test]
fn functions() {
    round_trips(
        "CREATE FUNCTION test_func(@param1 INT = 42) RETURNS INT AS BEGIN RETURN @param1; END",
    );
    round_trips(
        "CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) RETURNS INT AS BEGIN SET @foo = @foo + 1; RETURN @foo; END",
    );
    round_trips(
        "CREATE OR ALTER FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) RETURNS INT AS BEGIN SET @foo = @foo + 1; RETURN @foo; END",
    );
    round_trips(
        "CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) RETURNS INT AS BEGIN RETURN CONVERT(INT, 1) + 2; END",
    );
    round_trips(
        "CREATE FUNCTION some_scalar_udf() RETURNS INT AS BEGIN IF 1 = 2 BEGIN RETURN 1; END; RETURN 0; END",
    );
    parses_to(
        "CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) RETURNS INT BEGIN SET @foo = @foo + 1; RETURN @foo; END",
        "CREATE FUNCTION some_scalar_udf(@foo INT, @bar VARCHAR(256)) RETURNS INT AS BEGIN SET @foo = @foo + 1; RETURN @foo; END",
    );
    round_trips(
        "CREATE FUNCTION some_inline_tvf(@foo INT, @bar VARCHAR(256)) RETURNS TABLE AS RETURN (SELECT 1 AS col_1)",
    );
    round_trips(
        "CREATE FUNCTION some_inline_tvf(@foo INT, @bar VARCHAR(256)) RETURNS TABLE AS RETURN SELECT 1 AS col_1",
    );
    parses_to(
        "CREATE FUNCTION some_inline_tvf(@foo INT, @bar VARCHAR(256)) RETURNS TABLE RETURN (SELECT 1 col_1)",
        "CREATE FUNCTION some_inline_tvf(@foo INT, @bar VARCHAR(256)) RETURNS TABLE AS RETURN (SELECT 1 AS col_1)",
    );
    round_trips(
        "CREATE FUNCTION some_multi_statement_tvf(@foo INT, @bar VARCHAR(256)) RETURNS @t TABLE (col_1 INT) AS BEGIN INSERT INTO @t SELECT 1; RETURN; END",
    );
    round_trips(
        "CREATE FUNCTION some_multi_statement_tvf(@foo INT, @bar VARCHAR(256)) RETURNS @t TABLE (col_1 INT NOT NULL) AS BEGIN INSERT INTO @t SELECT 1; RETURN @t; END",
    );
    parses_to(
        "CREATE FUNCTION some_multi_statement_tvf(@foo INT, @bar VARCHAR(256)) RETURNS @t TABLE (col_1 INT) BEGIN INSERT INTO @t SELECT 1; RETURN; END",
        "CREATE FUNCTION some_multi_statement_tvf(@foo INT, @bar VARCHAR(256)) RETURNS @t TABLE (col_1 INT) AS BEGIN INSERT INTO @t SELECT 1; RETURN; END",
    );
    rejected("CREATE FUNCTION f(@foo INT) RETURNS @t TABLE BEGIN RETURN; END");
    rejected("CREATE FUNCTION f(@foo INT) RETURNS TABLE AS RETURN");
    rejected("CREATE FUNCTION f(@foo INT) RETURNS TABLE AS RETURN UPDATE t SET a = 1");
    rejected("CREATE FUNCTION f(@foo INT) RETURNS INT AS SELECT 1");
}

#[test]
fn triggers() {
    round_trips("CREATE TRIGGER some_trigger ON some_table FOR INSERT AS BEGIN RETURN; END");
    round_trips(
        "CREATE TRIGGER some_trigger ON some_table FOR INSERT AS BEGIN DECLARE @var INT; RAISERROR('Trigger fired', 10, 1); END",
    );
    round_trips(
        "CREATE TRIGGER some_trigger ON some_table FOR INSERT AS DECLARE @var INT; RAISERROR('Trigger fired', 10, 1);",
    );
    round_trips(
        "CREATE TRIGGER some_trigger ON some_table FOR INSERT AS BEGIN IF 1 = 2 BEGIN RAISERROR('Trigger fired', 10, 1); END; RETURN; END",
    );
    round_trips(
        "CREATE OR ALTER TRIGGER reminder1 ON Sales.Customer AFTER INSERT, UPDATE AS RAISERROR('Notify Customer Relations', 16, 10);",
    );
    round_trips("DROP TRIGGER emp_stamp");
    parses_to("DROP TRIGGER emp_stamp;", "DROP TRIGGER emp_stamp");
}

#[test]
fn table_options() {
    round_trips(
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (DISTRIBUTION = ROUND_ROBIN, PARTITION (column_a RANGE FOR VALUES (10, 11)))",
    );
    round_trips(
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (PARTITION (column_a RANGE LEFT FOR VALUES (10, 11)))",
    );
    round_trips(
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE INDEX)",
    );
    round_trips(
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE INDEX ORDER (column_a, column_b))",
    );
    round_trips(
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED INDEX (column_a ASC, column_b DESC, column_c))",
    );
    round_trips(
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (DISTRIBUTION = HASH(column_a, column_b), HEAP)",
    );
    for sql in [
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE INDEX ORDER ())",
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (CLUSTERED COLUMNSTORE)",
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (HEAP INDEX)",
        "CREATE TABLE mytable (column_a INT, column_b INT, column_c INT) WITH (PARTITION (RANGE LEFT FOR VALUES (10, 11)))",
    ] {
        rejected(sql);
    }
    round_trips("CREATE TABLE mytable (columnA INT IDENTITY NOT NULL)");
    round_trips("CREATE TABLE mytable (columnA INT IDENTITY(1, 1) NOT NULL)");
    round_trips("CREATE TABLE example (var_binary_col VARBINARY(MAX))");
    round_trips("CREATE TABLE example (var_binary_col VARBINARY(50))");
    round_trips(r#"CREATE TABLE "foo" ("bar" "int")"#);
}

#[test]
fn output_clauses() {
    round_trips(
        "INSERT INTO customers (name, email) OUTPUT INSERTED.id, INSERTED.name VALUES ('John', 'john@example.com')",
    );
    round_trips(
        "INSERT INTO customers (name, email) OUTPUT INSERTED.id, INSERTED.name INTO @new_ids VALUES ('John', 'john@example.com')",
    );
    round_trips(
        "UPDATE employees SET salary = salary * 1.1 OUTPUT INSERTED.id, DELETED.salary, INSERTED.salary WHERE department = 'Engineering'",
    );
    round_trips(
        "UPDATE employees SET salary = salary * 1.1 OUTPUT INSERTED.id, DELETED.salary, INSERTED.salary INTO @changes WHERE department = 'Engineering'",
    );
    round_trips("DELETE FROM customers OUTPUT DELETED.* WHERE id = 1");
    round_trips(
        "DELETE FROM customers OUTPUT DELETED.id, DELETED.name INTO @deleted_rows WHERE active = 0",
    );
    round_trips(
        "MERGE dso.products AS t USING dsi.products AS s ON s.ProductID = t.ProductID WHEN MATCHED AND NOT (t.ProductName = s.ProductName OR (ISNULL(t.ProductName, s.ProductName) IS NULL)) THEN UPDATE SET t.ProductName = s.ProductName WHEN NOT MATCHED BY TARGET THEN INSERT (ProductID, ProductName) VALUES (s.ProductID, s.ProductName) WHEN NOT MATCHED BY SOURCE THEN DELETE OUTPUT $action, deleted.ProductID INTO dsi.temp_products",
    );
}

#[test]
fn roles_and_permissions() {
    round_trips("CREATE ROLE mssql AUTHORIZATION helena");
    round_trips("ALTER ROLE role_name ADD MEMBER new_member");
    round_trips("ALTER ROLE role_name DROP MEMBER old_member");
    parses_to(
        "ALTER ROLE old_name WITH NAME = new_name",
        "ALTER ROLE old_name RENAME TO new_name",
    );
    rejected("ALTER ROLE r");
    round_trips("GRANT SELECT ON my_table TO public, db_admin");
    round_trips("DENY SELECT ON my_table TO public, db_admin");
    round_trips("DENY SELECT ON t TO u CASCADE AS dbo");
    rejected("DENY SELECT TO u");
}

#[test]
fn other_dialects_keep_their_syntax() {
    for sql in [
        "SELECT TOP (5) * FROM foo",
        "SELECT [a] FROM t",
        "SELECT @@version",
        "SELECT a FROM t WITH (NOLOCK)",
        "SELECT * FROM a CROSS APPLY f(a.b)",
        "IF 1 = 1 SELECT 1",
        "SELECT * FROM t FOR XML AUTO",
        "SET NOCOUNT ON",
        "SELECT $123.45",
    ] {
        assert!(parse_sql::<Core>(sql).is_err(), "Core accepted: {sql}");
    }
    assert!(parse_sql::<MsSql>("SELECT * FROM t WITH (NOLOCK)").is_ok());
    rejected("SELECT COUNT(*) FILTER (WHERE i > 0) FROM t");
}

mod shape {
    use sqlparser::ast::*;
    use sqlparser::dialect::MsSql;
    use sqlparser::dialect::builtin::{
        Builtin, BuiltinExpr, BuiltinQueryClauses, BuiltinStatement, BuiltinTableFactor,
    };
    use sqlparser::dialect::mssql::*;
    use sqlparser::{parse_query, parse_sql};

    fn select(sql: &str) -> Select<'_, Builtin> {
        match parse_query::<MsSql>(sql).unwrap().body {
            SetExpr::Select(select) => *select,
            other => panic!("not a select: {other:?}"),
        }
    }

    fn statement(sql: &str) -> MsSqlStatement<'_> {
        match parse_sql::<MsSql>(sql).unwrap().remove(0).kind {
            StatementKind::Ext(BuiltinStatement::MsSql(statement)) => statement,
            other => panic!("not an MsSql statement: {other:?}"),
        }
    }

    fn ident_text<'e>(expr: &'e Expr<'_, Builtin>) -> &'e str {
        match &expr.kind {
            ExprKind::Ident(ident) => ident.text.as_ref(),
            other => panic!("not an identifier: {other:?}"),
        }
    }

    #[test]
    fn identifiers_keep_their_quoting() {
        let select = select("SELECT [a b], @x, ##t FROM t");
        let SelectItemKind::Expr { expr, .. } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        let ExprKind::Ident(ident) = &expr.kind else {
            panic!("not an identifier");
        };
        assert_eq!(ident.text, "a b");
        assert_eq!(ident.quote, Some('['));
    }

    #[test]
    fn top_is_a_select_modifier() {
        let select = select("SELECT DISTINCT TOP (5) PERCENT WITH TIES a FROM t");
        assert_eq!(select.quantifier, SetQuantifier::Distinct);
        let top = select.modifiers.mssql_top.expect("a TOP clause");
        assert!(top.parenthesized && top.percent && top.with_ties);
        assert_eq!(top.quantity.to_string(), "5");
    }

    #[test]
    fn eq_alias_names_the_select_item() {
        let select = select("SELECT x = 1 FROM t");
        let SelectItemKind::Expr { expr, alias } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        assert_eq!(expr.to_string(), "1");
        assert_eq!(alias.as_ref().unwrap().text, "x");
    }

    #[test]
    fn string_aliases_are_single_quoted_identifiers() {
        let select = select("SELECT a 'b c' FROM t");
        let SelectItemKind::Expr { alias, .. } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        assert_eq!(alias.as_ref().unwrap().text, "b c");
        assert_eq!(alias.as_ref().unwrap().quote, Some('\''));
    }

    #[test]
    fn national_strings_are_flagged() {
        let select = select("SELECT N'x'");
        let SelectItemKind::Expr { expr, .. } = &select.projection[0].kind else {
            panic!("not an expression");
        };
        let ExprKind::Literal(Literal::String(string)) = &expr.kind else {
            panic!("not a string");
        };
        assert!(string.national);
        assert_eq!(string.text, "x");
    }

    #[test]
    fn apply_and_hints() {
        let select = select("SELECT 1 FROM t AS a WITH (NOLOCK) CROSS APPLY f(a.x) AS g");
        let from = &select.from[0];
        let TableFactor::Ext(BuiltinTableFactor::MsSql(MsSqlTableFactor::Hinted(hinted))) =
            &from.relation.factor
        else {
            panic!("not hinted");
        };
        assert_eq!(hinted.table.alias.as_ref().unwrap().name.text, "a");
        assert_eq!(hinted.hints[0].to_string(), "NOLOCK");
        assert!(matches!(
            from.joins[0].kind,
            JoinKind::Custom(std::borrow::Cow::Borrowed("CROSS APPLY"))
        ));
    }

    #[test]
    fn openjson_columns() {
        let select = select(
            "SELECT 1 FROM OPENJSON(a, '$.x') WITH (k VARCHAR(20) '$.k', v NVARCHAR(MAX) '$.v' AS JSON)",
        );
        let TableFactor::Ext(BuiltinTableFactor::MsSql(MsSqlTableFactor::OpenJson(openjson))) =
            &select.from[0].relation.factor
        else {
            panic!("not OPENJSON");
        };
        assert!(openjson.path.is_some());
        assert_eq!(openjson.columns.len(), 2);
        assert!(!openjson.columns[0].as_json);
        assert!(openjson.columns[1].as_json);
    }

    #[test]
    fn for_clause_is_in_the_query_tail() {
        let query = parse_query::<MsSql>("SELECT a FROM t FOR JSON PATH, ROOT('x')").unwrap();
        let Some(BuiltinQueryClauses::MsSql(MsSqlForClause::Json(json))) =
            query.tail.0.map(|tail| *tail)
        else {
            panic!("not FOR JSON");
        };
        assert_eq!(json.mode, "PATH");
        assert_eq!(json.root.unwrap().text, "x");
    }

    #[test]
    fn convert_and_method_expressions() {
        let select = select("SELECT TRY_CONVERT(INT, a, 1)");
        let SelectItemKind::Expr {
            expr:
                Expr {
                    kind: ExprKind::Ext(BuiltinExpr::MsSql(MsSqlExpr::Convert(convert))),
                    ..
                },
            ..
        } = &select.projection[0].kind
        else {
            panic!("not CONVERT");
        };
        assert!(convert.is_try);
        assert_eq!(convert.styles.len(), 1);
    }

    #[test]
    fn output_clause_position() {
        let parsed =
            parse_sql::<MsSql>("UPDATE t SET a = 1 OUTPUT INSERTED.a WHERE b = 2").unwrap();
        let StatementKind::Update(update) = &parsed[0].kind else {
            panic!("not an update");
        };
        let output = update.output.as_ref().expect("an OUTPUT clause");
        assert_eq!(output.items.len(), 1);
        assert!(output.into.is_none());
        assert!(update.selection.is_some());
    }

    #[test]
    fn if_statement_branches() {
        let MsSqlStatement::If(statement) = statement("IF a BEGIN SELECT 1; END ELSE SELECT 2;")
        else {
            panic!("not IF");
        };
        assert_eq!(ident_text(&statement.condition), "a");
        let MsSqlBody::Block(then_branch) = &statement.then_branch else {
            panic!("not a block");
        };
        assert_eq!(then_branch.len(), 1);
        assert!(matches!(
            statement.else_branch,
            Some(MsSqlBody::Statements(_))
        ));
    }

    #[test]
    fn declare_items() {
        let MsSqlStatement::Declare(items) = statement("DECLARE @a INT, @b CURSOR") else {
            panic!("not DECLARE");
        };
        assert_eq!(items.len(), 2);
        assert!(items[0].data_type.is_some() && !items[0].cursor);
        assert!(items[1].cursor);
    }

    #[test]
    fn execute_dynamic_sql_has_no_name() {
        let MsSqlStatement::Execute(execute) = statement("EXEC (@sql)") else {
            panic!("not EXEC");
        };
        assert!(execute.name.is_none());
        assert!(execute.has_parentheses);
    }

    #[test]
    fn create_function_returns_a_table() {
        let MsSqlStatement::CreateFunction(function) =
            statement("CREATE FUNCTION f(@a INT) RETURNS @t TABLE (c INT) AS BEGIN RETURN; END")
        else {
            panic!("not CREATE FUNCTION");
        };
        assert!(!function.or_alter);
        assert_eq!(function.params.len(), 1);
        let MsSqlReturns::Table { name, columns } = &function.returns else {
            panic!("not a table");
        };
        assert_eq!(name.as_ref().unwrap().text, "@t");
        assert_eq!(columns.len(), 1);
        assert!(matches!(function.body, MsSqlFunctionBody::Block(_)));
    }

    #[test]
    fn set_session_param_kinds() {
        let MsSqlStatement::SessionParam(param) = statement("SET ANSI_NULLS, ANSI_PADDING ON")
        else {
            panic!("not a session param");
        };
        let MsSqlSessionParam::Generic { names, value } = *param else {
            panic!("not generic");
        };
        assert_eq!(names, ["ANSI_NULLS", "ANSI_PADDING"]);
        assert_eq!(ident_text(&value), "ON");
    }

    #[test]
    fn table_options_after_columns() {
        let MsSqlStatement::CreateTable(table) =
            statement("CREATE TABLE t (a INT) WITH (HEAP, DISTRIBUTION = ROUND_ROBIN)")
        else {
            panic!("not a table");
        };
        assert_eq!(table.table.columns.len(), 1);
        assert!(matches!(table.options[0], MsSqlTableOption::Heap));
        assert!(matches!(
            table.options[1],
            MsSqlTableOption::KeyValue { .. }
        ));
    }

    #[test]
    fn double_dot_names_keep_the_empty_schema() {
        let select = select("SELECT 1 FROM db..t");
        let TableFactor::Named(name) = &select.from[0].relation.factor else {
            panic!("not a name");
        };
        assert_eq!(name.parts.len(), 3);
        assert_eq!(name.parts[1].text, "");
    }
}
