//! Verification for the vendored strum `EnumIter` derive.

#![allow(dead_code)]

use pgorm::strum::IntoEnumIterator;
use pgorm_macros::EnumIter;

#[derive(Debug, PartialEq, EnumIter)]
enum Simple {
    A,
    B,
    C,
}

/// Variants carrying data are constructed with `Default::default()` for every
/// field, whether the fields are named or positional.
#[derive(Debug, PartialEq, EnumIter)]
enum WithData {
    Unit,
    Tuple(i32, String),
    Named { flag: bool, count: u8 },
}

/// `#[strum(disabled)]` drops a variant from iteration entirely. This is the
/// mechanism `table_iden` uses to keep the generated `Table` column variant out
/// of `Column::iter()`.
#[derive(Debug, PartialEq, EnumIter)]
enum WithDisabled {
    First,
    #[strum(disabled)]
    Hidden,
    Last,
}

/// Type parameters are supported: the generated iterator carries a
/// `PhantomData` marker for them.
#[derive(Debug, PartialEq, EnumIter)]
enum Generic<T: Default> {
    Value(T),
    Nothing,
}

// [spec:pgorm:sem:macros.derive.enum-iter/test]    the EIter struct and IntoEnumIterator
#[test]
fn enum_iter_generates_a_named_iterator_type() {
    // The generated type is named `{Enum}Iter` and is what
    // `IntoEnumIterator::iter()` returns.
    let iter: SimpleIter = Simple::iter();
    assert_eq!(
        iter.collect::<Vec<_>>(),
        vec![Simple::A, Simple::B, Simple::C]
    );

    fn assert_into_enum_iterator<E: IntoEnumIterator>() {}
    assert_into_enum_iterator::<Simple>();
}

// [spec:pgorm:sem:macros.derive.enum-iter/test]    the trait set on the iterator
#[test]
fn the_iterator_implements_the_full_trait_set() {
    fn assert_fused<I: std::iter::FusedIterator>(_: &I) {}

    let iter = Simple::iter();
    assert_fused(&iter);

    // ExactSizeIterator
    assert_eq!(iter.len(), 3);
    // Clone
    let cloned = iter.clone();
    assert_eq!(cloned.count(), 3);
    // Debug
    assert_eq!(format!("{:?}", Simple::iter()), "SimpleIter { len: 3 }");
    // DoubleEndedIterator
    assert_eq!(
        Simple::iter().rev().collect::<Vec<_>>(),
        vec![Simple::C, Simple::B, Simple::A]
    );

    // A fused iterator keeps returning `None` once exhausted.
    let mut done = Simple::iter();
    for _ in 0..3 {
        assert!(done.next().is_some());
    }
    assert!(done.next().is_none());
    assert!(done.next().is_none());
}

// [spec:pgorm:sem:macros.derive.enum-iter/test]    data-carrying variants
#[test]
fn variants_with_data_are_built_from_default() {
    assert_eq!(
        WithData::iter().collect::<Vec<_>>(),
        vec![
            WithData::Unit,
            WithData::Tuple(0, String::new()),
            WithData::Named {
                flag: false,
                count: 0
            },
        ]
    );
}

// [spec:pgorm:sem:macros.derive.enum-iter/test]    #[strum(disabled)]
#[test]
fn disabled_variants_are_skipped_entirely() {
    assert_eq!(
        WithDisabled::iter().collect::<Vec<_>>(),
        vec![WithDisabled::First, WithDisabled::Last]
    );
    // The count reflects the skip, not just the yielded items.
    assert_eq!(WithDisabled::iter().len(), 2);
}

// [spec:pgorm:sem:macros.derive.enum-iter/test]    generic enums
#[test]
fn type_parameters_are_supported() {
    assert_eq!(
        Generic::<i32>::iter().collect::<Vec<_>>(),
        vec![Generic::Value(0), Generic::Nothing]
    );
    assert_eq!(
        Generic::<String>::iter().collect::<Vec<_>>(),
        vec![Generic::Value(String::new()), Generic::Nothing]
    );
}
