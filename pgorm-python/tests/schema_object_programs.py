"""DDL over keys, foreign keys, identities, ALTER TABLE's actions, index entries, types,
sequences, extensions and comments, held to the Rust builders by schema_objects.rs."""

from decimal import Decimal


def programs(p):
    booking = p.Table("booking", schema="app")
    room = p.Table("room", schema="app")
    mood = p.TypeName("mood", schema="app")

    def app(name):
        return p.Table(name, schema="app")

    query = (
        p.Select(p.col("id"), p.col("room")).from_(booking)
        .where(p.col("room") > 10).where(p.col("note") != "it's")
    )
    return {
        **tables(p, booking, room, mood),
        **indexes(p, booking),
        **types(p, mood),
        **sequences(p, booking),
        "create_view_repeated": p.create_view(app("large_room"), query).or_replace()
            .columns(["booking"]).columns(["room_no"]).check_option("local")
            .check_option("cascaded").security_barrier().security_invoker(),
        "drop_views": p.drop_view(["large_room", app("other")], if_exists=True, behavior="cascade"),
        "drop_materialized_views": p.drop_materialized_view([app("room_total"), "other"], behavior="restrict"),
        "drop_schema_restrict": p.drop_schema("app", behavior="restrict"),
        "drop_domains_restrict": p.drop_domain(
            ["quantity", p.TypeName("quantity", schema="app")], if_exists=True, behavior="restrict",
        ),
        "comment_on_table": p.comment_on_table(booking, "Rooms booked, and when"),
        "comment_on_column": p.comment_on_column(booking, "note", "it's a \\ note\nover two lines"),
    }


def tables(p, booking, room, mood):
    return {
        "create_table_columns": p.create_table(booking)
            .column(p.ColumnDef("id", "bigint").identity("always", start_with=10, increment_by=2, max_value=None))
            .column(p.ColumnDef("legacy", "integer").auto_increment())
            .column(
                p.ColumnDef("seats", "smallint")
                .not_null(name="seats_present", no_inherit=True)
                .default(1)
                .check(p.col("seats") > 0, name="seats_positive", enforcement="not_enforced")
            )
            .column(p.ColumnDef("note", "text").null().collate("C", schema="pg_catalog"))
            .column(p.ColumnDef("price", p.DataType("numeric", precision=10, scale=2)).default(Decimal("9.50")))
            .column(p.ColumnDef("doubled", "integer").generated(p.col("seats") * 2, "stored"))
            .column(p.ColumnDef("halved", "integer").generated(p.col("seats") / 2, "virtual"))
            .column(p.ColumnDef("serial", "bigint").identity("by_default")),
        "create_table_keys": p.create_table(booking)
            .if_not_exists()
            .column(p.ColumnDef("id", "bigint"))
            .column(p.ColumnDef("room", "integer"))
            .column(p.ColumnDef("during", "tstzrange"))
            .primary_key("id", name="booking_key", include=["room"], deferrability="deferrable_initially_immediate")
            .unique(["room", "id"], nulls_not_distinct=True)
            .unique("room", without_overlaps="during", name="no_double_booking")
            .foreign_key(
                ["room", "id"], room, ["id", "booking"], name="booking_room", on_delete="cascade",
                on_update="set_null", deferrability="deferrable_initially_deferred",
                enforcement="not_enforced",
            )
            .foreign_key("room", room, "id", period=("during", "open"))
            .check(p.col("room") < 1000, name="room_small", no_inherit=True, enforcement="enforced"),
        "create_table_typed_columns": p.create_table("feeling")
            .column(p.ColumnDef("mood", mood).default(p.Value("calm", mood)))
            .column(p.ColumnDef("moods", p.DataType(mood).array()))
            .column(p.ColumnDef("tags", p.DataType("varchar", length=20).array()))
            .column(p.ColumnDef("span", p.CreatedRange("floatrange", "f64", schema="app")))
            .column(p.ColumnDef("token", "uuid").default(p.call("gen_random_uuid"))),
        "alter_table_actions": p.alter_table(booking)
            .add_column(p.ColumnDef("extra", "text").not_null(), if_not_exists=True)
            .drop_column("legacy")
            .add_primary_key(["id"], name="booking_pkey")
            .add_unique("room", nulls_not_distinct=True, deferrability="not_deferrable")
            .add_foreign_key("room", room, "id", name="room_fk", not_valid=True, on_delete="restrict")
            .add_check(p.col("seats") <= 9, name="few_seats", not_valid=True, no_inherit=True)
            .add_not_null("note", name="note_present", no_inherit=True, not_valid=True)
            .drop_constraint("old", if_exists=True, behavior="cascade")
            .validate_constraint("room_fk")
            .alter_constraint("room_fk", "not_enforced")
            .alter_constraint("note_present", "inherit")
            .set_expression("doubled", p.col("seats") * 3)
            .drop_expression("halved", if_exists=True),
        "alter_table_modify_column": p.alter_table(booking)
            .modify_column(p.ColumnDef("note", "varchar").collate("C").default("none").not_null())
            .modify_column(p.ColumnDef("seats").null())
            .modify_column(p.ColumnDef("room").not_null(name="room_present"))
            .modify_column(p.ColumnDef("serial").identity("always", cache=5))
            .modify_column(p.ColumnDef("price").check(p.col("price") >= 0)),
        "drop_tables": p.drop_table([booking, "scratch"], if_exists=True, behavior="restrict"),
    }


