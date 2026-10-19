//! `CREATE SCHEMA`, `DROP SCHEMA` and `ALTER SCHEMA ... RENAME TO`.

use crate::{DropBehavior, IntoName, Name, QueryBuilder};

/// Helper for constructing any statement over a schema, the namespace a
/// table, type or sequence is qualified by
///
/// A schema is named by one identifier, never qualified: schemas do not nest,
/// and the grammar refuses a dotted schema name. Each name is an
/// [`IntoName`], rendered as a quoted identifier.
///
/// `pgorm::Schema`, the root crate's generator of an entity's DDL, is a
/// different type; a file that imports both writes one by its path.
// [spec:pgorm:req:sql.ddl.schema]
#[derive(Debug, Clone, Copy)]
pub struct Schema;

impl Schema {
    /// Construct a [`SchemaCreateStatement`] over the schema it creates
    pub fn create<T>(name: T) -> SchemaCreateStatement
    where
        T: IntoName,
    {
        SchemaCreateStatement {
            name: name.into_name(),
            if_not_exists: false,
        }
    }

    /// Construct a [`SchemaDropStatement`] over the first schema it drops
    pub fn drop<T>(name: T) -> SchemaDropStatement
    where
        T: IntoName,
    {
        SchemaDropStatement {
            first: name.into_name(),
            rest: Vec::new(),
            if_exists: false,
            behavior: None,
        }
    }

    /// Construct a [`SchemaRenameStatement`] from the old and new name
    pub fn rename<T, R>(from_name: T, to_name: R) -> SchemaRenameStatement
    where
        T: IntoName,
        R: IntoName,
    {
        SchemaRenameStatement {
            from_name: from_name.into_name(),
            to_name: to_name.into_name(),
        }
    }
}

/// Create a schema: `CREATE SCHEMA [IF NOT EXISTS] "name"`.
///
/// The name is taken by [`Schema::create`], `CREATE SCHEMA` without one being
/// a syntax error. Like the other DDL statements it runs inside a transaction
/// and is undone with it.
///
/// Two clauses of PostgreSQL's are not built. `AUTHORIZATION` names the role
/// that owns the schema, and roles are outside pgorm-query's scope with
/// `GRANT` and `REVOKE`. The list of statements that may follow the name
/// creates objects inside the new schema, each of which a builder already
/// renders qualified by the schema's name; PostgreSQL refuses that list beside
/// `IF NOT EXISTS` (`0A000`), which is the clause a migration wants.
///
/// What the server refuses, no builder value can see: a schema that already
/// exists (`42P06`, a notice under [`if_not_exists`](Self::if_not_exists)),
/// and a name starting `pg_`, which PostgreSQL reserves for its own schemas
/// (`42939`).
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Schema::create(Name::runtime("billing")).if_not_exists().to_string(),
///     r#"CREATE SCHEMA IF NOT EXISTS "billing""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.schema]
#[derive(Debug, Clone)]
pub struct SchemaCreateStatement {
    pub(crate) name: Name,
    pub(crate) if_not_exists: bool,
}

impl SchemaCreateStatement {
    /// `IF NOT EXISTS`: a schema of that name already there is a notice
    /// rather than an error, and is left as it is.
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }

    /// The schema created
    pub fn get_name(&self) -> &Name {
        &self.name
    }
}

/// Drop one or more schemas: `DROP SCHEMA [IF EXISTS] "a"[, "b"...][ CASCADE |
/// RESTRICT]`.
///
/// The first name is taken by [`Schema::drop`] and every further one is
/// appended, so the list is non-empty: PostgreSQL rejects `DROP SCHEMA ` at
/// end of input. A schema holding anything — a table, a type, a sequence — is
/// refused under `RESTRICT`, which is also what an unsaid behavior means
/// (`2BP01`); `CASCADE` drops what it holds with it, and whatever depends on
/// that in turn, in any schema. A missing schema is refused with `3F000`,
/// a notice under [`if_exists`](Self::if_exists).
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Schema::drop(Name::runtime("billing"))
///         .name(Name::runtime("archive"))
///         .if_exists()
///         .cascade()
///         .to_string(),
///     r#"DROP SCHEMA IF EXISTS "billing", "archive" CASCADE"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.schema]
#[derive(Debug, Clone)]
pub struct SchemaDropStatement {
    pub(crate) first: Name,
    pub(crate) rest: Vec<Name>,
    pub(crate) if_exists: bool,
    pub(crate) behavior: Option<DropBehavior>,
}

impl SchemaDropStatement {
    /// Drop a further schema
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoName,
    {
        self.rest.push(name.into_name());
        self
    }

    /// `IF EXISTS`: a missing schema is a notice rather than an error.
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// `CASCADE`: drop everything in the schema too, and what depends on it.
    /// The last of this and [`restrict`](Self::restrict) wins.
    pub fn cascade(&mut self) -> &mut Self {
        self.behavior = Some(DropBehavior::Cascade);
        self
    }

    /// `RESTRICT`: refuse to drop a schema that holds anything (`2BP01`),
    /// which is also what an unsaid behavior does.
    pub fn restrict(&mut self) -> &mut Self {
        self.behavior = Some(DropBehavior::Restrict);
        self
    }

    /// The schemas dropped, in declaration order, of which there is at least
    /// one
    pub fn names(&self) -> impl Iterator<Item = &Name> {
        std::iter::once(&self.first).chain(&self.rest)
    }
}

/// Rename a schema: `ALTER SCHEMA "from" RENAME TO "to"`.
///
/// Both names are taken by [`Schema::rename`]. What the schema holds moves
/// with it, so a statement or entity that names the old schema stops finding
/// its objects. PostgreSQL has no `IF EXISTS` for this statement (`42601`),
/// so the builder has none either:
///
/// ```compile_fail,E0599
/// use pgorm_query::*;
///
/// Schema::rename(Name::runtime("a"), Name::runtime("b")).if_exists();
/// ```
///
/// The server refuses a missing schema (`3F000`), a new name already taken
/// (`42P06`) and a new name starting `pg_` (`42939`).
///
/// `ALTER SCHEMA ... OWNER TO`, the statement's other form, is a role change
/// and is not built, as `AUTHORIZATION` is not.
///
/// ```
/// use pgorm_query::*;
///
/// assert_eq!(
///     Schema::rename(Name::runtime("billing"), Name::runtime("invoicing")).to_string(),
///     r#"ALTER SCHEMA "billing" RENAME TO "invoicing""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.schema]
#[derive(Debug, Clone)]
pub struct SchemaRenameStatement {
    pub(crate) from_name: Name,
    pub(crate) to_name: Name,
}

macro_rules! impl_schema_display {
    ( $( $struct_name:ident => $func_name:ident ),* $(,)? ) => {
        $(
            /// Renders the statement, its only rendering: a schema statement
            /// carries names and nothing to bind, so there is no `build`.
            // [spec:pgorm:req:sql.ddl+12]
            impl std::fmt::Display for $struct_name {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let mut sql = String::with_capacity(64);
                    QueryBuilder.$func_name(self, &mut sql);
                    f.write_str(&sql)
                }
            }
        )*
    };
}

impl_schema_display!(
    SchemaCreateStatement => prepare_schema_create_statement,
    SchemaDropStatement => prepare_schema_drop_statement,
    SchemaRenameStatement => prepare_schema_rename_statement,
);
