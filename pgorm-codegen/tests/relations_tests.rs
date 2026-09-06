//! Relation variants and attributes, the `Related` impls, and the many-to-many
//! `via` impls.

mod common;

use common::*;
use pgorm_query::{Alias, ColumnDef, ForeignKey, ForeignKeyAction, Table, TableCreateStatement};

fn bare(table: &str) -> TableCreateStatement {
    table_with(table, vec![serial_pk("id")])
}

/// `cake` with a self-referencing `base_id`.
fn self_referencing_cake() -> TableCreateStatement {
    Table::create(Alias::new("cake"))
        .col(serial_pk("id"))
        .col(ColumnDef::new(Alias::new("base_id")).integer().to_owned())
        .foreign_key(&mut ForeignKey::create(
            Alias::new("cake"),
            Alias::new("base_id"),
            Alias::new("cake"),
            Alias::new("id"),
        ))
        .to_owned()
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

fn basket_with_two_fruit_keys() -> TableCreateStatement {
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
        .to_owned()
}

// [spec:pgorm:sem:codegen.entity.relations+1/test]    variants are the
// UpperCamelCase of the referenced table, `SelfRef` when self-referencing, with
// a nonzero `num_suffix` appended
#[test]
fn relation_variants_are_named_after_the_referenced_table() {
    let plain = generate(vec![cake(), fruit()], Opts::default());
    assert_contains(plain.file("fruit.rs"), "Cake,");

    let self_ref = generate(vec![self_referencing_cake()], Opts::default());
    assert_contains(self_ref.file("cake.rs"), "SelfRef,");

    // two FKs onto the same table: each variant takes a distinct 1-based
    // suffix. NB the generator numbers them in reverse declaration order, so
    // `fruit_id1` lands on `Fruit2`.
    let suffixed = generate(
        vec![bare("fruit"), basket_with_two_fruit_keys()],
        Opts::default(),
    );
    let basket = suffixed.file("basket.rs");
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
}

// [spec:pgorm:sem:codegen.entity.relations+1/test]    a compact FK-owning relation
// renders belongs_to / from / to, with parenthesized tuples for multi-column
// keys and `Entity` (no module path) when self-referencing
#[test]
fn compact_belongs_to_attributes_carry_from_and_to() {
    let single = generate(vec![cake(), fruit()], Opts::default());
    assert_contains(
        single.file("fruit.rs"),
        r#"#[pgorm(
            belongs_to = "super::cake::Entity",
            from = "Column::CakeId",
            to = "super::cake::Column::Id",
            on_update = "Cascade",
            on_delete = "Cascade",
        )]
        Cake,"#,
    );

    let self_ref = generate(vec![self_referencing_cake()], Opts::default());
    assert_contains(
        self_ref.file("cake.rs"),
        r#"#[pgorm(belongs_to = "Entity", from = "Column::BaseId", to = "Column::Id",)] SelfRef,"#,
    );

    let composite = generate(
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
                    .col(Alias::new("cake_kind"), Alias::new("kind")),
                )
                .to_owned(),
        ],
        Opts::default(),
    );
    assert_contains(
        composite.file("fruit.rs"),
        r#"from = "(Column::CakeId, Column::CakeKind)","#,
    );
    assert_contains(
        composite.file("fruit.rs"),
        r#"to = "(super::cake::Column::Id, super::cake::Column::Kind)","#,
    );
}

