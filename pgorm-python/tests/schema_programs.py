"""DDL inputs shared by independent native parity and installed-wheel tests."""


def programs(p):
    table = p.Table('items "雪"', schema='schema "雪"')
    kind = p.TypeName('Mood "雪"', schema='schema "雪"')
    id_col = p.ColumnDef('id "x"', "integer").not_null()
    base = p.CreateTable(table).column(id_col)
    return {
        "table": base.column(p.ColumnDef("name", p.DataType("varchar", length=80)).default("O'Brien \\ 雪"))
            .column(p.ColumnDef("mood", kind).default(p.Value("busy", kind)))
            .column(p.ColumnDef("moods", p.DataType(kind).array()))
            .column(p.ColumnDef("amount", p.DataType("numeric", precision=12, scale=3)))
            .primary_key('id "x"').unique("name", name='unique "x"', nulls_not_distinct=True)
            .check(p.col('id "x"') > 0).if_not_exists(),
        "generated": base.column(p.ColumnDef("twice", "integer").generated(p.col('id "x"') * 2, "stored")),
        "generated_virtual": base.column(p.ColumnDef("next", "integer").generated(p.col('id "x"') + 1, "virtual")),
        "column_specs": p.CreateTable(table).column(p.ColumnDef("id", "bigint").auto_increment())
            .column(p.ColumnDef("n", "integer").null().check(p.col("n") > 0)).primary_key("id").unique("n"),
        "index": p.CreateIndex(table, "name", name='index "x"').column('id "x"', descending=True)
            .unique().nulls_not_distinct().using("btree").if_not_exists(),
        "index_gin": p.CreateIndex(table, "moods").using("gin"),
        "drop_index": p.drop_index(table, 'index "x"', if_exists=True),
        "drop_table": p.drop_table(table, if_exists=True, cascade=True),
        "rename_table": p.rename_table(table, 'new "x"'),
        "rename_column": p.rename_column(table, 'id "x"', 'new "x"'),
        "truncate_table": p.truncate_table(table),
        "add_column": p.add_column(table, p.ColumnDef("extra", "text"), if_not_exists=True),
        "modify_column": p.modify_column(table, p.ColumnDef("extra", "varchar").not_null().default("hello")),
        "collated": p.CreateTable(table).column(p.ColumnDef('word "x"', "text").not_null().collate("C"))
            .column(p.ColumnDef("label", "text").collate("C").collate("POSIX", schema="pg_catalog")),
        "add_column_collated": p.add_column(table, p.ColumnDef("extra", "text").collate('C "x"', schema='schema "雪"')),
        "modify_column_collated": p.modify_column(table, p.ColumnDef("extra", "varchar").collate("C")),
        "add_primary_key": p.add_primary_key(table, 'id "x"', "name"),
        "add_unique": p.add_unique(table, "name", 'id "x"', name='unique "x"', nulls_not_distinct=True),
        "temporal_keys": base.column(p.ColumnDef('during "x"', p.TypeName("tstzrange")))
            .primary_key('id "x"', without_overlaps='during "x"')
            .unique("name", 'id "x"', name='unique "x"', nulls_not_distinct=True, without_overlaps='during "x"'),
        "add_primary_key_temporal": p.add_primary_key(table, 'id "x"', without_overlaps='during "x"'),
        "add_unique_temporal": p.add_unique(table, "name", without_overlaps='during "x"'),
        "drop_column": p.drop_column(table, "extra"),
        "set_expression": p.set_expression(table, "twice", p.col('id "x"') * 3),
        "drop_expression": p.drop_expression(table, "twice"),
        "drop_expression_if_exists": p.drop_expression(table, "twice", if_exists=True),
        "not_null_named": p.CreateTable(table).column(p.ColumnDef("extra", "text").not_null(name='present "x"', no_inherit=True))
            .column(p.ColumnDef("kept", "text").not_null(no_inherit=True)),
        "add_not_null": p.add_not_null(table, "extra"),
        "add_not_null_named": p.add_not_null(table, "extra", name='present "x"', no_inherit=True, not_valid=True),
        "validate_constraint": p.validate_constraint(table, 'present "x"'),
        "alter_constraint_inherit": p.alter_constraint(table, 'present "x"', "inherit"),
        "alter_constraint_no_inherit": p.alter_constraint(table, 'present "x"', "no_inherit"),
        "drop_constraint": p.drop_constraint(table, 'present "x"'),
        "drop_constraint_if_exists": p.drop_constraint(table, 'present "x"', if_exists=True, cascade=True),
        "rename_constraint": p.rename_constraint(table, 'present "x"', 'kept "x"'),
        "check_named": p.CreateTable(table)
            .column(p.ColumnDef("n", "integer").check(p.col("n") > 0, name='positive "x"', not_enforced=True))
            .check(p.col("n") < 100, name='small "x"').check(p.col("n") != 7, not_enforced=True),
        "add_check": p.add_check(table, p.col("n") > 0, name='positive "x"', not_enforced=True),
        "add_check_plain": p.add_check(table, p.col("n") > 0),
        "add_check_not_valid": p.add_check(table, p.col("n") > 0, name='positive "x"', no_inherit=True, not_valid=True),
        "check_no_inherit": p.CreateTable(table)
            .column(p.ColumnDef("n", "integer").check(p.col("n") > 0, no_inherit=True))
            .check(p.col("n") < 100, name='small "x"', no_inherit=True, not_enforced=True),
        "alter_constraint_enforced": p.alter_constraint(table, 'fk "x"', "enforced"),
        "alter_constraint_not_enforced": p.alter_constraint(table, 'fk "x"', "not_enforced"),
        "enum": p.create_enum(kind, ["", "O'Brien \\ 雪", "busy"]),
        "enum_before": p.add_enum_value(kind, "new", before="busy"),
        "enum_after": p.add_enum_value(kind, "new", after="busy"),
        "enum_rename_value": p.rename_enum_value(kind, "busy", "calm"),
        "enum_rename": p.rename_enum(kind, 'new "x"'),
        "enum_drop": p.drop_enum(kind, if_exists=True, cascade=True),
        **views(p, table),
        **schemas(p),
        **domains(p),
    }


