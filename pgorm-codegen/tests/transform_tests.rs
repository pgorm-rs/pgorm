//! Schema discovery -> `Entity` model: `EntityTransformer::transform`, the
//! inverse relations it synthesises, and the many-to-many conjunct relations it
//! derives from junction tables.

mod common;

use common::*;
use pgorm_codegen::{Column, EntityTransformer, Error};
use pgorm_query::{
    Alias, ColumnDef, ColumnType, ForeignKey, ForeignKeyAction, Index, IntoIden, Table,
    TableCreateStatement, TableRef,
};

fn fk(from_table: &str, from_col: &str, to_table: &str, to_col: &str) -> TableCreateStatement {
    Table::create()
        .table(Alias::new(from_table))
        .col(serial_pk("id"))
        .col(ColumnDef::new(Alias::new(from_col)).integer().to_owned())
        .foreign_key(
            ForeignKey::create()
                .from(Alias::new(from_table), Alias::new(from_col))
                .to(Alias::new(to_table), Alias::new(to_col)),
        )
        .to_owned()
}

fn bare(table: &str) -> TableCreateStatement {
    table_with(table, vec![serial_pk("id")])
}

// [spec:pgorm:sem:codegen.entity.transform+1/test]    one Entity per input
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    the table name is unpacked
// from every `TableRef` variant that carries a table iden
#[test]
fn transform_unpacks_table_name_from_every_table_ref() {
    let cake = || Alias::new("cake").into_iden();
    let schema = || Alias::new("public").into_iden();
    let db = || Alias::new("app").into_iden();
    let alias = || Alias::new("c").into_iden();

    let refs = [
        TableRef::Table(cake()),
        TableRef::SchemaTable(schema(), cake()),
        TableRef::DatabaseSchemaTable(db(), schema(), cake()),
        TableRef::TableAlias(cake(), alias()),
        TableRef::SchemaTableAlias(schema(), cake(), alias()),
        TableRef::DatabaseSchemaTableAlias(db(), schema(), cake(), alias()),
    ];

    for table_ref in refs {
        let stmt = Table::create()
            .table(table_ref.clone())
            .col(serial_pk("id"))
            .to_owned();
        let generated = generate(vec![stmt], Opts::default());
        assert!(
            generated.has("cake.rs"),
            "{table_ref:?} should resolve to the `cake` table"
        );
        assert_contains(generated.file("cake.rs"), r#"table_name = "cake""#);
    }
}

// [spec:pgorm:sem:codegen.entity.transform+1/test]    a statement with no table
// name is a `TransformError`
#[test]
fn transform_rejects_a_statement_without_a_table_name() {
    let err = EntityTransformer::transform(vec![Table::create().col(serial_pk("id")).to_owned()])
        .expect_err("a nameless table should not transform");
    match err {
        Error::TransformError(msg) => assert_eq!(msg, "Table name should not be empty"),
        other => panic!("expected a TransformError, got {other:?}"),
    }
}

// [spec:pgorm:sem:codegen.entity.transform+1/test]    a column with no
// `ColumnType` panics
#[test]
#[should_panic(expected = "ColumnType should not be empty")]
fn transform_panics_on_column_without_column_type() {
    let untyped = Table::create()
        .table(Alias::new("cake"))
        .col(ColumnDef::new(Alias::new("id")))
        .to_owned();
    let _ = EntityTransformer::transform(vec![untyped]);
}

// [spec:pgorm:sem:codegen.entity.transform+1/test]    `auto_increment`,
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
        Opts::expanded(),
    );
    let cake = generated.file("cake.rs");

    // not_null decides the `Option<..>` wrapping and the trailing `.null()`
    assert_contains(cake, "pub id: i32,");
    assert_contains(cake, "pub baked_at: Option<DateTimeUtc>,");
    assert_contains(cake, "Self::BakedAt => ColumnType::Timestamp.def().null(),");
    assert_contains(cake, "Self::Name => ColumnType::Text.def(),");
    // auto_increment
    assert_contains(cake, "fn auto_increment() -> bool { true }");

    // `unique` likewise comes off `ColumnSpec::UniqueKey` when a `ColumnDef` is
    // converted into a codegen `Column`
    let unique: Column = (&ColumnDef::new(Alias::new("email"))
        .string()
        .not_null()
        .unique_key()
        .to_owned())
        .into();
    assert_eq!(
        norm(&unique.get_def().to_string()),
        norm("ColumnType::String(StringLen::None).def().unique()")
    );
}

