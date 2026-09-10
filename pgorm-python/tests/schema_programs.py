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
        "generated": base.column(p.ColumnDef("twice", "integer").generated(p.col('id "x"') * 2)),
        "column_specs": p.CreateTable(table).column(p.ColumnDef("id", "bigint").primary_key().auto_increment())
            .column(p.ColumnDef("n", "integer").unique().null().check(p.col("n") > 0)),
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
        "drop_column": p.drop_column(table, "extra"),
        "enum": p.create_enum(kind, ["", "O'Brien \\ 雪", "busy"]),
        "enum_before": p.add_enum_value(kind, "new", before="busy"),
        "enum_after": p.add_enum_value(kind, "new", after="busy"),
        "enum_rename_value": p.rename_enum_value(kind, "busy", "calm"),
        "enum_rename": p.rename_enum(kind, 'new "x"'),
        "enum_drop": p.drop_enum(kind, if_exists=True, cascade=True),
    }
