//! Schema qualifiers, from the DDL that spells them to the attribute the
//! generated entity carries — the identity that keeps a `tenant_a.item`
//! entity off `public.item` whatever `search_path` says.

mod common;

use common::*;
use pgorm_codegen::sql_schema::entities_from_sql;
use pgorm_codegen::{Error, WriterOutput};

fn from_sql(sql: &str, opts: Opts) -> Generated {
    Generated {
        files: files(entities_from_sql(sql, opts).expect("schema should generate")),
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
fn assert_error(sql: &str, expected: &str) {
    match entities_from_sql(sql, Opts::default()) {
        Err(Error::TransformError(message)) => assert_eq!(message, expected),
        Err(other) => panic!("expected a TransformError, got {other:?}"),
        Ok(_) => panic!("expected an error, got generated entities"),
    }
}

const QUALIFIED: &str = "CREATE TABLE tenant_a.item (id int PRIMARY KEY);";

// [spec:pgorm:sem:codegen.entity.transform+7/test]    the source table's schema
// survives transformation and reaches the compact entity
// [spec:pgorm:def:codegen.entity.compact+1/test]
#[test]
fn compact_entity_carries_the_source_schema() {
    let generated = from_sql(QUALIFIED, Opts::default());

    assert_contains(
        generated.file("item.rs"),
        r#"#[pgorm(schema_name = "tenant_a", table_name = "item")]"#,
    );
}

// [spec:pgorm:def:codegen.entity.expanded+1/test]    and the expanded
// `EntityName::schema_name`, which is what qualifies every generated statement
#[test]
fn expanded_entity_name_carries_the_source_schema() {
    let generated = from_sql(QUALIFIED, expanded());

    assert_contains(
        generated.file("item.rs"),
        r#"
        impl EntityName for Entity {
            fn schema_name(&self) -> Option<&str> {
                Some("tenant_a")
            }
            fn table_name(&self) -> &str {
                "item"
            }
        }
        "#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    an unqualified table
// still generates no schema of its own
#[test]
fn an_unqualified_table_carries_no_schema() {
    let generated = from_sql("CREATE TABLE item (id int PRIMARY KEY);", Opts::default());

    assert_contains(
        generated.file("item.rs"),
        r#"#[pgorm(table_name = "item")]"#,
    );
    assert_not_contains(generated.file("item.rs"), "schema_name");
}

// [spec:pgorm:sem:codegen.entity.context+2/test]    the configured schema is a
// default, not an override: it fills in for tables the DDL left unqualified and
// never contradicts one that names its own schema
#[test]
fn the_configured_schema_is_only_a_default() {
    let generated = from_sql(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         CREATE TABLE stock (id int PRIMARY KEY);",
        Opts {
            schema_name: Some("public".to_owned()),
            ..Default::default()
        },
    );

    assert_contains(
        generated.file("item.rs"),
        r#"#[pgorm(schema_name = "tenant_a", table_name = "item")]"#,
    );
    assert_contains(
        generated.file("stock.rs"),
        r#"#[pgorm(schema_name = "public", table_name = "stock")]"#,
    );
}

// [spec:pgorm:sem:codegen.entity.context+2/test]    the same precedence in the
// expanded format
#[test]
fn the_expanded_format_defaults_the_schema_too() {
    let generated = from_sql(
        QUALIFIED,
        Opts {
            expanded_format: true,
            schema_name: Some("public".to_owned()),
            ..Default::default()
        },
    );

    assert_contains(
        generated.file("item.rs"),
        r#"fn schema_name(&self) -> Option<&str> { Some("tenant_a") }"#,
    );
}

// [spec:pgorm:req:codegen.entity.collisions+1/test]    two schemas' same-named
// tables are two tables that want one file, and the gate says so
#[test]
fn same_name_in_two_schemas_is_refused() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         CREATE TABLE tenant_b.item (id int PRIMARY KEY);",
        "tables `tenant_a.item` and `tenant_b.item` both generate the module name `item`: \
         same-named tables in different schemas are different tables, and need one generation run \
         each",
    );
}

// [spec:pgorm:req:codegen.entity.collisions+1/test]    a qualified table and an
// unqualified one of the same name collide the same way — whether they are one
// table is `search_path`'s business, and either way there is one `item.rs`
#[test]
fn qualified_and_unqualified_namesakes_are_refused() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         CREATE TABLE item (id int PRIMARY KEY);",
        "tables `tenant_a.item` and `item` both generate the module name `item`: same-named \
         tables in different schemas are different tables, and need one generation run each",
    );
}

// [spec:pgorm:req:codegen.entity.collisions+1/test]    one identity declared
// twice is a duplicate, not a collision
#[test]
fn a_table_declared_twice_is_refused() {
    use pgorm_codegen::EntityTransformer;

    let twice = vec![qualified_item(), qualified_item()];

    match EntityTransformer::transform(twice) {
        Err(Error::TransformError(message)) => {
            assert_eq!(message, "table `tenant_a.item` is declared twice");
        }
        other => panic!("expected a TransformError, got {other:?}"),
    }
}

