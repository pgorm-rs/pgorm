//! The DDL bridge: `schema.sql` in, the same entities the statement-building
//! path produces out — and a named error for everything else in the file.

mod common;

use common::*;
use pgorm_codegen::sql_schema::{entities_from_sql, parse_schema};
use pgorm_codegen::{Error, WriterOutput};
use pgorm_query::extension::Type;
use pgorm_query::{ColumnSpec, ColumnType, TableName};

const SCHEMA: &str = include_str!("sql/schema.sql");

fn from_sql(sql: &str) -> Generated {
    Generated {
        files: files(entities_from_sql(sql, Opts::default()).expect("schema should generate")),
    }
}

fn files(output: WriterOutput) -> Vec<(String, String)> {
    output
        .files
        .into_iter()
        .map(|file| (file.name, file.content))
        .collect()
}

#[track_caller]
fn error(sql: &str) -> String {
    match entities_from_sql(sql, Opts::default()) {
        Err(Error::TransformError(message)) => message,
        Err(other) => panic!("expected a TransformError, got {other:?}"),
        Ok(_) => panic!("expected an error, got generated entities"),
    }
}

#[track_caller]
fn assert_error(sql: &str, expected: &str) {
    assert_eq!(error(sql), expected);
}

// [spec:pgorm:def:codegen.ddl+2/test]    the whole pipeline runs from DDL text:
// one entity file per CREATE TABLE, plus index, prelude and active enums
#[test]
fn schema_sql_generates_one_file_per_table() {
    let generated = from_sql(SCHEMA);

    assert_eq!(
        generated.names(),
        [
            "label.rs",
            "owner.rs",
            "task.rs",
            "task_label.rs",
            "mod.rs",
            "prelude.rs",
            "pgorm_active_enums.rs",
        ]
    );
}

