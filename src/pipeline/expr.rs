//! Branded scalar expressions and the operators that combine them.

use std::marker::PhantomData;

use pgorm_query::Iden;

use super::adapter::{self, BinOp, PlExpr, UnOp};
use super::funcs::CastType;

/// A scalar expression, branded with the lifetime of the pipeline stage that
/// is allowed to consume it.
///
/// Expressions built purely from names and literals ([`col`], [`out`], the
/// constructors in this module) are brand-polymorphic and can appear in any
/// pipeline. An expression that contains a bound placeholder — anything
/// touched by [`Binder::bind`](super::Binder::bind) — is pinned to the brand
/// of the binder that minted it, so carrying it into another pipeline (whose
/// stages quantify over a fresh brand) does not typecheck.
// [spec:pgorm:req:pipeline.params]
#[derive(Debug, Clone)]
pub struct Expr<'brand> {
    pub(super) node: PlExpr,
    pub(super) _brand: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

pub(super) fn branded<'brand>(node: PlExpr) -> Expr<'brand> {
    Expr {
        node,
        _brand: PhantomData,
    }
}

/// A table-qualified column reference.
///
/// Qualification is not optional: prqlc has no catalog, so a bare column name
/// becomes ambiguous the moment a join enters the pipeline. Minting the
/// reference from a `(table, column)` [`Iden`] pair — entity and column enums
/// in the common case — makes the qualified form the only representable one.
// [spec:pgorm:sem:pipeline.qualify]
pub fn col<'brand>(table: impl Iden, column: impl Iden) -> Expr<'brand> {
    branded(adapter::ident_in(
        vec![Iden::to_string(&table)],
        Iden::to_string(&column),
    ))
}

/// A reference to a name the pipeline itself introduced — a `derive`,
/// `aggregate` or window alias. For table columns use [`col`].
pub fn out<'brand>(name: &str) -> Expr<'brand> {
    branded(adapter::ident(name))
}

impl<'brand> Expr<'brand> {
    fn bin(self, op: BinOp, rhs: Expr<'brand>) -> Expr<'brand> {
        branded(adapter::binary(self.node, op, rhs.node))
    }

    /// `=` (or `IS NULL` when compared against [`null`](super::null)).
    pub fn eq(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Eq, rhs)
    }

    /// `<>` (or `IS NOT NULL` against [`null`](super::null)).
    pub fn ne(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Ne, rhs)
    }

    /// `>`
    pub fn gt(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Gt, rhs)
    }

    /// `>=`
    pub fn gte(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Gte, rhs)
    }

    /// `<`
    pub fn lt(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Lt, rhs)
    }

    /// `<=`
    pub fn lte(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Lte, rhs)
    }

    /// Logical `AND`.
    pub fn and(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::And, rhs)
    }

    /// Logical `OR`.
    pub fn or(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Or, rhs)
    }

    /// `COALESCE(self, rhs)`.
    pub fn coalesce(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Coalesce, rhs)
    }

    /// `IS NULL`.
    pub fn is_null(self) -> Expr<'brand> {
        self.bin(BinOp::Eq, branded(adapter::lit_null()))
    }

    /// `IS NOT NULL`.
    pub fn is_not_null(self) -> Expr<'brand> {
        self.bin(BinOp::Ne, branded(adapter::lit_null()))
    }

    /// Membership in an explicit list: `self IN (items...)`.
    ///
    /// Each item is an expression, so the members can be bound placeholders.
    pub fn in_array(self, items: Vec<Expr<'brand>>) -> Expr<'brand> {
        let items = items.into_iter().map(|item| item.node).collect();
        branded(adapter::call("in", vec![adapter::array(items), self.node]))
    }

    /// `CAST(self AS type)`, over the closed [`CastType`] set.
    pub fn cast(self, ty: CastType) -> Expr<'brand> {
        branded(adapter::call(
            "as",
            vec![adapter::ident(ty.name()), self.node],
        ))
    }

    /// Name this expression in a `derive`, `select`, `aggregate` or window
    /// projection.
    ///
    /// The name must not collide with a PRQL built-in;
    /// [`into_sql`](super::Pipeline::into_sql) refuses the closed reserved set
    /// with [`PipelineError::ReservedAlias`](super::PipelineError::ReservedAlias).
    pub fn aliased(self, alias: &str) -> Expr<'brand> {
        branded(adapter::aliased(self.node, alias.to_owned()))
    }

    /// Mark a sort key descending.
    pub fn desc(self) -> Expr<'brand> {
        branded(adapter::unary(UnOp::Neg, self.node))
    }

    /// Mark a sort key ascending (the default; provided for symmetry).
    pub fn asc(self) -> Expr<'brand> {
        branded(adapter::unary(UnOp::Add, self.node))
    }
}

impl<'brand> std::ops::Add for Expr<'brand> {
    type Output = Expr<'brand>;
    fn add(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Add, rhs)
    }
}

impl<'brand> std::ops::Sub for Expr<'brand> {
    type Output = Expr<'brand>;
    fn sub(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Sub, rhs)
    }
}

impl<'brand> std::ops::Mul for Expr<'brand> {
    type Output = Expr<'brand>;
    fn mul(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Mul, rhs)
    }
}

impl<'brand> std::ops::Div for Expr<'brand> {
    type Output = Expr<'brand>;
    fn div(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::DivFloat, rhs)
    }
}

impl<'brand> std::ops::Rem for Expr<'brand> {
    type Output = Expr<'brand>;
    fn rem(self, rhs: Expr<'brand>) -> Expr<'brand> {
        self.bin(BinOp::Mod, rhs)
    }
}

impl<'brand> std::ops::Neg for Expr<'brand> {
    type Output = Expr<'brand>;
    fn neg(self) -> Expr<'brand> {
        branded(adapter::unary(UnOp::Neg, self.node))
    }
}

impl<'brand> std::ops::Not for Expr<'brand> {
    type Output = Expr<'brand>;
    fn not(self) -> Expr<'brand> {
        branded(adapter::unary(UnOp::Not, self.node))
    }
}
