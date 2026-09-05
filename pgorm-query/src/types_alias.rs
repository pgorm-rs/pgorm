//! The [`AliasName`] token: a query-introduced name minted once.

use super::*;

/// A name the query introduces, minted once and referred to by value.
///
/// `alias("rn")` binds a `Copy` token that is both the declaration site and
/// every reference site of an introduced name, so the name exists in the
/// program exactly once and a typo cannot make a reference miss its
/// declaration. It is an ordinary [`Iden`], so every position that accepts
/// an identifier — `IntoIden`, `IntoColumnRef`, a projection alias, an
/// `ORDER BY` key — accepts the token with no conversion.
///
/// The name is `&'static str` by construction: an introduced name is part of
/// the shape of the query, not a runtime value. [`Alias`] remains for names
/// computed at runtime.
///
/// ```
/// use pgorm_query::{Iden, alias};
///
/// let rn = alias("rn");
/// assert_eq!(Iden::to_string(&rn), "rn");
/// ```
// [spec:pgorm:def:sql.types+3]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AliasName(&'static str);

/// Mint an [`AliasName`] token for a name the query introduces.
// [spec:pgorm:def:sql.types+3]
pub const fn alias(name: &'static str) -> AliasName {
    AliasName(name)
}

impl AliasName {
    /// The name, as written.
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

// [spec:pgorm:def:sql.types+3]
impl Iden for AliasName {
    fn unquoted(&self, s: &mut dyn fmt::Write) {
        write!(s, "{}", self.0).unwrap();
    }
}

// [spec:pgorm:def:sql.types+3]
impl IdenStatic for AliasName {
    fn as_str(&self) -> &'static str {
        self.0
    }
}

// [spec:pgorm:def:sql.types+3]
impl From<&'static str> for AliasName {
    fn from(name: &'static str) -> Self {
        AliasName(name)
    }
}

impl fmt::Display for AliasName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