fn qualified_item() -> pgorm_query::TableCreateStatement {
    pgorm_query::Table::create((alias("tenant_a"), alias("item")))
        .col(serial_pk("id"))
        .to_owned()
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    a qualified foreign key
// resolves to the table it names, across schemas
#[test]
fn a_foreign_key_resolves_across_schemas() {
    let generated = from_sql(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         CREATE TABLE tenant_b.cart (id int PRIMARY KEY, item_id int REFERENCES tenant_a.item(id));",
        Opts::default(),
    );

    assert_contains(
        generated.file("cart.rs"),
        r#"#[pgorm(schema_name = "tenant_b", table_name = "cart")]"#,
    );
    assert_contains(
        generated.file("cart.rs"),
        r#"belongs_to = "super::item::Entity""#,
    );
    assert_contains(
        generated.file("item.rs"),
        r#"has_many = "super::cart::Entity""#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    an unqualified reference
// resolves to the one table with that bare name, whatever schema it is in
#[test]
fn an_unqualified_key_reaches_a_qualified_table() {
    let generated = from_sql(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         CREATE TABLE cart (id int PRIMARY KEY, item_id int REFERENCES item(id));",
        Opts::default(),
    );

    assert_contains(
        generated.file("cart.rs"),
        r#"belongs_to = "super::item::Entity""#,
    );
    assert_contains(
        generated.file("item.rs"),
        r#"has_many = "super::cart::Entity""#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    a key onto another
// schema's table is a key onto a table this schema does not define
#[test]
fn a_key_onto_another_schemas_table_is_unresolved() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         CREATE TABLE cart (id int PRIMARY KEY, item_id int REFERENCES tenant_b.item(id));",
        "table `cart`: relation to `tenant_b.item` names a table the schema does not define",
    );
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    self-reference is decided
// on identity: a qualified table keying itself is still `SelfRef`
#[test]
fn a_qualified_table_can_reference_itself() {
    let generated = from_sql(
        "CREATE TABLE tenant_a.node (id int PRIMARY KEY, parent int REFERENCES tenant_a.node(id));",
        Opts::default(),
    );

    assert_contains(generated.file("node.rs"), "SelfRef");
    assert_contains(
        generated.file("node.rs"),
        r#"belongs_to = "Entity", from = "Column::Parent", to = "Column::Id""#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    the gate names a table by
// its identity, so a failure in one of two schemas' tables says which
#[test]
fn a_refusal_names_the_qualified_table() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY, \"1st\" int);",
        "table `tenant_a.item` column `1st`: `1st` is not a valid Rust identifier",
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    an index attaches to the table
// its own name resolves to, not to whatever shares the bare name
#[test]
fn an_index_attaches_by_qualified_name() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY, sku text);
         CREATE UNIQUE INDEX item_sku ON tenant_b.item (sku);",
        "statement 2: no CREATE TABLE for table `tenant_b.item`",
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    and a qualified index that does
// name its table is folded into it, giving the column its `unique`
#[test]
fn a_qualified_index_reaches_its_own_table() {
    let generated = from_sql(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY, sku text);
         CREATE UNIQUE INDEX item_sku ON tenant_a.item (sku);",
        Opts::default(),
    );

    assert_contains(
        generated.file("item.rs"),
        r#"#[pgorm(column_type = "Text", nullable, unique)] pub sku"#,
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    a comment resolves the same way
#[test]
fn a_comment_attaches_by_qualified_name() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY);
         COMMENT ON TABLE tenant_b.item IS 'the other one';",
        "statement 2: no CREATE TABLE for table `tenant_b.item`",
    );
}

// [spec:pgorm:sem:codegen.ddl.objects+1/test]    an unqualified reference that
// two tables answer to is named as such, rather than attached to one of them
#[test]
fn an_ambiguous_unqualified_index_is_refused() {
    assert_error(
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY, sku text);
         CREATE TABLE tenant_b.item (id int PRIMARY KEY, sku text);
         CREATE UNIQUE INDEX item_sku ON item (sku);",
        "statement 3: `item` names more than one table",
    );
}

// [spec:pgorm:sem:codegen.ddl.tables+2/test]    the bridge preserved the
// qualifier all along; it is the whole pipeline that now keeps it
#[test]
fn bridge_and_generated_entity_agree_on_schema() {
    use pgorm_codegen::sql_schema::parse_schema;
    use pgorm_query::TableName;

    let statements = parse_schema(QUALIFIED).expect("schema should parse");
    let [statement] = statements.as_slice() else {
        panic!("expected one statement, got {}", statements.len());
    };

    assert!(matches!(
        statement.get_table_name(),
        TableName::SchemaTable(schema, table)
            if schema.to_string() == "tenant_a" && table.to_string() == "item"
    ));
    assert_contains(
        from_sql(QUALIFIED, Opts::default()).file("item.rs"),
        r#"schema_name = "tenant_a""#,
    );
}