def indexes(p, booking):
    return {
        "create_index_entries": p.create_index(
            booking, p.call("lower", p.col("note")), name="booking_note", order="desc",
            operator_class="text_pattern_ops",
        )
            .column("room")
            .column("id", order="asc")
            .nulls_not_distinct()
            .if_not_exists()
            .include(["seats"])
            .where(p.col("seats") > 0)
            .where(~p.col("room").is_null()),
        "create_index_methods": p.create_index("doc", p.col("body")).using("gin").column(p.col("tags")),
        "create_index_access_method": p.create_index(booking, "during").using("gist"),
        "drop_index": p.drop_index(booking, "booking_note", if_exists=True),
    }


def types(p, mood):
    return {
        "create_shell_type": p.create_type("later"),
        "create_enum": p.create_type(mood).values(["calm", "O'Brien"]).values([""]),
        "create_composite": p.create_type("pair").attribute("a", "integer")
            .attribute("b", "text", collation={"name": "C", "schema": "pg_catalog"})
            .attribute("c", mood),
        "create_empty_composite": p.create_type("nothing").values(["x"]).as_composite(),
        "create_range": p.create_type(p.TypeName("floatrange", schema="app")).as_range(
            "double", subtype_diff="float8mi", subtype_opclass="float8_ops",
            multirange_type_name=p.TypeName("floatmultirange", schema="app"),
        ),
        "create_text_range": p.create_type("textrange").as_range("text", collation="C"),
        "alter_enum_add_value": p.alter_type(mood).add_value("tense", after="calm"),
        "alter_composite": p.alter_type("pair")
            .add_attribute("d", "bigint")
            .add_attribute("e", "text", collation="C")
            .drop_attribute("a", if_exists=True)
            .drop_attribute("b")
            .alter_attribute("c", "text", collation="C")
            .alter_attribute("d", "integer")
            .behavior("cascade"),
        "rename_attribute": p.alter_type("pair").rename_attribute("a", "z").behavior("restrict"),
        "drop_types": p.drop_type([mood, "pair"], if_exists=True, behavior="cascade"),
    }


def sequences(p, booking):
    return {
        "create_sequence": p.create_sequence(p.Table("ticket", schema="app"))
            .if_not_exists()
            .as_type("integer")
            .options(increment_by=5, min_value=None, max_value=9_007_199_254_740_991, start_with=-3, cache=2)
            .options(cycle=True, min_value=1)
            .owned_by(booking, "id"),
        "alter_sequence": p.alter_sequence("ticket").restart(100).if_exists().as_type("bigint")
            .options(cycle=False).owned_by(None),
        "alter_sequence_restart": p.alter_sequence("ticket").restart(),
        "drop_sequences": p.drop_sequence(["ticket", p.Table("other", schema="app")], behavior="cascade"),
        "rename_sequence": p.rename_sequence(p.Table("ticket", schema="app"), "voucher"),
        "create_extension": p.create_extension(
            "btree_gist", if_not_exists=True, schema="public", version="1.7", cascade=True,
        ),
        "drop_extension": p.drop_extension("btree_gist", if_exists=True, behavior="restrict"),
    }