// [spec:pgorm:sem:codegen.entity.relations+1/test]    `on_update` / `on_delete`
// appear only when the FK declared an action, and cover all five actions
#[test]
fn foreign_key_actions_render_only_when_declared() {
    let mut audit = Table::create(Alias::new("audit"));
    audit.col(serial_pk("id"));

    let cases = [
        ("restrict", ForeignKeyAction::Restrict, "Restrict"),
        ("cascade", ForeignKeyAction::Cascade, "Cascade"),
        ("setnull", ForeignKeyAction::SetNull, "SetNull"),
        ("noaction", ForeignKeyAction::NoAction, "NoAction"),
        ("setdefault", ForeignKeyAction::SetDefault, "SetDefault"),
    ];
    let mut schema: Vec<TableCreateStatement> = Vec::new();
    for (target, action, _) in &cases {
        let column = format!("{target}_id");
        audit.col(ColumnDef::new(Alias::new(&column)).integer().to_owned());
        audit.foreign_key(
            ForeignKey::create(
                Alias::new("audit"),
                Alias::new(&column),
                Alias::new(*target),
                Alias::new("id"),
            )
            .on_delete(*action)
            .on_update(*action),
        );
        schema.push(bare(target));
    }
    // one more FK with no declared action at all
    audit.col(ColumnDef::new(Alias::new("plain_id")).integer().to_owned());
    audit.foreign_key(&mut ForeignKey::create(
        Alias::new("audit"),
        Alias::new("plain_id"),
        Alias::new("plain"),
        Alias::new("id"),
    ));
    schema.push(bare("plain"));
    schema.push(audit);

    let generated = generate(schema, Opts::default());
    let audit = generated.file("audit.rs");

    for (_, _, rendered) in cases {
        assert_contains(audit, &format!(r#"on_update = "{rendered}","#));
        assert_contains(audit, &format!(r#"on_delete = "{rendered}","#));
    }
    assert_contains(
        audit,
        r#"#[pgorm(
            belongs_to = "super::plain::Entity",
            from = "Column::PlainId",
            to = "super::plain::Column::Id",
        )]
        Plain,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.relations+1/test]    inverse relations render as
// `has_one` / `has_many` with no `from` / `to`
#[test]
fn inverse_relations_render_without_from_and_to() {
    let generated = generate(vec![cake(), fruit()], Opts::default());
    assert_contains(
        generated.file("cake.rs"),
        r#"#[pgorm(has_many = "super::fruit::Entity")] Fruit,"#,
    );

    let has_one = generate(
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
        has_one.file("users.rs"),
        r#"#[pgorm(has_one = "super::profile::Entity")] Profile,"#,
    );
}

// [spec:pgorm:sem:codegen.entity.relations+1/test]    in the expanded format the
// same facts render as `RelationTrait::def` builder chains
#[test]
fn expanded_relation_defs_render_as_builder_chains() {
    let generated = generate(vec![cake(), fruit()], expanded());

    assert_contains(
        generated.file("cake.rs"),
        "Self::Fruit => Entity::has_many(super::fruit::Entity).into(),",
    );
    assert_contains(
        generated.file("fruit.rs"),
        "Self::Cake => Entity::belongs_to(super::cake::Entity)
            .columns(Column::CakeId, super::cake::Column::Id)
            .into(),",
    );

    let self_ref = generate(vec![self_referencing_cake()], expanded());
    assert_contains(
        self_ref.file("cake.rs"),
        "Self::SelfRef => Entity::belongs_to(Entity)
            .columns(Column::BaseId, Column::Id)
            .into(),",
    );
}

// [spec:pgorm:sem:codegen.entity.relations.related/test]    an ordinary relation
// gets `impl Related<super::<module>::Entity> for Entity`, in both formats
#[test]
fn ordinary_relations_get_related_impl_in_both_formats() {
    for opts in [Opts::default(), expanded()] {
        let generated = generate(vec![cake(), fruit()], opts);
        assert_contains(
            generated.file("fruit.rs"),
            "impl Related<super::cake::Entity> for Entity {
                fn to() -> RelationDef { Relation::Cake.def() }
            }",
        );
        assert_contains(
            generated.file("cake.rs"),
            "impl Related<super::fruit::Entity> for Entity {
                fn to() -> RelationDef { Relation::Fruit.def() }
            }",
        );
    }
}

// [spec:pgorm:sem:codegen.entity.relations.related/test]    self-referencing and
// suffixed relations get `Relation` variants but no `Related` impl
#[test]
fn self_ref_and_suffixed_get_no_related_impl() {
    let self_ref = generate(vec![self_referencing_cake()], Opts::default());
    let cake = self_ref.file("cake.rs");
    assert_contains(cake, "SelfRef,");
    assert_not_contains(cake, "impl Related<Entity> for Entity");

    let suffixed = generate(
        vec![bare("fruit"), basket_with_two_fruit_keys()],
        Opts::default(),
    );
    let basket = suffixed.file("basket.rs");
    assert_contains(basket, "Fruit1,");
    assert_not_contains(basket, "impl Related<super::fruit::Entity> for Entity");
}

// [spec:pgorm:sem:codegen.entity.relations.related/test]    a conjunct-shadowed
// relation loses its plain `Related` impl to the `via` one
#[test]
fn conjunct_shadowed_relations_lose_their_plain_related_impl() {
    let generated = generate(
        vec![
            bare("users"),
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
            junction("users_votes", ("users", "user_id"), ("bills", "bill_id")),
        ],
        Opts::default(),
    );
    let users = generated.file("users.rs");

    assert_contains(
        users,
        r#"#[pgorm(has_many = "super::bills::Entity")] Bills,"#,
    );
    assert_not_contains(
        users,
        "impl Related<super::bills::Entity> for Entity {
            fn to() -> RelationDef { Relation::Bills.def() }
        }",
    );
    assert_contains(users, "fn via() -> Option<RelationDef>");
}

// [spec:pgorm:sem:codegen.entity.relations.via/test]    each conjunct relation
// emits the many-to-many `Related` impl with `to()` and `via()`
#[test]
fn conjunct_relations_emit_a_via_related_impl() {
    for opts in [Opts::default(), expanded()] {
        let generated = generate(cake_schema(), opts);
        assert_contains(
            generated.file("cake.rs"),
            "impl Related<super::filling::Entity> for Entity {
                fn to() -> RelationDef { super::cake_filling::Relation::Filling.def() }
                fn via() -> Option<RelationDef> {
                    Some(super::cake_filling::Relation::Cake.def().rev())
                }
            }",
        );
    }
}

// [spec:pgorm:sem:codegen.entity.relations.via/test]    the `via` impls come
// after the plain `Related` impls, in both formats
#[test]
fn via_impls_follow_the_plain_related_impls() {
    for opts in [Opts::default(), expanded()] {
        let generated = generate(cake_schema(), opts);
        let cake = generated.file("cake.rs");

        let plain = position_of(cake, "impl Related<super::fruit::Entity> for Entity");
        let via = position_of(cake, "impl Related<super::filling::Entity> for Entity");
        assert!(plain < via, "expected the via impl last:\n{cake}");
        assert!(
            via < position_of(cake, "impl ActiveModelBehavior for ActiveModel"),
            "expected the via impl before ActiveModelBehavior"
        );
    }
}
