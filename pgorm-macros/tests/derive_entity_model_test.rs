//! Verification for `DeriveEntityModel` and the pieces it composes: the `Column`
//! enum and its `ColumnTrait` impl, the `Entity` unit struct and its `EntityName`
//! impl, and the `PrimaryKey` enum and its `PrimaryKeyTrait` impl.

#![allow(dead_code, non_snake_case)]

use pgorm::entity::prelude::*;
use pgorm::pgorm_query::{Alias, ColumnType, Expr, StringLen};
use pgorm::{Iterable, PrimaryKeyToColumn};

/// The kitchen-sink entity: every struct-level and field-level attribute the
/// entity-model derive recognises appears here at least once.
mod filling {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(
        table_name = "filling",
        schema_name = "baking",
        comment = "cake fillings",
        table_iden
    )]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        #[pgorm(column_type = "Text", unique, indexed, comment = "display name")]
        pub name: String,
        pub vendor_id: Option<i32>,
        #[pgorm(nullable, default_value = 0)]
        pub weight: i32,
        #[pgorm(default_expr = "Expr::current_date()")]
        pub baked_on: Date,
        #[pgorm(column_name = "SKU")]
        pub sku: String,
        #[pgorm(enum_name = "Renamed")]
        pub original: i32,
        #[pgorm(select_as = "text", save_as = "citext")]
        pub casted: String,
        #[pgorm(ignore)]
        pub not_a_column: i32,
        #[pgorm(no_such_field_key = "silently skipped")]
        pub plain: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// Every branch of the Rust-type -> `ColumnType` inference table that a `Model`
