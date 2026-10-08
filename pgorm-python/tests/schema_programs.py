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
            .unique().nulls_not_distinct().method("btree").if_not_exists(),
        "index_gin": p.CreateIndex(table, "moods").method("gin"),
        "drop_index": p.drop_index(table, 'index "x"', if_exists=True),
        "drop_table": p.drop_table(table, if_exists=True, cascade=True),
        "rename_table": p.rename_table(table, 'new "x"'),
        "rename_column": p.rename_column(table, 'id "x"', 'new "x"'),
        "truncate": p.truncate(table),
        "add_column": p.add_column(table, p.ColumnDef("extra", "text"), if_not_exists=True),
        "modify_column": p.modify_column(table, p.ColumnDef("extra", "varchar").not_null().default("hello")),
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
    }