def domains(p):
    name = p.TypeName('Qty "雪"', schema='schema "雪"')
    value = p.domain_value()
    return {
        "create_domain": p.CreateDomain('qty "雪"', "integer"),
        "create_domain_clauses": p.CreateDomain(name, "text")
            .check(value != "", name='filled "x"').not_null().default("O'Brien \\ 雪")
            .collate("C", schema="pg_catalog").check(value != "none"),
        "create_domain_over_array": p.CreateDomain(name, p.DataType("varchar", length=20).array())
            .null().default(p.literal(1) + 2),
        "create_domain_over_domain": p.CreateDomain("small", name).not_null().null(),
        "set_domain_default": p.set_domain_default(name, 5),
        "drop_domain_default": p.drop_domain_default(name),
        "set_domain_not_null": p.set_domain_not_null(name),
        "drop_domain_not_null": p.drop_domain_not_null(name),
        "add_domain_check": p.add_domain_check(name, value > 0),
        "add_domain_check_not_valid": p.add_domain_check(
            name, value < 1000, name='small "x"', not_valid=True,
        ),
        "drop_domain_constraint": p.drop_domain_constraint(
            name, 'small "x"', if_exists=True, cascade=True,
        ),
        "rename_domain_constraint": p.rename_domain_constraint(name, 'small "x"', 'tiny "雪"'),
        "validate_domain_constraint": p.validate_domain_constraint(name, 'small "x"'),
        "rename_domain": p.rename_domain(name, 'units "雪"'),
        "drop_domain": p.drop_domain(name),
        "drop_domains": p.drop_domain(
            name, "plain", p.TypeName("other", schema="archive"), if_exists=True, cascade=True,
        ),
    }


def schemas(p):
    name = 'schema "雪"'
    return {
        "create_schema": p.create_schema(name),
        "create_schema_if_not_exists": p.create_schema(p.Identifier(name), if_not_exists=True),
        "drop_schema": p.drop_schema(name),
        "drop_schemas": p.drop_schema(name, "archive", p.Identifier('Old "x"'), if_exists=True, cascade=True),
        "rename_schema": p.rename_schema(name, 'renamed "雪"'),
    }


def views(p, table):
    query = p.Select(p.col('id "x"'), p.col("name")).from_(table).where(p.col("name") != "O'Brien \\ 雪")
    view = p.Table('view "雪"', schema='schema "雪"')
    return {
        "create_view": p.create_view(view, query),
        "create_view_options": p.create_view(
            view, query, columns=['key "x"', "label"], or_replace=True, check_option="local",
            security_barrier=True, security_invoker=True,
        ),
        "create_view_cascaded": p.create_view(view, query, check_option="cascaded"),
        "create_recursive_view": p.create_recursive_view(
            view, ['key "x"', "label"], query, or_replace=True, security_barrier=True,
        ),
        "drop_view": p.drop_view(view, if_exists=True, cascade=True),
        "create_materialized_view": p.create_materialized_view(
            view, query, columns=['key "x"'], if_not_exists=True, with_no_data=True,
        ),
        "refresh_materialized_view": p.refresh_materialized_view(view),
        "refresh_concurrently": p.refresh_materialized_view(view, concurrently=True),
        "refresh_with_no_data": p.refresh_materialized_view(view, with_no_data=True),
        "drop_materialized_view": p.drop_materialized_view(view, if_exists=True),
        "create_table_as": p.create_table_as(
            view, query, columns=['key "x"', "label"], if_not_exists=True, with_no_data=True,
        ),
    }