/// field can actually hold. `char` and `u64` are omitted because neither
/// implements `TryGetable`, so the `FromQueryResult` impl that
/// `DeriveEntityModel` re-runs would not compile; they are covered through
/// `DeriveValueType`, which shares the same table, in `derive_value_type_test`.
mod scalars {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "scalars")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub txt: String,
        pub tiny: i8,
        pub small: i16,
        pub big: i64,
        pub uns: u32,
        pub single: f32,
        pub dbl: f64,
        pub flag: bool,
        pub d: Date,
        pub t: Time,
        pub dt: DateTime,
        pub dtu: DateTimeUtc,
        pub dtl: DateTimeLocal,
        pub dttz: DateTimeWithTimeZone,
        pub uid: Uuid,
        pub js: Json,
        pub dec: Decimal,
        pub blob: Vec<u8>,
        pub maybe: Option<i64>,
        pub tea: Tea,
    }

    #[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[pgorm(rs_type = "String", db_type = "String(StringLen::N(1))")]
    pub enum Tea {
        #[pgorm(string_value = "E")]
        EverydayTea,
        #[pgorm(string_value = "B")]
        BreakfastTea,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// Casing: raw-identifier trimming, `enum_name` substitution, keyword escaping,
/// and the conditional pinning of the SQL column name.
mod casing {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "casing")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        // Raw identifier: the leading `r#` is trimmed before UpperCamelCase.
        pub r#type: i32,
        // Already-clean snake_case: no `column_name` attribute is pinned.
        pub first_name: i32,
        // Does not survive the UpperCamelCase -> snake_case round trip, so the
        // snake_case of the *original* field name is pinned instead.
        pub abc_1: i32,
        // Upper-cases to the special keyword `Self`, which is escaped by
        // appending an underscore.
        pub SELF: i32,
        // `enum_name` replaces the computed variant name entirely.
        #[pgorm(enum_name = "Wholly")]
        pub renamed_away: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// `rename_all` drives the SQL column names for every field that has no
/// explicit `column_name`.
mod renamed {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "renamed", rename_all = "SCREAMING_SNAKE_CASE")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub first_name: i32,
        #[pgorm(column_name = "explicit")]
        pub second_name: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// A single primary key with the default (true) auto-increment flag, plus a
/// `column_name` override on the key itself.
mod single_pk {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "single_pk")]
    pub struct Model {
        #[pgorm(primary_key, column_name = "ID")]
        pub id: i32,
        pub name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// A composite primary key: the `ValueType` becomes a tuple and
/// `auto_increment()` is false because there is more than one key column.
mod composite_pk {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "composite_pk")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub cake_id: i32,
        #[pgorm(primary_key)]
        pub filling_id: i32,
        pub qty: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// `auto_increment` is a single shared flag: setting it false on a field that
/// is not even a primary key flips it for the whole entity.
mod shared_auto_increment {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "shared_auto_increment")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        #[pgorm(auto_increment = false)]
        pub not_a_key: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// No `table_name`, so no `Entity` struct is generated: the entity module is
/// hand-finished and `DeriveEntityModel` contributes only `Column`,
/// `PrimaryKey`, `Model` and `ActiveModel`.
mod no_table_name {
    use pgorm::entity::prelude::*;

    #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
    pub struct Entity;

    impl EntityName for Entity {
        fn table_name(&self) -> &str {
            "hand_written"
        }
    }

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// [spec:pgorm:sem:macros.derive.entity-model/test]
// [spec:pgorm:syn:macros.derive.entity-model.attrs/test]    struct-level table_name / schema_name / comment
#[test]
fn struct_attributes_drive_entity_and_entity_name() {
    // (3) `table_name` present, so `pub struct Entity;` plus a hand-rolled
    //     `EntityName` carrying table_name / schema_name / comment.
    assert_eq!(filling::Entity.table_name(), "filling");
    assert_eq!(filling::Entity.schema_name(), Some("baking"));
    assert_eq!(filling::Entity.comment(), Some("cake fillings"));
    // Entity is `Copy, Clone, Default, Debug` and derives `DeriveEntity`.
    let entity = filling::Entity;
    let copied = entity;
    assert_eq!(format!("{:?}", copied), "Entity");
    assert_eq!(format!("{:?}", filling::Entity {}), "Entity");
}

// [spec:pgorm:sem:macros.derive.entity-model/test]    one derive yields the whole entity module
#[test]
fn one_derive_yields_the_whole_entity_module() {
    // (1) the `Column` enum, with `EnumIter` + `DeriveColumn` behaviour.
    let columns: Vec<String> = filling::Column::iter().map(|c| c.to_string()).collect();
    assert_eq!(
        columns,
        vec![
            "id",
            "name",
            "vendor_id",
            "weight",
            "baked_on",
            "SKU",
            "renamed",
            "casted",
            "plain",
        ]
    );

    // (2) `impl ColumnTrait for Column` with `type EntityName = Entity`.
    fn assert_entity_name<C: ColumnTrait<EntityName = filling::Entity>>(_: C) {}
    assert_entity_name(filling::Column::Id);

    // (4) the `PrimaryKey` enum.
    let keys: Vec<String> = filling::PrimaryKey::iter().map(|k| k.to_string()).collect();
    assert_eq!(keys, vec!["id"]);

    // DeriveModel + DeriveActiveModel are re-run on the same input, so
    // `Model: ModelTrait` and `ActiveModel` both exist.
    let model = filling::Model {
        id: 1,
        name: "raspberry".to_owned(),
        vendor_id: None,
        weight: 3,
        baked_on: Date::from_ymd_opt(2024, 1, 1).unwrap(),
        sku: "RB-1".to_owned(),
        original: 7,
        casted: "x".to_owned(),
        not_a_column: 99,
        plain: 0,
    };
    assert_eq!(model.get(filling::Column::Id), 1i32.into());
    let active: filling::ActiveModel = model.into();
    assert_eq!(active.id, pgorm::ActiveValue::unchanged(1));
}

// [spec:pgorm:sem:macros.derive.entity-model/test]    select_as / save_as overrides and their fallback
#[test]
fn select_as_and_save_as_cast_columns() {
    let casted = filling::Column::Casted;
    assert_eq!(
        casted.select_as(Expr::col(casted)),
        Expr::col(casted).cast_as(Alias::new("text"))
    );
    assert_eq!(
        casted.save_as(Expr::val("v")),
        Expr::val("v").cast_as(Alias::new("citext"))
    );

    // Columns without the attributes fall through to the trait defaults.
    let plain = filling::Column::Plain;
    assert_eq!(
        plain.select_as(Expr::col(plain)),
        ColumnTrait::select_enum_as(&plain, Expr::col(plain))
    );
    assert_eq!(
        plain.save_as(Expr::val(1)),
        ColumnTrait::save_enum_as(&plain, Expr::val(1))
    );
}

// [spec:pgorm:syn:macros.derive.entity-model.attrs/test]    `table_iden` adds a Table variant
#[test]
fn table_iden_variant_is_skipped_by_enum_iter() {
    // The variant exists...
    assert_eq!(filling::Column::Table.to_string(), "table");
    // ...but `#[strum(disabled)]` keeps it out of iteration.
    assert!(
        !filling::Column::iter()
            .map(|c| c.to_string())
            .any(|c| c == "table")
    );
}

// [spec:pgorm:syn:macros.derive.entity-model.attrs/test]    the Table variant has no column def
#[test]
#[should_panic(expected = "Table cannot be used as a column")]
fn table_iden_variant_has_no_column_def() {
    let _ = filling::Column::Table.def();
}

// [spec:pgorm:syn:macros.derive.entity-model.attrs/test]    field-level keys, and unknown keys skipped
#[test]
fn field_level_attributes_shape_the_column_defs() {
    use pgorm::ColumnTypeTrait;

    // bare `primary_key`
    assert_eq!(filling::Column::Id.def(), ColumnType::Integer.def());
    // `column_type` + `unique` + `indexed` + `comment`
    assert_eq!(
        filling::Column::Name.def(),
        ColumnType::Text
            .def()
            .unique()
            .indexed()
            .comment("display name")
    );
    // bare `nullable` + `default_value`
    assert_eq!(
        filling::Column::Weight.def(),
        ColumnType::Integer.def().nullable().default_value(0)
    );
    // `default_expr`
    assert_eq!(
        filling::Column::BakedOn.def(),
        ColumnType::Date.def().default(Expr::current_date())
    );
    // `column_name`
    assert_eq!(filling::Column::Sku.to_string(), "SKU");
    // `enum_name`
    assert_eq!(filling::Column::Renamed.to_string(), "renamed");
    // `ignore` keeps the field out of `Column` entirely; an unrecognised key is
    // silently skipped rather than diagnosed, so `plain` is still a column.
    assert_eq!(filling::Column::Plain.def(), ColumnType::Integer.def());
}

// [spec:pgorm:sem:macros.derive.entity-model.column-def+3/test]    the Rust-type inference table
#[test]
fn column_types_inferred_from_rust_type_name() {
    use pgorm::ColumnTypeTrait;
    use scalars::Column as C;

    assert_eq!(C::Txt.def(), ColumnType::string(None).def());
    assert_eq!(C::Tiny.def(), ColumnType::SmallInteger.def());
    assert_eq!(C::Small.def(), ColumnType::SmallInteger.def());
    assert_eq!(C::Id.def(), ColumnType::Integer.def());
    assert_eq!(C::Big.def(), ColumnType::BigInteger.def());
    assert_eq!(C::Uns.def(), ColumnType::BigInteger.def());
    assert_eq!(C::Single.def(), ColumnType::Float.def());
    assert_eq!(C::Dbl.def(), ColumnType::Double.def());
    assert_eq!(C::Flag.def(), ColumnType::Boolean.def());
    assert_eq!(C::D.def(), ColumnType::Date.def());
    assert_eq!(C::T.def(), ColumnType::Time.def());
    assert_eq!(C::Dt.def(), ColumnType::Timestamp.def());
    assert_eq!(C::Dtu.def(), ColumnType::TimestampWithTimeZone.def());
    assert_eq!(C::Dtl.def(), ColumnType::TimestampWithTimeZone.def());
    assert_eq!(C::Dttz.def(), ColumnType::TimestampWithTimeZone.def());
    assert_eq!(C::Uid.def(), ColumnType::Uuid.def());
    assert_eq!(C::Js.def(), ColumnType::Json.def());
    assert_eq!(C::Dec.def(), ColumnType::Decimal(None).def());
    assert_eq!(C::Blob.def(), ColumnType::Bytea.def());

    // `Option<T>` is unwrapped to `T` and forces nullability.
    assert_eq!(C::Maybe.def(), ColumnType::BigInteger.def().nullable());

    // Anything else resolves through `<T as ValueType>::column_type()`.
    assert_eq!(
        C::Tea.def(),
        <scalars::Tea as pgorm::pgorm_query::ValueType>::column_type().def()
    );
    assert_eq!(C::Tea.def(), ColumnType::String(StringLen::N(1)).def());
}

// [spec:pgorm:sem:macros.derive.entity-model.column-def+3/test]    explicit column_type wins
#[test]
fn an_explicit_column_type_overrides_the_inferred_one() {
    // `name: String` would infer `string(None)`; the attribute pins `Text`.
    assert_ne!(
        filling::Column::Name.def().get_column_type(),
        &ColumnType::string(None)
    );
    assert_eq!(
        filling::Column::Name.def().get_column_type(),
        &ColumnType::Text
    );
}

// [spec:pgorm:sem:macros.derive.entity-model.casing+1/test]
#[test]
fn column_variant_names_follow_the_casing_rules() {
    // Raw identifier trimmed, then UpperCamelCase.
    assert_eq!(casing::Column::Type.to_string(), "type");
    // Escaped keyword: `SELF` upper-camel-cases to `Self`, which gets a
    // trailing underscore to stay a legal identifier.
    assert_eq!(casing::Column::Self_.to_string(), "self");
    // `enum_name` replaces the variant name entirely.
    assert_eq!(casing::Column::Wholly.to_string(), "wholly");
}

// [spec:pgorm:sem:macros.derive.entity-model.casing+1/test]    when the SQL name is pinned
#[test]
fn sql_column_names_are_pinned_only_when_needed() {
    // Clean snake_case: no attribute, the name falls out of `DeriveColumn`.
    assert_eq!(casing::Column::FirstName.to_string(), "first_name");
    // Does not survive the round trip, so the original snake_case is pinned
    // (`Abc1` would otherwise render as `abc1`).
    assert_eq!(casing::Column::Abc1.to_string(), "abc_1");
    // With `rename_all`, the variant name is converted per the case style...
    assert_eq!(renamed::Column::FirstName.to_string(), "FIRST_NAME");
    // ...unless an explicit `column_name` wins.
    assert_eq!(renamed::Column::SecondName.to_string(), "explicit");
}

// [spec:pgorm:sem:macros.derive.entity-model.primary-key+1/test]
#[test]
fn primary_key_value_type_and_auto_increment() {
    // A single key contributes a bare type...
    let _: <single_pk::PrimaryKey as PrimaryKeyTrait>::ValueType = 1i32;
    assert!(single_pk::PrimaryKey::auto_increment());

    // ...composite keys contribute a tuple, and disable auto-increment.
    let _: <composite_pk::PrimaryKey as PrimaryKeyTrait>::ValueType = (1i32, 2i32);
    assert!(!composite_pk::PrimaryKey::auto_increment());
    let keys: Vec<String> = composite_pk::PrimaryKey::iter()
        .map(|k| k.to_string())
        .collect();
    assert_eq!(keys, vec!["cake_id", "filling_id"]);

    // The flag is shared across the whole struct: `auto_increment = false` on a
    // field that is not a primary key still flips it globally.
    assert!(!shared_auto_increment::PrimaryKey::auto_increment());
}

// [spec:pgorm:sem:macros.derive.entity-model.primary-key+1/test]    what DerivePrimaryKey itself emits
#[test]
fn derive_primary_key_emits_iden_and_mapping() {
    // `IdenStr` maps the variant to its snake_case name, or a `column_name`
    // override; `Iden` delegates to it.
    assert_eq!(pgorm::IdenStr::as_str(&single_pk::PrimaryKey::Id), "ID");
    assert_eq!(single_pk::PrimaryKey::Id.to_string(), "ID");
    assert_eq!(
        pgorm::IdenStr::as_str(&composite_pk::PrimaryKey::CakeId),
        "cake_id"
    );

    // `PrimaryKeyToColumn` maps to the same-named variants of `Column`.
    assert!(matches!(
        single_pk::PrimaryKey::Id.into_column(),
        single_pk::Column::Id
    ));
    assert!(matches!(
        single_pk::PrimaryKey::from_column(single_pk::Column::Id),
        Some(single_pk::PrimaryKey::Id)
    ));
    assert!(
        single_pk::PrimaryKey::from_column(single_pk::Column::Name).is_none(),
        "a non-key column has no PrimaryKey variant"
    );
}

// [spec:pgorm:sem:macros.derive.entity+1/test]    EntityTrait associated types and the Iden pair
#[test]
fn derive_entity_wires_up_the_entity_trait() {
    // The five associated types default to the conventional names.
    let _: <filling::Entity as EntityTrait>::Model = filling::Model {
        id: 1,
        name: String::new(),
        vendor_id: None,
        weight: 0,
        baked_on: Date::from_ymd_opt(2024, 1, 1).unwrap(),
        sku: String::new(),
        original: 0,
        casted: String::new(),
        not_a_column: 0,
        plain: 0,
    };
    let _: <filling::Entity as EntityTrait>::ActiveModel =
        <filling::ActiveModel as Default>::default();
    let _: <filling::Entity as EntityTrait>::Column = filling::Column::Id;
    let _: <filling::Entity as EntityTrait>::PrimaryKey = filling::PrimaryKey::Id;
    fn assert_relation<E: EntityTrait<Relation = filling::Relation>>() {}
    assert_relation::<filling::Entity>();

    // `Iden` and `IdenStr` both render `EntityName::table_name`.
    assert_eq!(pgorm::IdenStr::as_str(&filling::Entity), "filling");
    assert_eq!(filling::Entity.to_string(), "filling");
}

// [spec:pgorm:sem:macros.derive.entity+1/test]    EntityName only when table_name is present
#[test]
fn derive_entity_omits_entity_name_without_table_name() {
    // `no_table_name::Entity` carries `DeriveEntity` with no `table_name`, and a
    // hand-written `EntityName` impl right beside it. If the derive had emitted
    // its own `EntityName` this module would not compile at all.
    assert_eq!(no_table_name::Entity.table_name(), "hand_written");
    assert_eq!(no_table_name::Entity.schema_name(), None);
    assert_eq!(no_table_name::Entity.to_string(), "hand_written");
    // The composite still produced Column / PrimaryKey without an Entity struct
    // of its own.
    assert_eq!(no_table_name::Column::Id.to_string(), "id");
}
