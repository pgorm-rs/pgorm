//! The type a value of a domain travels as.

use postgres_types::{Kind, Type};

/// The type `ty`'s values are sent and read in: `ty` with its domains looked
/// through, as PostgreSQL's own `getBaseType` does.
///
/// A domain's values travel in the representation of the type it is over,
/// so a domain — a domain over a domain included, however deep — is the type
/// at the bottom, and any other type is `ty` itself. Only `ty` is looked
/// through: an array, range or multirange of a domain is its own type, so it
/// is returned as it is, and a caller reading one asks this of the element or
/// subtype it reaches.
///
/// ```
/// use pgorm_query::base_type;
/// use postgres_types::{Kind, Type};
///
/// let domain = |name: &str, base: Type| {
///     Type::new(name.to_owned(), 16_384, Kind::Domain(base), "public".to_owned())
/// };
/// let quantity = domain("quantity", Type::INT4);
/// let small = domain("small_quantity", quantity.clone());
/// assert_eq!(*base_type(&small), Type::INT4);
/// assert_eq!(*base_type(&quantity), Type::INT4);
/// assert_eq!(*base_type(&Type::TEXT), Type::TEXT);
///
/// let quantities = Type::new(
///     "_quantity".to_owned(),
///     16_385,
///     Kind::Array(quantity),
///     "public".to_owned(),
/// );
/// assert_eq!(*base_type(&quantities), quantities);
/// ```
// [spec:pgorm:def:sql.value.base-type]
pub fn base_type(ty: &Type) -> &Type {
    let mut ty = ty;
    while let Kind::Domain(base) = ty.kind() {
        ty = base;
    }
    ty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, kind: Kind) -> Type {
        Type::new(name.to_owned(), 16_384, kind, "public".to_owned())
    }

    // [spec:pgorm:def:sql.value.base-type/test]    a domain over a domain is the type at the
    // bottom, any other type is the very type handed in, and an array or range of a domain
    // is not looked into
    #[test]
    fn looks_through_domains_and_nothing_else() {
        let span = named("span", Kind::Domain(Type::INTERVAL));
        let short = named("short_span", Kind::Domain(span.clone()));
        assert_eq!(*base_type(&short), Type::INTERVAL);
        assert_eq!(*base_type(&span), Type::INTERVAL);

        let text = Type::TEXT;
        assert!(std::ptr::eq(base_type(&text), &text));

        let spans = named("_span", Kind::Array(span.clone()));
        assert!(std::ptr::eq(base_type(&spans), &spans));
        let range = named("span_range", Kind::Range(span));
        assert!(std::ptr::eq(base_type(&range), &range));

        let over_array = named("spans", Kind::Domain(spans.clone()));
        assert_eq!(*base_type(&over_array), spans);
    }
}
