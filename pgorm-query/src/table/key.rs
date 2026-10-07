//! A table's keys: the primary key and the unique keys, each a tuple of one
//! or more columns.

use crate::{Deferrability, IntoName, Name};
use std::marker::PhantomData;

/// A primary or unique key of a table: a non-empty, ordered tuple of columns,
/// with the options a key constraint takes.
///
/// PostgreSQL's own model is the one here: a primary key is a unique key over
/// columns that are never null, which the table calls *the* key, and either is
/// one column or several. So there is one key type, and the kind is its
/// parameter — [`Primary`] for the slot
/// [`TableCreateStatement::primary_key`](crate::TableCreateStatement::primary_key)
/// fills, [`Unique`] for the list
/// [`TableCreateStatement::unique`](crate::TableCreateStatement::unique)
/// appends to. A key is declared on the table and only there: a column has no
/// key clause of its own, so a table cannot be written with two primary keys
/// (`42P16`).
///
/// One column, or a tuple of up to twelve, converts into a key wherever one is
/// taken ([`IntoTableKey`]); [`TableKey::new`] starts the builder for a key that
/// takes options or whose columns are a computed list.
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::create(Glyph::Table)
///     .col(ColumnDef::new(Glyph::Id).integer().not_null())
///     .col(ColumnDef::new(Glyph::Aspect).integer().not_null())
///     .col(ColumnDef::new(Glyph::Image).text())
///     .primary_key((Glyph::Id, Glyph::Aspect))
///     .unique(
///         TableKey::new(Glyph::Image)
///             .name(Name::runtime("glyph_image"))
///             .nulls_not_distinct(),
///     )
///     .to_owned();
///
/// assert_eq!(
///     table.to_string(),
///     [
///         r#"CREATE TABLE "glyph" ("#,
///         r#""id" integer NOT NULL,"#,
///         r#""aspect" integer NOT NULL,"#,
///         r#""image" text,"#,
///         r#"PRIMARY KEY ("id", "aspect"),"#,
///         r#"CONSTRAINT "glyph_image" UNIQUE NULLS NOT DISTINCT ("image")"#,
///         r#")"#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// `NULLS NOT DISTINCT` is a unique key's alone — PostgreSQL refuses it on a
/// primary key (`42601`), whose columns hold no null to compare — so a primary
/// key has no method to ask for it:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::create(Glyph::Table).primary_key(TableKey::<Primary>::new(Glyph::Id).nulls_not_distinct());
/// ```
///
/// A key is never empty: it starts at its first column, and a computed list
/// extends it with [`cols`](TableKey::cols).
///
/// ```compile_fail,E0061
/// use pgorm_query::*;
///
/// TableKey::<Unique>::new();
/// ```
///
/// Nor does a key render on its own — no `Display` and no build path —
/// because PostgreSQL spells one only inside the table it constrains:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// TableKey::<Unique>::new(Glyph::Id).to_string();
/// ```
// [spec:pgorm:req:sql.ddl.create-table+13]
#[derive(Debug, Clone)]
pub struct TableKey<K> {
    pub(crate) name: Option<Name>,
    pub(crate) columns: Vec<Name>,
    pub(crate) include: Vec<Name>,
    pub(crate) deferrability: Option<Deferrability>,
    pub(crate) nulls_not_distinct: bool,
    kind: PhantomData<K>,
}

/// The kind of the table's one primary key: [`TableKey<Primary>`].
// [spec:pgorm:req:sql.ddl.create-table+13]
#[derive(Debug, Clone, Copy)]
pub struct Primary;

/// The kind of a unique key, of which a table has any number:
/// [`TableKey<Unique>`].
// [spec:pgorm:req:sql.ddl.create-table+13]
#[derive(Debug, Clone, Copy)]
pub struct Unique;

impl<K> TableKey<K> {
    /// A key over `column`, the first of its columns; [`col`](Self::col) and
    /// [`cols`](Self::cols) add the rest.
    pub fn new<C>(column: C) -> Self
    where
        C: IntoName,
    {
        Self {
            name: None,
            columns: vec![column.into_name()],
            include: Vec::new(),
            deferrability: None,
            nulls_not_distinct: false,
            kind: PhantomData,
        }
    }

    /// Add a further key column.
    #[must_use]
    pub fn col<C>(mut self, column: C) -> Self
    where
        C: IntoName,
    {
        self.columns.push(column.into_name());
        self
    }

    /// Add every column of `columns` after the key's own, in order: the
    /// spelling of a key whose columns are computed. The key already holds
    /// its first column, so an empty list leaves it as it was.
    #[must_use]
    pub fn cols<C, I>(mut self, columns: I) -> Self
    where
        C: IntoName,
        I: IntoIterator<Item = C>,
    {
        self.columns
            .extend(columns.into_iter().map(IntoName::into_name));
        self
    }

    /// Name the constraint: `CONSTRAINT "name" ...`. Unnamed, PostgreSQL
    /// derives one.
    #[must_use]
    pub fn name<N>(mut self, name: N) -> Self
    where
        N: IntoName,
    {
        self.name = Some(name.into_name());
        self
    }