// [spec:pgorm:sem:codegen.entity.transform+1/test]    a single-column unique index
// over exactly that column also marks it unique
#[test]
fn transform_marks_columns_from_single_column_unique_index() {
    let generated = generate(
        vec![
            Table::create()
                .table(Alias::new("vendor"))
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
                    &mut Index::create()
                        .name("idx_vendor_region_tier")
                        .table(Alias::new("vendor"))
                        .col(Alias::new("region"))
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    primary keys come from
// `ColumnSpec::PrimaryKey` and are extended by a table-level primary-key index
#[test]
fn transform_collects_pks_from_specs_and_table_indexes() {
    let by_spec = generate(
        vec![table_with("cake", vec![serial_pk("id")])],
        Opts::expanded(),
    );
    assert_contains(by_spec.file("cake.rs"), "pub enum PrimaryKey { Id, }");

    let by_index = generate(
        vec![
            Table::create()
                .table(Alias::new("cake_filling"))
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
                    Index::create()
                        .col(Alias::new("cake_id"))
                        .col(Alias::new("filling_id")),
                )
                .to_owned(),
        ],
        Opts::expanded(),
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    every enum column registers
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    foreign keys become
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
            Table::create()
                .table(Alias::new("fruit"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("cake_kind")).integer().to_owned())
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            Alias::new("fruit"),
                            (Alias::new("cake_id"), Alias::new("cake_kind")),
                        )
                        .to(Alias::new("cake"), (Alias::new("id"), Alias::new("kind")))
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    a relation onto its own
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    several FKs onto the same
// target take 1-based `num_suffix`es in declaration order; a lone FK keeps 0
#[test]
fn transform_numbers_repeated_fks_to_same_table() {
    let generated = generate(
        vec![
            bare("fruit"),
            bare("cake"),
            Table::create()
                .table(Alias::new("basket"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("fruit_id1")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("fruit_id2")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("basket"), Alias::new("fruit_id1"))
                        .to(Alias::new("fruit"), Alias::new("id")),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("basket"), Alias::new("fruit_id2"))
                        .to(Alias::new("fruit"), Alias::new("id")),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("basket"), Alias::new("cake_id"))
                        .to(Alias::new("cake"), Alias::new("id")),
                )
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

// [spec:pgorm:sem:codegen.entity.transform+1/test]    relations are sorted by
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
    Table::create()
        .table(Alias::new(name))
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
        .foreign_key(
            ForeignKey::create()
                .from(Alias::new(name), Alias::new(left.1))
                .to(Alias::new(left.0), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Alias::new(name), Alias::new(right.1))
                .to(Alias::new(right.0), Alias::new("id")),
        )
        .to_owned()
}

// [spec:pgorm:sem:codegen.entity.transform.inverse/test]    a non-unique FK
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

// [spec:pgorm:sem:codegen.entity.transform.inverse/test]    a FK whose every
// column is unique in the owning table inverts to `HasOne`
#[test]
fn inverse_has_one_for_unique_foreign_key() {
    let generated = generate(
        vec![
            cake(),
            Table::create()
                .table(Alias::new("fruit"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("cake_id")).integer().to_owned())
                .index(&mut unique_index("fruit", "cake_id"))
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("fruit"), Alias::new("cake_id"))
                        .to(Alias::new("cake"), Alias::new("id")),
                )
                .to_owned(),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("cake.rs"),
        r#"#[pgorm(has_one = "super::fruit::Entity")] Fruit,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform.inverse/test]    a FK whose column
// set is exactly the owning table's primary key also inverts to `HasOne`
#[test]
fn inverse_has_one_for_whole_primary_key_fk() {
    let generated = generate(
        vec![
            bare("users"),
            Table::create()
                .table(Alias::new("profile"))
                .col(
                    ColumnDef::new(Alias::new("user_id"))
                        .integer()
                        .not_null()
                        .primary_key()
                        .to_owned(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("profile"), Alias::new("user_id"))
                        .to(Alias::new("users"), Alias::new("id")),
                )
                .to_owned(),
        ],
        Opts::default(),
    );

    assert_contains(
        generated.file("users.rs"),
        r#"#[pgorm(has_one = "super::profile::Entity")] Profile,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.transform.inverse/test]    self-referencing and
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
            Table::create()
                .table(Alias::new("basket"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("fruit_id1")).integer().to_owned())
                .col(ColumnDef::new(Alias::new("fruit_id2")).integer().to_owned())
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("basket"), Alias::new("fruit_id1"))
                        .to(Alias::new("fruit"), Alias::new("id")),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("basket"), Alias::new("fruit_id2"))
                        .to(Alias::new("fruit"), Alias::new("id")),
                )
                .to_owned(),
        ],
        Opts::default(),
    );
    assert_contains(suffixed.file("fruit.rs"), "pub enum Relation { }");
}

// [spec:pgorm:sem:codegen.entity.transform.inverse/test]    an inverse relation
// is dropped when the target entity already relates to that table
#[test]
fn inverse_dropped_when_target_already_relates_back() {
    let generated = generate(
        vec![
            Table::create()
                .table(Alias::new("users"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("bill_id")).integer().to_owned())
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("users"), Alias::new("bill_id"))
                        .to(Alias::new("bills"), Alias::new("id")),
                )
                .to_owned(),
            Table::create()
                .table(Alias::new("bills"))
                .col(serial_pk("id"))
                .col(ColumnDef::new(Alias::new("user_id")).integer().to_owned())
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new("bills"), Alias::new("user_id"))
                        .to(Alias::new("users"), Alias::new("id")),
                )
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

// [spec:pgorm:sem:codegen.entity.transform.conjunct/test]    a table with two
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

// [spec:pgorm:sem:codegen.entity.transform.conjunct/test]    duplicated
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

// [spec:pgorm:sem:codegen.entity.transform.conjunct/test]    when a conjunct
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