// [spec:pgorm:sem:codegen.ddl.types+2/test]    the type spellings map onto the
// ColumnType vocabulary, serial included
#[test]
fn column_types_map_through_the_vocabulary() {
    let generated = from_sql(SCHEMA);
    let task = generated.file("task.rs");

    assert_contains(task, "#[pgorm(primary_key)] pub id: i64,");
    assert_contains(task, "pub owner_id: i32,");
    assert_contains(task, r#"#[pgorm(column_type = "Text")] pub title: String,"#);
    assert_contains(task, "pub state: TaskState,");
    assert_contains(
        task,
        r#"#[pgorm(column_type = "Double", nullable)] pub weight: Option<f64>,"#,
    );
    assert_contains(task, "pub tags: Option<Vec<String>>,");
    assert_contains(task, "pub due: Option<DateTimeWithTimeZone>,");
    assert_contains(
        task,
        r#"#[pgorm(column_type = "JsonBinary", nullable)] pub body: Option<Json>,"#,
    );
    assert_contains(task, "pub r#ref: Option<Uuid>,");
    assert_contains(generated.file("owner.rs"), "pub name: String,");
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    a CREATE TYPE ... AS ENUM
// reaches the generated active enum through the columns that name it
#[test]
fn enum_type_reaches_the_generated_active_enum() {
    let generated = from_sql(SCHEMA);
    let enums = generated.file("pgorm_active_enums.rs");

    assert_contains(
        enums,
        r#"#[pgorm(rs_type = "String", db_type = "Enum", enum_name = "task_state")]"#,
    );
    assert_contains(enums, "pub enum TaskState");
    assert_contains(enums, r#"#[pgorm(string_value = "open")] Open,"#);
    assert_contains(enums, r#"#[pgorm(string_value = "closed")] Closed,"#);
    assert_contains(
        generated.file("task.rs"),
        "use super::pgorm_active_enums::TaskState;",
    );
}

// [spec:pgorm:sem:codegen.ddl.tables+2/test]    a foreign key keeps its columns
// and its declared actions
#[test]
fn foreign_keys_keep_their_columns_and_actions() {
    let generated = from_sql(SCHEMA);

    assert_contains(
        generated.file("task.rs"),
        r#"#[pgorm(belongs_to = "super::owner::Entity", from = "Column::OwnerId", to = "super::owner::Column::Id", on_update = "Restrict", on_delete = "Cascade",)]"#,
    );
}

// [spec:pgorm:sem:codegen.ddl.tables+2/test]    a table-level composite primary
// key plus two foreign keys is read as a junction table
#[test]
fn composite_key_junction_becomes_conjunct_relations() {
    let generated = from_sql(SCHEMA);

    assert_contains(
        generated.file("task_label.rs"),
        "#[pgorm(primary_key, auto_increment = false)] pub task_id: i64,",
    );
    assert_contains(
        generated.file("task.rs"),
        "impl Related<super::label::Entity> for Entity",
    );
    assert_contains(
        generated.file("label.rs"),
        "impl Related<super::task::Entity> for Entity",
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    a single-column unique index
// marks its column unique; a plain index states no entity fact
#[test]
fn unique_index_marks_its_column_unique() {
    let generated = from_sql(SCHEMA);

    assert_contains(
        generated.file("owner.rs"),
        r#"#[pgorm(column_type = "Text", nullable, unique)] pub email: Option<String>,"#,
    );
    assert_contains(
        generated.file("task.rs"),
        "pub due: Option<DateTimeWithTimeZone>,",
    );
}

// [spec:pgorm:sem:codegen.ddl.tables+2/test]    a column-level UNIQUE becomes the
// index Postgres creates for it, which is where the entity model reads unique
#[test]
fn column_unique_constraint_marks_the_column() {
    let generated = from_sql("CREATE TABLE t (id serial PRIMARY KEY, email text UNIQUE);");

    assert_contains(
        generated.file("t.rs"),
        r#"#[pgorm(column_type = "Text", nullable, unique)] pub email: Option<String>,"#,
    );
}

// [spec:pgorm:sem:codegen.ddl.tables+2/test]    a schema-qualified name is kept
// as the schema-qualified table name the statement targets
#[test]
fn schema_qualified_table_names_are_kept() {
    const SQL: &str = "CREATE TABLE app.task (id serial PRIMARY KEY);";
    let tables = parse_schema(SQL).expect("schema should parse");
    let table = tables.first().expect("the task table");

    let TableName::SchemaTable(schema, name) = table.get_table_name() else {
        panic!("the task table should carry its schema");
    };
    assert_eq!(schema.to_string(), "app");
    assert_eq!(name.to_string(), "task");
    assert!(
        from_sql(SQL).has("task.rs"),
        "the entity is keyed by the table name alone"
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    COMMENT ON statements are folded
// into the table and column they describe
#[test]
fn comments_are_folded_into_their_table() {
    let tables = parse_schema(SCHEMA).expect("schema should parse");
    let task = tables.get(1).expect("the task table");

    let TableName::Table(name) = task.get_table_name() else {
        panic!("the task table should be a plain table name");
    };
    assert_eq!(name.to_string(), "task");
    assert_eq!(
        task.get_comment().map(String::as_str),
        Some("work to be done")
    );

    let title = task
        .get_columns()
        .iter()
        .find(|column| column.get_column_name() == "title")
        .expect("the title column");
    assert!(
        title
            .get_column_spec()
            .iter()
            .any(|spec| matches!(spec, ColumnSpec::Comment(text) if text == "short summary")),
        "the column comment should ride on the column definition"
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    a statement the bridge does
// not read is named, never skipped
#[test]
fn unsupported_statements_are_named() {
    assert_error(
        "CREATE TABLE t (id int); ALTER TABLE t ADD COLUMN b int;",
        "unsupported DDL: ALTER TABLE at statement 2",
    );
    assert_error(
        "CREATE TABLE t (id int); CREATE TRIGGER x BEFORE INSERT ON t EXECUTE FUNCTION f();",
        "unsupported DDL: CREATE TRIGGER at statement 2",
    );
    assert_error(
        "CREATE VIEW v AS SELECT 1;",
        "unsupported DDL: CREATE VIEW at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int); INSERT INTO t VALUES (1);",
        "unsupported DDL: INSERT at statement 2",
    );
    assert_error(
        "CREATE SCHEMA app;",
        "unsupported DDL: CREATE SCHEMA at statement 1",
    );
    assert_error(
        "CREATE SEQUENCE s;",
        "unsupported DDL: CREATE SEQUENCE at statement 1",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    a CREATE TABLE clause with
// no entity meaning is named rather than dropped
#[test]
fn unsupported_table_clauses_are_named() {
    assert_error(
        "CREATE TABLE t (id int) PARTITION BY RANGE (id);",
        "unsupported DDL: a PARTITION BY clause on table `t` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int) INHERITS (u);",
        "unsupported DDL: an INHERITS clause on table `t` at statement 1",
    );
    assert_error(
        "CREATE TEMP TABLE t (id int);",
        "unsupported DDL: a temporary or unlogged table on table `t` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int) WITH (fillfactor = 70);",
        "unsupported DDL: a WITH storage option on table `t` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (LIKE u);",
        "unsupported DDL: a LIKE clause on table `t` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int, CHECK (id > 0));",
        "unsupported DDL: a CHECK constraint on table `t` at statement 1",
    );
    assert_error(
        "CREATE TABLE other.app.t (id int);",
        "unsupported DDL: a cross-database table name on table `t` at statement 1",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    the same holds for column
// clauses the entity model has no room for
#[test]
fn unsupported_column_clauses_are_named() {
    assert_error(
        "CREATE TABLE t (id int DEFAULT 1);",
        "unsupported DDL: a DEFAULT clause on column `t`.`id` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int CHECK (id > 0));",
        "unsupported DDL: a CHECK constraint on column `t`.`id` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int GENERATED ALWAYS AS IDENTITY);",
        "unsupported DDL: an identity clause on column `t`.`id` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int, total int GENERATED ALWAYS AS (id * 2) STORED);",
        "unsupported DDL: a GENERATED clause on column `t`.`total` at statement 1",
    );
    assert_error(
        r#"CREATE TABLE t (name text COLLATE "C");"#,
        "unsupported DDL: a COLLATE clause on column `t`.`name` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (id int REFERENCES u);",
        "unsupported DDL: REFERENCES without a column list on column `t`.`id` at statement 1",
    );
}

// [spec:pgorm:sem:codegen.ddl.types+2/test]    a type spelling outside the
// vocabulary is named, and so is a modifier the vocabulary cannot hold
#[test]
fn unsupported_types_are_named() {
    assert_error(
        "CREATE TABLE t (data hstore);",
        "unsupported DDL: type `hstore` on column `t`.`data` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (at timestamp(3));",
        "unsupported DDL: `timestamp` with a type modifier on column `t`.`at` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (bits varbit);",
        "unsupported DDL: `varbit` without a length on column `t`.`bits` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (tags text[3]);",
        "unsupported DDL: a sized array on column `t`.`tags` at statement 1",
    );
    assert_error(
        "CREATE TABLE t (grid text[][]);",
        "unsupported DDL: a multi-dimensional array on column `t`.`grid` at statement 1",
    );
}

// [spec:pgorm:def:codegen.ddl+2/test]    a type the builder can spell but codegen
// cannot render passes the bridge and is refused by the transform gate
#[test]
fn types_codegen_cannot_render_reach_the_gate() {
    assert!(parse_schema("CREATE TABLE t (net inet);").is_ok());
    assert_error(
        "CREATE TABLE t (net inet);",
        "table `t` column `net`: column type Inet is not supported by codegen",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    an index clause the builder
// cannot express is named
#[test]
fn unsupported_index_clauses_are_named() {
    let table = "CREATE TABLE t (id int, name text);";
    assert_error(
        &format!("{table} CREATE INDEX i ON t (name) WHERE id > 0;"),
        "unsupported DDL: a WHERE clause on index `i` at statement 2",
    );
    assert_error(
        &format!("{table} CREATE INDEX i ON t (lower(name));"),
        "unsupported DDL: an expression column on index `i` at statement 2",
    );
    assert_error(
        &format!("{table} CREATE INDEX i ON t (name) INCLUDE (id);"),
        "unsupported DDL: an INCLUDE clause on index `i` at statement 2",
    );
    assert_error(
        &format!("{table} CREATE INDEX CONCURRENTLY i ON t (name);"),
        "unsupported DDL: CONCURRENTLY on index `i` at statement 2",
    );
    assert_error(
        &format!("{table} CREATE INDEX i ON t (name NULLS FIRST);"),
        "unsupported DDL: a NULLS FIRST or NULLS LAST clause on index `i` at statement 2",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    a COMMENT the bridge cannot
// attach is named
#[test]
fn unsupported_comment_targets_are_named() {
    assert_error(
        "COMMENT ON SCHEMA public IS 'x';",
        "unsupported DDL: COMMENT ON an object other than a table or column at statement 1",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    a statement that names an
// object the file does not declare is named too
#[test]
fn unresolved_references_are_named() {
    assert_error(
        "CREATE INDEX i ON missing (id);",
        "statement 1: no CREATE TABLE for table `missing`",
    );
    assert_error(
        "CREATE TABLE t (id int); COMMENT ON COLUMN t.missing IS 'x';",
        "statement 2: table `t` has no column `missing`",
    );
    assert_error(
        "CREATE TABLE t (id int); CREATE TABLE t (id int);",
        "statement 2: table `t` is declared twice",
    );
    assert_error(
        "CREATE TYPE s AS ENUM ('a'); CREATE TYPE s AS ENUM ('b');",
        "statement 2: type `s` is declared twice",
    );
}

// [spec:pgorm:req:codegen.ddl.unsupported+1/test]    a foreign key onto a table
// or a column the file never declares is named too — by the transform gate the
// whole pipeline runs, which is where every table is in hand at once
#[test]
fn unresolved_foreign_keys_are_named() {
    assert_error(
        "CREATE TABLE orders (id serial PRIMARY KEY, customer_id integer REFERENCES customers (id));",
        "table `orders`: relation to `customers` names a table the schema does not define",
    );
    assert_error(
        "CREATE TABLE customers (id serial PRIMARY KEY);
         CREATE TABLE orders (id serial PRIMARY KEY, customer_id integer REFERENCES customers (code));",
        "table `orders`: relation to `customers` references column `code`, which `customers` does \
         not have",
    );
}

// [spec:pgorm:def:codegen.ddl+2/test]    text the PostgreSQL grammar rejects
// comes back as the parser's own message
#[test]
fn invalid_sql_reports_the_parser_message() {
    let message = error("CREATE TABLE t (id int;");
    assert!(
        message.starts_with("schema SQL did not parse: "),
        "unexpected message: {message}"
    );
    assert!(
        message.contains("syntax error"),
        "unexpected message: {message}"
    );
}

// [spec:pgorm:def:codegen.ddl+2/test]    the bridge is the inverse of the DDL
// builder: statements rendered to text and parsed back generate the same
// entities as the statements themselves
#[test]
fn rendered_ddl_round_trips_through_the_bridge() {
    let statements = cake_schema();
    let text = statements
        .iter()
        .map(|statement| format!("{statement};"))
        .collect::<Vec<_>>()
        .join("\n");

    let direct = generate(cake_schema(), Opts::default());
    let round_tripped = from_sql(&text);

    assert_eq!(round_tripped.files, direct.files);
}

// [spec:pgorm:sem:codegen.ddl.types+2/test]    the types that once shared a
// spelling with another variant now each recover themselves
#[test]
fn one_spelling_one_variant_round_trips() {
    let statements = || {
        vec![table_with(
            "ledger",
            vec![
                serial_pk("id"),
                typed("payload", ColumnType::Bytea),
                typed("seen", ColumnType::Timestamp),
                typed("amount", ColumnType::Money),
                typed("width", ColumnType::SmallInteger),
            ],
        )]
    };
    let text = statements()
        .iter()
        .map(|statement| format!("{statement};"))
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(
        from_sql(&text).files,
        generate(statements(), Opts::default()).files
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    the round trip holds for the
// statements outside the table too: an enum type and a unique index
#[test]
fn enum_and_unique_index_round_trip() {
    let statements = || {
        let mut task = table_with(
            "task",
            vec![
                serial_pk("id"),
                enum_col("state", "task_state", &["open", "done"]),
                col("code").string().not_null().to_owned(),
            ],
        );
        task.index(&mut unique_index("task", "code"));
        vec![task.take()]
    };
    let enum_type = Type::create(alias("task_state"))
        .values(vec![alias("open"), alias("done")])
        .to_string();
    let text = statements()
        .iter()
        .map(|statement| format!("{statement};"))
        .fold(format!("{enum_type};\n"), |mut text, statement| {
            text.push_str(&statement);
            text
        });

    assert_eq!(
        from_sql(&text).files,
        generate(statements(), Opts::default()).files
    );
}