    /// Carry further columns in the key's index without making them part of
    /// the key — `INCLUDE (…)`. Repeated calls append.
    #[must_use]
    pub fn include<N, I>(mut self, columns: I) -> Self
    where
        N: IntoName,
        I: IntoIterator<Item = N>,
    {
        self.include
            .extend(columns.into_iter().map(IntoName::into_name));
        self
    }

    /// Run the key's check when `deferrability` says
    /// (`[spec:pgorm:req:sql.ddl.deferrability+4]`), replacing any already
    /// set.
    // [spec:pgorm:req:sql.ddl.deferrability+4]
    #[must_use]
    pub fn deferrability(mut self, deferrability: Deferrability) -> Self {
        self.deferrability = Some(deferrability);
        self
    }

    /// The constraint's name, if it was given one.
    pub fn get_name(&self) -> Option<&Name> {
        self.name.as_ref()
    }

    /// The key columns, in order. Never empty.
    pub fn get_columns(&self) -> &[Name] {
        &self.columns
    }

    /// The `INCLUDE` columns, in order.
    pub fn get_include(&self) -> &[Name] {
        &self.include
    }

    /// When the key's check runs, if the caller said.
    pub fn get_deferrability(&self) -> Option<Deferrability> {
        self.deferrability
    }
}

impl TableKey<Unique> {
    /// Treat nulls as equal: `UNIQUE NULLS NOT DISTINCT (…)`, under which at
    /// most one row may hold a null where a plain unique key admits any
    /// number.
    #[must_use]
    pub fn nulls_not_distinct(mut self) -> Self {
        self.nulls_not_distinct = true;
        self
    }

    /// Whether nulls are equal under this key.
    pub fn is_nulls_not_distinct(&self) -> bool {
        self.nulls_not_distinct
    }
}

/// One column, or a tuple of one to twelve columns: the non-empty, ordered
/// column list that a [`TableKey`] and an `ON CONFLICT` target are both
/// written as.
///
/// A tuple is the columns in order, so `(Glyph::Id, Glyph::Aspect)` is
/// `("id", "aspect")` wherever it is taken —
/// [`TableCreateStatement::primary_key`](crate::TableCreateStatement::primary_key)
/// through [`IntoTableKey`], and
/// [`OnConflict::columns`](crate::OnConflict::columns). There is no impl for an
/// empty tuple, a slice or a `Vec`, which could be empty, and the conversion
/// hands back the first column apart from the rest, so no impl can produce an
/// empty list either: `PRIMARY KEY ()` and `ON CONFLICT ()` are both syntax
/// errors. A computed list starts at its first column and extends from there,
/// with [`TableKey::cols`] or
/// [`ConflictTarget::and_columns`](crate::ConflictTarget::and_columns).
///
/// ```compile_fail,E0277
/// use pgorm_query::*;
///
/// Table::create(Name::runtime("t")).primary_key(());
/// ```
// [spec:pgorm:req:sql.ddl.create-table+13]
// [spec:pgorm:req:sql.ast.on-conflict+3]
pub trait IntoKeyColumns {
    /// The first column, and the rest in order.
    fn into_key_columns(self) -> (Name, Vec<Name>);
}

impl<C> IntoKeyColumns for C
where
    C: IntoName,
{
    fn into_key_columns(self) -> (Name, Vec<Name>) {
        (self.into_name(), Vec::new())
    }
}

/// A value that converts into a [`TableKey`] of kind `K`: any
/// [`IntoKeyColumns`] — one column or a tuple of one to twelve — or a key
/// already built, which is how a key carrying a name or options is passed.
// [spec:pgorm:req:sql.ddl.create-table+13]
pub trait IntoTableKey<K> {
    /// The key.
    fn into_table_key(self) -> TableKey<K>;
}

impl<K> IntoTableKey<K> for TableKey<K> {
    fn into_table_key(self) -> TableKey<K> {
        self
    }
}

impl<K, T> IntoTableKey<K> for T
where
    T: IntoKeyColumns,
{
    fn into_table_key(self) -> TableKey<K> {
        let (first, rest) = self.into_key_columns();
        TableKey::new(first).cols(rest)
    }
}

macro_rules! impl_into_key_columns {
    ( $C0:ident : $N0:tt $(, $C:ident : $N:tt)* $(,)? ) => {
        impl<$C0 $(, $C)*> IntoKeyColumns for ( $C0, $($C,)* )
        where
            $C0: IntoName,
            $($C: IntoName),*
        {
            fn into_key_columns(self) -> (Name, Vec<Name>) {
                (self.$N0.into_name(), vec![ $(self.$N.into_name()),* ])
            }
        }
    };
}

#[rustfmt::skip]
mod impl_into_key_columns {
    use super::*;

    impl_into_key_columns!(C0:0);
    impl_into_key_columns!(C0:0, C1:1);
    impl_into_key_columns!(C0:0, C1:1, C2:2);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5, C6:6);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5, C6:6, C7:7);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5, C6:6, C7:7, C8:8);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5, C6:6, C7:7, C8:8, C9:9);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5, C6:6, C7:7, C8:8, C9:9, C10:10);
    impl_into_key_columns!(C0:0, C1:1, C2:2, C3:3, C4:4, C5:5, C6:6, C7:7, C8:8, C9:9, C10:10, C11:11);
}
