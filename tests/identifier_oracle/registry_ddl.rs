//! The registry, continued: pgorm-query's DDL sites. See `registry.rs` for
//! how to add one.

use pgorm::pgorm_query::{
    ColumnDef, ColumnType, Comment, ForeignKey, Index, IndexColumn, IndexType, Table,
    TableForeignKey, TypeName,
    extension::{Extension, Type},
};

use super::{
    oracle::{
        Policy::{Literal, Quoted, TypePart},
        Site,
    },
    registry::{fixed, n_, sql},
};

// ---------------------------------------------------------------------------
// pgorm-query: DDL
// ---------------------------------------------------------------------------

pub fn sites() -> Vec<Site> {
    vec![
        // -- CREATE TABLE --------------------------------------------------
        Site {
            id: "ddl/create-table.table",
            api: "Table::create(Name)",
            kinds: &["CreateStmt.relation.relname"],
            policy: Quoted,
            render: |n| sql(Table::create(n_(n)).col(ColumnDef::new(fixed("c")).integer())),
        },
        Site {
            id: "ddl/create-table.schema",
            api: "Table::create((Name, Name)) — the schema part",
            kinds: &["CreateStmt.relation.schemaname"],
            policy: Quoted,
            render: |n| {
                sql(Table::create((n_(n), fixed("t"))).col(ColumnDef::new(fixed("c")).integer()))
            },
        },
        Site {
            id: "ddl/create-table.column",
            api: "ColumnDef::new(Name)",
            kinds: &["ColumnDef.colname"],
            policy: Quoted,
            render: |n| sql(Table::create(fixed("t")).col(ColumnDef::new(n_(n)).integer())),
        },
        Site {
            id: "ddl/create-table.column-type-named",
            api: "ColumnType::named(String)",
            kinds: &["ColumnDef.type_name.names[0]"],
            policy: TypePart,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new_with_type(fixed("c"), ColumnType::named(n))))
            },
        },
        Site {
            id: "ddl/create-table.column-type-schema",
            api: "ColumnDef::named(TypeName::new(type).schema(Name))",
            kinds: &["ColumnDef.type_name.names[0]"],
            policy: TypePart,
            render: |n| {
                sql(Table::create(fixed("t")).col(
                    ColumnDef::new(fixed("c")).named(TypeName::new(fixed("ty")).schema(n_(n))),
                ))
            },
        },
        Site {
            id: "ddl/create-table.column-type-raw-schema",
            api: "ColumnDef::named(TypeName::raw(type).schema(Name))",
            kinds: &["ColumnDef.type_name.names[0]"],
            policy: TypePart,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).named(TypeName::raw("ty").schema(n_(n)))))
            },
        },
        Site {
            id: "ddl/create-table.column-type-array",
            api: "ColumnDef::array(ColumnType::named(String))",
            kinds: &["ColumnDef.type_name.names[0]"],
            policy: TypePart,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).array(ColumnType::named(n))))
            },
        },
        Site {
            id: "ddl/create-table.column-type-enum",
            api: "ColumnDef::enumeration(Name, labels)",
            kinds: &["ColumnDef.type_name.names[0]"],
            policy: TypePart,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).enumeration(n_(n), [fixed("a")])))
            },
        },
        Site {
            id: "ddl/create-table.index-constraint-name",
            api: "TableCreateStatement::index(Index::create().name(Name))",
            kinds: &["Constraint.conname"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .index(
                        Index::create(fixed("t"), fixed("c"))
                            .name(n_(n))
                            .unique()
                            .to_owned(),
                    ))
            },
        },
        Site {
            id: "ddl/create-table.index-constraint-column",
            api: "TableCreateStatement::index(Index::create(table, Name))",
            kinds: &["Constraint.keys[0]"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .index(Index::create(fixed("t"), n_(n)).unique().to_owned()))
            },
        },
        Site {
            id: "ddl/create-table.index-constraint-include",
            api: "TableCreateStatement::index(Index::create().include([Name]))",
            kinds: &["Constraint.including[0]"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .index(
                        Index::create(fixed("t"), fixed("c"))
                            .unique()
                            .include([n_(n)])
                            .to_owned(),
                    ))
            },
        },
        Site {
            id: "ddl/create-table.foreign-key-name",
            api: "TableCreateStatement::foreign_key(ForeignKey::create(..).name(Name))",
            kinds: &["Constraint.conname"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .foreign_key(
                        ForeignKey::create(fixed("t"), fixed("c"), fixed("r"), fixed("rc"))
                            .name(n_(n))
                            .to_owned(),
                    ))
            },
        },
        Site {
            id: "ddl/create-table.foreign-key-column",
            api: "TableCreateStatement::foreign_key(ForeignKey::create(table, Name, ..))",
            kinds: &["Constraint.fk_attrs[0]"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .foreign_key(ForeignKey::create(
                        fixed("t"),
                        n_(n),
                        fixed("r"),
                        fixed("rc"),
                    )))
            },
        },
        Site {
            id: "ddl/create-table.foreign-key-ref-table",
            api: "TableCreateStatement::foreign_key(ForeignKey::create(.., Name, ..))",
            kinds: &["Constraint.pktable.relname"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .foreign_key(ForeignKey::create(
                        fixed("t"),
                        fixed("c"),
                        n_(n),
                        fixed("rc"),
                    )))
            },
        },
        Site {
            id: "ddl/create-table.foreign-key-ref-column",
            api: "TableCreateStatement::foreign_key(ForeignKey::create(.., Name))",
            kinds: &["Constraint.pk_attrs[0]"],
            policy: Quoted,
            render: |n| {
                sql(Table::create(fixed("t"))
                    .col(ColumnDef::new(fixed("c")).integer())
                    .foreign_key(ForeignKey::create(
                        fixed("t"),
                        fixed("c"),
                        fixed("r"),
                        n_(n),
                    )))
            },
        },
        // -- ALTER TABLE ---------------------------------------------------
        Site {
            id: "ddl/alter-table.table",
            api: "Table::alter(Name)",
            kinds: &["AlterTableStmt.relation.relname"],
            policy: Quoted,
            render: |n| sql(&Table::alter(n_(n)).drop_column(fixed("c"))),
        },
        Site {
            id: "ddl/alter-table.add-column",
            api: "TableAlterStatement::add_column(ColumnDef::new(Name))",
            kinds: &["ColumnDef.colname"],
            policy: Quoted,
            render: |n| sql(&Table::alter(fixed("t")).add_column(ColumnDef::new(n_(n)).integer())),
        },
        Site {
            id: "ddl/alter-table.modify-column",
            api: "TableAlterStatement::modify_column(ColumnDef::new(Name))",
            kinds: &["AlterTableCmd.name", "AlterTableCmd.name"],
            policy: Quoted,
            render: |n| {
                sql(&Table::alter(fixed("t"))
                    .modify_column(ColumnDef::new(n_(n)).integer().not_null()))
            },
        },
        Site {
            id: "ddl/alter-table.drop-column",
            api: "TableAlterStatement::drop_column(Name)",
            kinds: &["AlterTableCmd.name"],
            policy: Quoted,
            render: |n| sql(&Table::alter(fixed("t")).drop_column(n_(n))),
        },
        Site {
            id: "ddl/alter-table.drop-foreign-key",
            api: "TableAlterStatement::drop_foreign_key(Name)",
            kinds: &["AlterTableCmd.name"],
            policy: Quoted,
            render: |n| sql(&Table::alter(fixed("t")).drop_foreign_key(n_(n))),
        },
        Site {
            id: "ddl/alter-table.add-foreign-key-name",
            api: "TableAlterStatement::add_foreign_key(TableForeignKey::new(..).name(Name))",
            kinds: &["Constraint.conname"],
            policy: Quoted,
            render: |n| {
                sql(&Table::alter(fixed("t")).add_foreign_key(
                    TableForeignKey::new(fixed("t"), fixed("c"), fixed("r"), fixed("rc"))
                        .name(n_(n))
                        .to_owned(),
                ))
            },
        },
        Site {
            id: "ddl/rename-table.to",
            api: "Table::rename(table, Name)",
            kinds: &["RenameStmt.newname"],
            policy: Quoted,
            render: |n| sql(&Table::rename(fixed("t"), n_(n))),
        },
        Site {
            id: "ddl/rename-table.from",
            api: "Table::rename(Name, to)",
            kinds: &["RenameStmt.relation.relname"],
            policy: Quoted,
            render: |n| sql(&Table::rename(n_(n), fixed("u"))),
        },
        Site {
            id: "ddl/rename-column.from",
            api: "Table::rename_column(table, Name, to)",
            kinds: &["RenameStmt.subname"],
            policy: Quoted,
            render: |n| sql(&Table::rename_column(fixed("t"), n_(n), fixed("d"))),
        },
        Site {
            id: "ddl/rename-column.to",
            api: "Table::rename_column(table, from, Name)",
            kinds: &["RenameStmt.newname"],
            policy: Quoted,
            render: |n| sql(&Table::rename_column(fixed("t"), fixed("c"), n_(n))),
        },
        Site {
            id: "ddl/drop-table.table",
            api: "Table::drop(Name)",
            kinds: &["DropStmt.objects[0].List.items[0]"],
            policy: Quoted,
            render: |n| sql(&Table::drop(n_(n))),
        },
        Site {
            id: "ddl/truncate-table.table",
            api: "Table::truncate(Name)",
            kinds: &["RangeVar.relname"],
            policy: Quoted,
            render: |n| sql(&Table::truncate(n_(n))),
        },
        // -- COMMENT ON ----------------------------------------------------
        Site {
            id: "ddl/comment.table",
            api: "Comment::on_table(Name, text)",
            kinds: &["CommentStmt.object.List.items[0]"],
            policy: Quoted,
            render: |n| sql(&Comment::on_table(n_(n), "x")),
        },
        Site {
            id: "ddl/comment.column",
            api: "Comment::on_column(table, Name, text)",
            kinds: &["CommentStmt.object.List.items[1]"],
            policy: Quoted,
            render: |n| sql(&Comment::on_column(fixed("t"), n_(n), "x")),
        },
        // -- CREATE / DROP INDEX -------------------------------------------
        Site {
            id: "ddl/create-index.name",
            api: "IndexCreateStatement::name(Name)",
            kinds: &["IndexStmt.idxname"],
            policy: Quoted,
            render: |n| sql(Index::create(fixed("t"), fixed("c")).name(n_(n))),
        },
        Site {
            id: "ddl/create-index.table",
            api: "Index::create(Name, column)",
            kinds: &["IndexStmt.relation.relname"],
            policy: Quoted,
            render: |n| sql(Index::create(n_(n), fixed("c")).name(fixed("i"))),
        },
        Site {
            id: "ddl/create-index.column",
            api: "Index::create(table, Name)",
            kinds: &["IndexElem.name"],
            policy: Quoted,
            render: |n| sql(Index::create(fixed("t"), n_(n)).name(fixed("i"))),
        },
        Site {
            id: "ddl/create-index.operator-class",
            api: "IndexColumn::operator_class(Name)",
            kinds: &["IndexElem.opclass[0]"],
            policy: Quoted,
            render: |n| {
                sql(Index::create(
                    fixed("t"),
                    IndexColumn::name(fixed("c")).operator_class(n_(n)),
                )
                .name(fixed("i")))
            },
        },
        Site {
            id: "ddl/create-index.access-method",
            api: "IndexCreateStatement::index_type(IndexType::Named(Name))",
            kinds: &["IndexStmt.access_method"],
            policy: TypePart,
            render: |n| {
                sql(Index::create(fixed("t"), fixed("c"))
                    .name(fixed("i"))
                    .index_type(IndexType::Named(n_(n))))
            },
        },
        Site {
            id: "ddl/create-index.include",
            api: "IndexCreateStatement::include([Name])",
            kinds: &["IndexElem.name"],
            policy: Quoted,
            render: |n| {
                sql(Index::create(fixed("t"), fixed("c"))
                    .name(fixed("i"))
                    .include([n_(n)]))
            },
        },
        Site {
            id: "ddl/drop-index.name",
            api: "Index::drop(Name)",
            kinds: &["DropStmt.objects[0].List.items[0]"],
            policy: Quoted,
            render: |n| sql(&Index::drop(n_(n))),
        },
        Site {
            id: "ddl/drop-index.schema",
            api: "IndexDropStatement::table((Name, table)) — the schema part",
            kinds: &["DropStmt.objects[0].List.items[0]"],
            policy: Quoted,
            render: |n| sql(Index::drop(fixed("i")).table((n_(n), fixed("t")))),
        },
        // -- FOREIGN KEY ---------------------------------------------------
        Site {
            id: "ddl/foreign-key.name",
            api: "ForeignKey::create(..).name(Name)",
            kinds: &["Constraint.conname"],
            policy: Quoted,
            render: |n| {
                sql(ForeignKey::create(fixed("t"), fixed("c"), fixed("r"), fixed("rc")).name(n_(n)))
            },
        },
        Site {
            id: "ddl/foreign-key.table",
            api: "ForeignKey::create(Name, ..)",
            kinds: &["AlterTableStmt.relation.relname"],
            policy: Quoted,
            render: |n| {
                sql(&ForeignKey::create(
                    n_(n),
                    fixed("c"),
                    fixed("r"),
                    fixed("rc"),
                ))
            },
        },
        Site {
            id: "ddl/foreign-key.column",
            api: "ForeignKey::create(table, Name, ..)",
            kinds: &["Constraint.fk_attrs[0]"],
            policy: Quoted,
            render: |n| {
                sql(&ForeignKey::create(
                    fixed("t"),
                    n_(n),
                    fixed("r"),
                    fixed("rc"),
                ))
            },
        },
        Site {
            id: "ddl/foreign-key.ref-table",
            api: "ForeignKey::create(table, column, Name, ..)",
            kinds: &["Constraint.pktable.relname"],
            policy: Quoted,
            render: |n| {
                sql(&ForeignKey::create(
                    fixed("t"),
                    fixed("c"),
                    n_(n),
                    fixed("rc"),
                ))
            },
        },
        Site {
            id: "ddl/foreign-key.ref-column",
            api: "ForeignKey::create(table, column, ref_table, Name)",
            kinds: &["Constraint.pk_attrs[0]"],
            policy: Quoted,
            render: |n| {
                sql(&ForeignKey::create(
                    fixed("t"),
                    fixed("c"),
                    fixed("r"),
                    n_(n),
                ))
            },
        },
        Site {
            id: "ddl/foreign-key-drop.name",
            api: "ForeignKey::drop(table, Name)",
            kinds: &["AlterTableCmd.name"],
            policy: Quoted,
            render: |n| sql(&ForeignKey::drop(fixed("t"), n_(n))),
        },
        Site {
            id: "ddl/foreign-key-drop.table",
            api: "ForeignKey::drop(Name, name)",
            kinds: &["AlterTableStmt.relation.relname"],
            policy: Quoted,
            render: |n| sql(&ForeignKey::drop(n_(n), fixed("k"))),
        },
        // -- TYPE ----------------------------------------------------------
        Site {
            id: "ddl/create-type.name",
            api: "Type::create(Name)",
            kinds: &["CreateEnumStmt.type_name[0]"],
            policy: Quoted,
            render: |n| sql(Type::create(n_(n)).as_enum().values(["a"])),
        },
        Site {
            id: "ddl/create-type.schema",
            api: "Type::create((Name, type)) — the schema part",
            kinds: &["CreateEnumStmt.type_name[0]"],
            policy: Quoted,
            render: |n| sql(Type::create((n_(n), fixed("ty"))).as_enum().values(["a"])),
        },
        Site {
            id: "ddl/create-type.database",
            api: "Type::create((Name, schema, type)) — the database part",
            kinds: &["CreateEnumStmt.type_name[0]"],
            policy: Quoted,
            render: |n| {
                sql(Type::create((n_(n), fixed("s"), fixed("ty")))
                    .as_enum()
                    .values(["a"]))
            },
        },
        Site {
            id: "ddl/create-type.label",
            api: "TypeCreateStatement::values([String]) — enum labels are values",
            kinds: &["CreateEnumStmt.vals[0]"],
            policy: Literal,
            render: |n| sql(Type::create(fixed("ty")).as_enum().values([n])),
        },
        Site {
            id: "ddl/drop-type.name",
            api: "Type::drop(Name)",
            kinds: &["TypeName.names[0]"],
            policy: Quoted,
            render: |n| sql(&Type::drop(n_(n))),
        },
        Site {
            id: "ddl/alter-type.name",
            api: "Type::alter(Name)",
            kinds: &["AlterEnumStmt.type_name[0]"],
            policy: Quoted,
            render: |n| sql(&Type::alter(n_(n)).add_value("a")),
        },
        Site {
            id: "ddl/alter-type.rename-to",
            api: "PendingTypeAlter::rename_to(Name)",
            kinds: &["RenameStmt.newname"],
            policy: Quoted,
            render: |n| sql(&Type::alter(fixed("ty")).rename_to(n_(n))),
        },
        Site {
            id: "ddl/alter-type.add-value",
            api: "PendingTypeAlter::add_value(String) — a label, a value",
            kinds: &["AlterEnumStmt.new_val"],
            policy: Literal,
            render: |n| sql(&Type::alter(fixed("ty")).add_value(n)),
        },
        Site {
            id: "ddl/alter-type.add-value-before",
            api: "TypeAlterStatement::before(String) — a label, a value",
            kinds: &["AlterEnumStmt.new_val_neighbor"],
            policy: Literal,
            render: |n| sql(&Type::alter(fixed("ty")).add_value("a").before(n)),
        },
        Site {
            id: "ddl/alter-type.rename-value-existing",
            api: "PendingTypeAlter::rename_value(String, new) — a label, a value",
            kinds: &["AlterEnumStmt.old_val"],
            policy: Literal,
            render: |n| sql(&Type::alter(fixed("ty")).rename_value(n, "b")),
        },
        Site {
            id: "ddl/alter-type.rename-value-new",
            api: "PendingTypeAlter::rename_value(existing, String) — a label, a value",
            kinds: &["AlterEnumStmt.new_val"],
            policy: Literal,
            render: |n| sql(&Type::alter(fixed("ty")).rename_value("a", n)),
        },
        // -- EXTENSION -----------------------------------------------------
        Site {
            id: "ddl/create-extension.name",
            api: "Extension::create(Name)",
            kinds: &["CreateExtensionStmt.extname"],
            policy: Quoted,
            render: |n| sql(&Extension::create(n_(n))),
        },
        Site {
            id: "ddl/create-extension.schema",
            api: "ExtensionCreateStatement::schema(Name)",
            kinds: &["DefElem.arg"],
            policy: Quoted,
            render: |n| sql(Extension::create(fixed("e")).schema(n_(n))),
        },
        Site {
            id: "ddl/drop-extension.name",
            api: "Extension::drop(Name)",
            kinds: &["DropStmt.objects[0]"],
            policy: Quoted,
            render: |n| sql(&Extension::drop(n_(n))),
        },
    ]
}
