use crate::{FromQueryResult, SelectColumns};

/// A trait for a part of [Model](super::model::ModelTrait)
// [spec:pgorm:def:entity.traits.from-query-result]
pub trait PartialModelTrait: FromQueryResult {
    /// Select specific columns this partial model needs
    fn select_cols<S: SelectColumns>(select: S) -> S;
}
