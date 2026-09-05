//! Schema discovery -> `Entity` model: `EntityTransformer::transform`, the
//! inverse relations it synthesises, and the many-to-many conjunct relations it
//! derives from junction tables.

mod common;

use common::*;
use pgorm_codegen::Column;
use pgorm_query::{
    Alias, ColumnDef, ColumnType, ForeignKey, ForeignKeyAction, Index, IntoIden, Table,
    TableCreateStatement, TableName,
};

fn fk(from_table: &str, from_col: &str, to_table: &str, to_col: &str) -> TableCreateStatement {
    Table::create(Alias::new(from_table))
        .col(serial_pk("id"))
        .col(ColumnDef::new(Alias::new(from_col)).integer().to_owned())
        .foreign_key(&mut ForeignKey::create(
            Alias::new(from_table),
            Alias::new(from_col),
            Alias::new(to_table),
            Alias::new(to_col),
        ))
        .to_owned()
}

fn bare(table: &str) -> TableCreateStatement {
    table_with(table, vec![serial_pk("id")])
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    one Entity per input
// statement, held in a BTreeMap so every output is ordered by table name
#[test]
fn transform_builds_entity_per_statement_ordered_by_name() {
    let generated = generate(
        vec![bare("zebra"), bare("apple"), bare("mango")],
        Opts::default(),
    );

    assert_eq!(
        generated.names(),
        ["apple.rs", "mango.rs", "zebra.rs", "mod.rs", "prelude.rs"]
    );
    let index: Vec<&str> = generated
        .file("mod.rs")
        .lines()
        .filter(|l| l.starts_with("pub mod") && !l.contains("prelude"))
        .collect();
    assert_eq!(
        index,
        ["pub mod apple ;", "pub mod mango ;", "pub mod zebra ;"]
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    the table name is unpacked
// from every `TableName` form
#[test]
fn transform_unpacks_the_table_name_from_every_form() {
    let cake = || Alias::new("cake").into_iden();
    let schema = || Alias::new("public").into_iden();

    let names = [
        TableName::Table(cake()),
        TableName::SchemaTable(schema(), cake()),
    ];

    for table_name in names {
        let stmt = Table::create(table_name.clone())
            .col(serial_pk("id"))
            .to_owned();
        let generated = generate(vec![stmt], Opts::default());
        assert!(
            generated.has("cake.rs"),
            "{table_name:?} should resolve to the `cake` table"
        );
        assert_contains(generated.file("cake.rs"), r#"table_name = "cake""#);
    }
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    a column with no
// `ColumnType` is a `TransformError` naming the table and the column
#[test]
fn transform_rejects_a_column_without_column_type() {
    let untyped = Table::create(Alias::new("cake"))
        .col(ColumnDef::new(Alias::new("id")))
        .to_owned();

    assert_transform_error(
        vec![untyped],
        "table `cake` column `id`: column type should not be empty",
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    a primary-key index naming
// a column the table does not have is a `TransformError`
#[test]
fn transform_rejects_primary_key_over_unknown_column() {
    let mismatched = Table::create(Alias::new("cake"))
        .col(
            ColumnDef::new(Alias::new("id"))
                .integer()
                .not_null()
                .to_owned(),
        )
        .primary_key(&mut Index::create(
            Alias::new("cake"),
            Alias::new("missing"),
        ))
        .to_owned();

    assert_transform_error(
        vec![mismatched],
        "table `cake`: primary key column `missing` is not a column of the table",
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    a DB name with no Rust
// identifier form is a `TransformError` naming what it came from
#[test]
fn transform_rejects_names_without_identifier_form() {
    assert_transform_error(
        vec![table_with(
            "cake",
            vec![serial_pk("id"), typed("1", ColumnType::Integer)],
        )],
        "table `cake` column `1`: `1` is not a valid Rust identifier",
    );
    assert_transform_error(
        vec![table_with("-", vec![serial_pk("id")])],
        "table `-`: `` is not a valid Rust identifier",
    );
    assert_transform_error(
        vec![table_with(
            "cake",
            vec![serial_pk("id"), enum_col("tea", "tea", &["€"])],
        )],
        "enum `tea` value `€`: `€` is not a valid Rust identifier",
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    a relation onto a table the
// schema does not define, or onto a column either end does not have, is a
// `TransformError` naming the table, the relation and the column
#[test]
fn transform_rejects_relations_it_cannot_resolve() {
    // the partial dump: `orders` references a `customers` nobody passed
    assert_transform_error(
        vec![fk("orders", "customer_id", "customers", "id")],
        "table `orders`: relation to `customers` names a table the schema does not define",
    );

    assert_transform_error(
        vec![
            bare("customers"),
            fk("orders", "customer_id", "customers", "code"),
        ],
        "table `orders`: relation to `customers` references column `code`, which `customers` does \
         not have",
    );

    assert_transform_error(
        vec![
            bare("customers"),
            Table::create(Alias::new("orders"))
                .col(serial_pk("id"))
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("orders"),
                    Alias::new("customer_id"),
                    Alias::new("customers"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        "table `orders`: relation to `customers` constrains column `customer_id`, which the table \
         does not have",
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    `auto_increment`,
// `not_null` and `unique` come from the matching `ColumnSpec`
#[test]
fn transform_reads_column_specs_off_the_column_definition() {
    let generated = generate(
        vec![table_with(
            "cake",
            vec![
                // auto_increment + not_null + primary key
                serial_pk("id"),
                // a plain nullable column carries neither
                ColumnDef::new(Alias::new("baked_at"))
                    .timestamp()
                    .to_owned(),
                // not_null without auto_increment
                ColumnDef::new(Alias::new("name"))
                    .text()
                    .not_null()
                    .to_owned(),
            ],
        )],
        expanded(),
    );
    let cake = generated.file("cake.rs");

    // not_null decides the `Option<..>` wrapping and the trailing `.null()`
    assert_contains(cake, "pub id: i32,");
    assert_contains(cake, "pub baked_at: Option<DateTime>,");
    assert_contains(cake, "Self::BakedAt => ColumnType::Timestamp.def().null(),");
    assert_contains(cake, "Self::Name => ColumnType::Text.def(),");
    // auto_increment
    assert_contains(cake, "fn auto_increment() -> bool { true }");

    // `unique` likewise comes off `ColumnSpec::UniqueKey` when a `ColumnDef` is
    // converted into a codegen `Column`
    let unique = Column::try_from(
        &ColumnDef::new(Alias::new("email"))
            .string()
            .not_null()
            .unique_key()
            .to_owned(),
    )
    .expect("a typed column def should convert");
    assert_eq!(
        norm(&unique.get_def().to_string()),
        norm("ColumnType::String(StringLen::None).def().unique()")
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    a single-column unique index
// over exactly that column also marks it unique
#[test]
fn transform_marks_columns_from_single_column_unique_index() {
    let generated = generate(
        vec![
            Table::create(Alias::new("vendor"))
                .col(serial_pk("id"))
                .col(
                    ColumnDef::new(Alias::new("name"))
                        .string()
                        .not_null()
                        .to_owned(),
                )
                .col(
                    ColumnDef::new(Alias::new("region"))
                        .string()
                        .not_null()
                        .to_owned(),
                )
                .col(
                    ColumnDef::new(Alias::new("tier"))
                        .string()
                        .not_null()
                        .to_owned(),
                )
                .index(&mut unique_index("vendor", "name"))
                // a multi-column unique index marks nothing
                .index(
                    &mut Index::create(Alias::new("vendor"), Alias::new("region"))
                        .name("idx_vendor_region_tier")
                        .col(Alias::new("tier"))
                        .unique()
                        .to_owned(),
                )
                .to_owned(),
        ],
        Opts::default(),
    );
    let vendor = generated.file("vendor.rs");

    assert_contains(vendor, "#[pgorm(unique)] pub name: String,");
    assert_contains(vendor, "pub region: String,");
    assert_not_contains(vendor, "#[pgorm(unique)] pub region: String,");
    assert_not_contains(vendor, "#[pgorm(unique)] pub tier: String,");
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    primary keys come from
// `ColumnSpec::PrimaryKey` and are extended by a table-level primary-key index
#[test]
fn transform_collects_pks_from_specs_and_table_indexes() {
    let by_spec = generate(vec![table_with("cake", vec![serial_pk("id")])], expanded());
    assert_contains(by_spec.file("cake.rs"), "pub enum PrimaryKey { Id, }");

    let by_index = generate(
        vec![
            Table::create(Alias::new("cake_filling"))
                .col(
                    ColumnDef::new(Alias::new("cake_id"))
                        .integer()
                        .not_null()
                        .to_owned(),
                )
                .col(
                    ColumnDef::new(Alias::new("filling_id"))
                        .integer()
                        .not_null()
                        .to_owned(),
                )
                .primary_key(
                    Index::create(Alias::new("cake_filling"), Alias::new("cake_id"))
                        .col(Alias::new("filling_id")),
                )
                .to_owned(),
        ],
        expanded(),
    );
    assert_contains(
        by_index.file("cake_filling.rs"),
        "pub enum PrimaryKey { CakeId, FillingId, }",
    );
    assert_contains(
        by_index.file("cake_filling.rs"),
        "type ValueType = (i32, i32);",
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    every enum column registers
// an `ActiveEnum` keyed by enum name, deduplicated across tables and looked
// through `Array`
#[test]
fn transform_registers_enums_once_per_name_across_tables() {
    let tea = || ColumnType::Enum {
        name: Alias::new("tea").into_iden(),
        variants: vec![
            Alias::new("EverydayTea").into_iden(),
            Alias::new("BreakfastTea").into_iden(),
        ],
    };

    let generated = generate(
        vec![
            table_with(
                "cake",
                vec![
                    serial_pk("id"),
                    enum_col("tea", "tea", &["EverydayTea", "BreakfastTea"]),
                    enum_col("mood", "mood", &["Happy", "Sad"]),
                ],
            ),
            // the same `tea` enum again, this time as an array element type
            table_with(
                "biscuit",
                vec![
                    serial_pk("id"),
                    ColumnDef::new(Alias::new("teas"))
                        .array(tea())
                        .not_null()
                        .to_owned(),
                ],
            ),
        ],
        Opts::default(),
    );

    let enums = generated.file("pgorm_active_enums.rs");
    assert_eq!(
        enums.matches("pub enum Tea").count(),
        1,
        "the `tea` enum should be registered once: {enums}"
    );
    // BTreeMap ordering: `mood` before `tea`
    assert!(position_of(enums, "pub enum Mood") < position_of(enums, "pub enum Tea"));
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    foreign keys become
// `BelongsTo` relations that keep their columns, referenced columns and
// on_update / on_delete actions
#[test]
fn transform_turns_foreign_keys_into_belongs_to_relations() {
    let generated = generate(
        vec![
            table_with(
                "cake",
                vec![
                    ColumnDef::new(Alias::new("id"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                    ColumnDef::new(Alias::new("kind"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                ],
            ),
            Table::create(Alias::new("fruit"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("cake_kind")).integer().to_owned())
                .foreign_key(
                    ForeignKey::create(
                        Alias::new("fruit"),
                        Alias::new("cake_id"),
                        Alias::new("cake"),
                        Alias::new("id"),
                    )
                    .col(Alias::new("cake_kind"), Alias::new("kind"))
                    .on_update(ForeignKeyAction::Cascade)
                    .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("fruit.rs"),
        r#"#[pgorm(
            belongs_to = "super::cake::Entity",
            from = "(Column::CakeId, Column::CakeKind)",
            to = "(super::cake::Column::Id, super::cake::Column::Kind)",
            on_update = "Cascade",
            on_delete = "SetNull",
        )]"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    a relation onto its own
// table is flagged self-referencing
#[test]
fn transform_flags_self_referencing_relations() {
    let generated = generate(
        vec![fk("users", "manager_id", "users", "id")],
        Opts::default(),
    );

    assert_contains(
        generated.file("users.rs"),
        r#"#[pgorm(belongs_to = "Entity", from = "Column::ManagerId", to = "Column::Id",)] SelfRef,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    several FKs onto the same
// target take 1-based `num_suffix`es in declaration order; a lone FK keeps 0
#[test]
fn transform_numbers_repeated_fks_to_same_table() {
    let generated = generate(
        vec![
            bare("fruit"),
            bare("cake"),
            Table::create(Alias::new("basket"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("fruit_id1")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("fruit_id2")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("basket"),
                    Alias::new("fruit_id1"),
                    Alias::new("fruit"),
                    Alias::new("id"),
                ))
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("basket"),
                    Alias::new("fruit_id2"),
                    Alias::new("fruit"),
                    Alias::new("id"),
                ))
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("basket"),
                    Alias::new("cake_id"),
                    Alias::new("cake"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );
    let basket = generated.file("basket.rs");

    // both FKs to `fruit` take a distinct 1-based suffix. NB the numbering runs
    // in reverse declaration order, so `fruit_id1` lands on `Fruit2`.
    assert_contains(
        basket,
        r#"#[pgorm(
            belongs_to = "super::fruit::Entity",
            from = "Column::FruitId1",
            to = "super::fruit::Column::Id",
        )]
        Fruit2,"#,
    );
    assert_contains(
        basket,
        r#"#[pgorm(
            belongs_to = "super::fruit::Entity",
            from = "Column::FruitId2",
            to = "super::fruit::Column::Id",
        )]
        Fruit1,"#,
    );
    // the single FK to `cake` keeps suffix 0
    assert_contains(
        basket,
        r#"#[pgorm(
            belongs_to = "super::cake::Entity",
            from = "Column::CakeId",
            to = "super::cake::Column::Id",
        )]
        Cake,"#,
    );
    assert_not_contains(basket, "Cake1,");
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]    relations are sorted by
// referenced table name and conjunct relations by target name
#[test]
fn transform_sorts_relations_and_conjunct_relations() {
    let generated = generate(cake_schema(), Opts::default());
    let cake = generated.file("cake.rs");

    // `cake_filling` sorts before `fruit`
    assert!(position_of(cake, "CakeFilling,") < position_of(cake, "Fruit,"));

    // `users` sits between two junction targets, so its conjunct relations sort
    let generated = generate(
        vec![
            bare("users"),
            bare("apple"),
            bare("zebra"),
            junction("users_apples", ("users", "user_id"), ("apple", "apple_id")),
            junction("users_zebras", ("users", "user_id"), ("zebra", "zebra_id")),
        ],
        Opts::default(),
    );
    let users = generated.file("users.rs");
    assert!(
        position_of(users, "impl Related<super::apple::Entity> for Entity")
            < position_of(users, "impl Related<super::zebra::Entity> for Entity")
    );
}

fn junction(name: &str, left: (&str, &str), right: (&str, &str)) -> TableCreateStatement {
    Table::create(Alias::new(name))
        .col(
            ColumnDef::new(Alias::new(left.1))
                .integer()
                .not_null()
                .primary_key()
                .to_owned(),
        )
        .col(
            ColumnDef::new(Alias::new(right.1))
                .integer()
                .not_null()
                .primary_key()
                .to_owned(),
        )
        .foreign_key(&mut ForeignKey::create(
            Alias::new(name),
            Alias::new(left.1),
            Alias::new(left.0),
            Alias::new("id"),
        ))
        .foreign_key(&mut ForeignKey::create(
            Alias::new(name),
            Alias::new(right.1),
            Alias::new(right.0),
            Alias::new("id"),
        ))
        .to_owned()
}

// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    a non-unique FK
// gives the referenced entity a `HasMany` back-reference
#[test]
fn inverse_has_many_for_non_unique_foreign_key() {
    let generated = generate(vec![cake(), fruit()], Opts::default());

    assert_contains(
        generated.file("cake.rs"),
        r#"#[pgorm(has_many = "super::fruit::Entity")] Fruit,"#,
    );
    // the inverse carries no from/to
    assert_not_contains(generated.file("cake.rs"), r#"from = "Column::CakeId""#);
}

// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    a FK whose every
// column is unique in the owning table inverts to `HasOne`
#[test]
fn inverse_has_one_for_unique_foreign_key() {
    let generated = generate(
        vec![
            cake(),
            Table::create(Alias::new("fruit"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .index(&mut unique_index("fruit", "cake_id"))
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("fruit"),
                    Alias::new("cake_id"),
                    Alias::new("cake"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("cake.rs"),
        r#"#[pgorm(has_one = "super::fruit::Entity")] Fruit,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    a FK whose column
// set is exactly the owning table's primary key also inverts to `HasOne`
#[test]
fn inverse_has_one_for_whole_primary_key_fk() {
    let generated = generate(
        vec![
            bare("users"),
            Table::create(Alias::new("profile"))
                .col(
                    ColumnDef::new(Alias::new("user_id"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("profile"),
                    Alias::new("user_id"),
                    Alias::new("users"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("users.rs"),
        r#"#[pgorm(has_one = "super::profile::Entity")] Profile,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    a unique constraint
// over exactly the FK's columns constrains the key as a whole, which reading one
// column at a time cannot see: the inverse is `HasOne`
#[test]
fn inverse_has_one_for_composite_unique_foreign_key() {
    let cake = || {
        table_with(
            "cake",
            vec![
                ColumnDef::new(Alias::new("id"))
                    .integer()
                    .not_null()
                    .primary_key()
                    .to_owned(),
                ColumnDef::new(Alias::new("kind"))
                    .integer()
                    .not_null()
                    .primary_key()
                    .to_owned(),
            ],
        )
    };
    let cake_key = |table: &'static str| {
        ForeignKey::create(
            Alias::new(table),
            Alias::new("cake_id"),
            Alias::new("cake"),
            Alias::new("id"),
        )
        .col(Alias::new("cake_kind"), Alias::new("kind"))
        .to_owned()
    };

    let generated = generate(
        vec![
            cake(),
            Table::create(Alias::new("fruit"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("cake_kind")).integer().to_owned())
                .index(
                    &mut Index::create(Alias::new("fruit"), Alias::new("cake_id"))
                        .name("idx_fruit_cake")
                        .col(Alias::new("cake_kind"))
                        .unique()
                        .to_owned(),
                )
                .foreign_key(&mut cake_key("fruit"))
                .to_owned(),
            // the same key under a unique index covering more than the key
            // constrains nothing about the key
            Table::create(Alias::new("crumb"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("cake_kind")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("batch")).integer().to_owned())
                .index(
                    &mut Index::create(Alias::new("crumb"), Alias::new("cake_id"))
                        .name("idx_crumb_cake_batch")
                        .col(Alias::new("cake_kind"))
                        .col(Alias::new("batch"))
                        .unique()
                        .to_owned(),
                )
                .foreign_key(&mut cake_key("crumb"))
                .to_owned(),
        ],
        Opts::default(),
    );
    let cake_file = generated.file("cake.rs");

    assert_contains(
        cake_file,
        r#"#[pgorm(has_one = "super::fruit::Entity")] Fruit,"#,
    );
    assert_contains(
        cake_file,
        r#"#[pgorm(has_many = "super::crumb::Entity")] Crumb,"#,
    );
    // neither column is unique on its own
    assert_not_contains(generated.file("fruit.rs"), "#[pgorm(unique)] pub cake_id");
}

// [spec:pgorm:sem:codegen.entity.transform+6/test]
// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    a `UniqueKey` spec
// on the column definition marks the column unique on this path too, so its FK
// inverts to `HasOne`
#[test]
fn inverse_has_one_for_inline_unique_key_column() {
    let generated = generate(
        vec![
            cake(),
            Table::create(Alias::new("fruit"))
                .col(serial_pk("id"))
                .col(
                    ColumnDef::new(Alias::new("cake_id"))
                        .integer()
                        .unique_key()
                        .to_owned(),
                )
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("fruit"),
                    Alias::new("cake_id"),
                    Alias::new("cake"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("fruit.rs"),
        "#[pgorm(unique)] pub cake_id: Option<i32>,",
    );
    assert_contains(
        generated.file("cake.rs"),
        r#"#[pgorm(has_one = "super::fruit::Entity")] Fruit,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    self-referencing and
// suffixed relations produce no inverse
#[test]
fn no_inverse_for_self_referencing_or_suffixed_relations() {
    let self_ref = generate(
        vec![fk("users", "manager_id", "users", "id")],
        Opts::default(),
    );
    let users = self_ref.file("users.rs");
    // exactly the one `SelfRef` variant, no synthesised back-reference
    assert_contains(users, "SelfRef,");
    assert_not_contains(users, r#"has_many = "Entity""#);
    assert_not_contains(users, r#"has_one = "Entity""#);

    let suffixed = generate(
        vec![
            bare("fruit"),
            Table::create(Alias::new("basket"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("fruit_id1")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("fruit_id2")).integer().to_owned())
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("basket"),
                    Alias::new("fruit_id1"),
                    Alias::new("fruit"),
                    Alias::new("id"),
                ))
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("basket"),
                    Alias::new("fruit_id2"),
                    Alias::new("fruit"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );
    assert_contains(suffixed.file("fruit.rs"), "pub enum Relation { }");
}

// [spec:pgorm:sem:codegen.entity.transform.inverse+1/test]    an inverse relation
// is dropped when the target entity already relates to that table
#[test]
fn inverse_dropped_when_target_already_relates_back() {
    let generated = generate(
        vec![
            Table::create(Alias::new("users"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("bill_id")).integer().to_owned())
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("users"),
                    Alias::new("bill_id"),
                    Alias::new("bills"),
                    Alias::new("id"),
                ))
                .to_owned(),
            Table::create(Alias::new("bills"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("user_id")).integer().to_owned())
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("bills"),
                    Alias::new("user_id"),
                    Alias::new("users"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );

    // each side keeps only its own belongs_to; neither gains a has_many
    for (file, target) in [
        ("users.rs", "super::bills::Entity"),
        ("bills.rs", "super::users::Entity"),
    ] {
        let content = generated.file(file);
        assert_contains(content, &format!(r#"belongs_to = "{target}""#));
        assert_not_contains(content, &format!(r#"has_many = "{target}""#));
    }
}

// [spec:pgorm:sem:codegen.entity.transform.conjunct+1/test]    a table with two
// relations and two primary-key columns is a junction: both referenced entities
// gain a `ConjunctRelation`
#[test]
fn junction_table_gives_both_sides_a_conjunct_relation() {
    let generated = generate(cake_schema(), Opts::default());

    assert_contains(
        generated.file("cake.rs"),
        "impl Related<super::filling::Entity> for Entity {
            fn to() -> RelationDef { super::cake_filling::Relation::Filling.def() }
            fn via() -> Option<RelationDef> {
                Some(super::cake_filling::Relation::Cake.def().rev())
            }
        }",
    );
    assert_contains(
        generated.file("filling.rs"),
        "impl Related<super::cake::Entity> for Entity {
            fn to() -> RelationDef { super::cake_filling::Relation::Cake.def() }
            fn via() -> Option<RelationDef> {
                Some(super::cake_filling::Relation::Filling.def().rev())
            }
        }",
    );
}

// [spec:pgorm:sem:codegen.entity.transform.conjunct+1/test]    two foreign keys
// that are not the table's primary key join nothing: a table keyed by something
// of its own is a table with two references, not a junction
#[test]
fn fks_outside_the_primary_key_are_not_junctions() {
    let generated = generate(
        vec![
            bare("orgs"),
            bare("users"),
            Table::create(Alias::new("audit_events"))
                .col(
                    ColumnDef::new(Alias::new("id"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                .col(
                    ColumnDef::new(Alias::new("version"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                .col(ColumnDef::new(Alias::new("user_id")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("org_id")).integer().to_owned())
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("audit_events"),
                    Alias::new("user_id"),
                    Alias::new("users"),
                    Alias::new("id"),
                ))
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("audit_events"),
                    Alias::new("org_id"),
                    Alias::new("orgs"),
                    Alias::new("id"),
                ))
                .to_owned(),
        ],
        Opts::default(),
    );

    for (file, other) in [("users.rs", "orgs"), ("orgs.rs", "users")] {
        let content = generated.file(file);
        assert_contains(
            content,
            r#"#[pgorm(has_many = "super::audit_events::Entity")] AuditEvents,"#,
        );
        assert_not_contains(content, "fn via() -> Option<RelationDef>");
        assert_not_contains(
            content,
            &format!("impl Related<super::{other}::Entity> for Entity"),
        );
    }
}

// [spec:pgorm:sem:codegen.entity.transform.conjunct+1/test]    only the table's
// own foreign keys are junction legs: a synthesised inverse neither makes a
// junction nor stops one
#[test]
fn inbound_relations_are_not_junction_legs() {
    // `posts` has one foreign key and a two-column primary key; the `comments`
    // back-reference is `comments`' key, not a second leg of its own
    let generated = generate(
        vec![
            bare("tenants"),
            Table::create(Alias::new("posts"))
                .col(
                    ColumnDef::new(Alias::new("tenant_id"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                .col(
                    ColumnDef::new(Alias::new("id"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                .foreign_key(&mut ForeignKey::create(
                    Alias::new("posts"),
                    Alias::new("tenant_id"),
                    Alias::new("tenants"),
                    Alias::new("id"),
                ))
                .to_owned(),
            fk("comments", "post_id", "posts", "id"),
        ],
        Opts::default(),
    );

    assert_not_contains(
        generated.file("tenants.rs"),
        "fn via() -> Option<RelationDef>",
    );
    assert_not_contains(
        generated.file("tenants.rs"),
        "impl Related<super::comments::Entity> for Entity",
    );
    assert_not_contains(
        generated.file("comments.rs"),
        "fn via() -> Option<RelationDef>",
    );

    // and the junction below keeps its many-to-many despite the inbound
    // relation `vote_flags` gives it
    let generated = generate(
        vec![
            bare("users"),
            bare("bills"),
            junction("users_votes", ("users", "user_id"), ("bills", "bill_id")),
            fk("vote_flags", "user_id", "users_votes", "user_id"),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("users.rs"),
        "impl Related<super::bills::Entity> for Entity {
            fn to() -> RelationDef { super::users_votes::Relation::Bills.def() }
            fn via() -> Option<RelationDef> {
                Some(super::users_votes::Relation::Users.def().rev())
            }
        }",
    );
}

// [spec:pgorm:sem:codegen.entity.transform.conjunct+1/test]    duplicated
// many-to-many paths to the same target are all removed
#[test]
fn duplicated_many_to_many_paths_generate_no_conjunct() {
    let generated = generate(
        vec![
            bare("users"),
            bare("bills"),
            junction("users_votes", ("users", "user_id"), ("bills", "bill_id")),
            junction(
                "users_saved_bills",
                ("users", "user_id"),
                ("bills", "bill_id"),
            ),
        ],
        Opts::default(),
    );

    // both sides keep the junction relations but neither gets a `via` impl
    assert_not_contains(
        generated.file("users.rs"),
        "fn via() -> Option<RelationDef>",
    );
    assert_not_contains(
        generated.file("bills.rs"),
        "fn via() -> Option<RelationDef>",
    );
    assert_contains(
        generated.file("users.rs"),
        r#"#[pgorm(has_many = "super::users_votes::Entity")] UsersVotes,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform.conjunct+1/test]    when a conjunct
// relation targets the same table as an ordinary relation, the ordinary
// relation's `Related` impl is suppressed in favour of the `via` one
#[test]
fn conjunct_relation_suppresses_the_plain_related_impl() {
    let generated = generate(
        vec![
            bare("users"),
            // `bills` also has a direct FK to `users`, so `users` ends up with
            // both an ordinary relation and a conjunct relation to `bills`
            fk("bills", "user_id", "users", "id"),
            junction("users_votes", ("users", "user_id"), ("bills", "bill_id")),
        ],
        Opts::default(),
    );
    let users = generated.file("users.rs");

    // the Relation variant survives ...
    assert_contains(
        users,
        r#"#[pgorm(has_many = "super::bills::Entity")] Bills,"#,
    );
    // ... but the plain `Related` impl is replaced by the conjunct one
    assert_not_contains(
        users,
        "impl Related<super::bills::Entity> for Entity {
            fn to() -> RelationDef { Relation::Bills.def() }
        }",
    );
    assert_contains(
        users,
        "impl Related<super::bills::Entity> for Entity {
            fn to() -> RelationDef { super::users_votes::Relation::Bills.def() }
            fn via() -> Option<RelationDef> {
                Some(super::users_votes::Relation::Users.def().rev())
            }
        }",
    );
}
