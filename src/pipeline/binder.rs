//! The parameter binder: placeholders and values are minted as one.

use std::marker::PhantomData;

use pgorm_query::Value;

use super::adapter;
use super::expr::{Expr, branded};

/// Mints `$N` placeholders against the values the pipeline will carry.
///
/// A binder is only ever handed to the closures the
/// [`Pipeline`](super::Pipeline) transforms take, branded with a fresh
/// lifetime per call. [`bind`](Binder::bind) pushes the value and returns the
/// placeholder in one step, so a placeholder without its value — or a value
/// without its placeholder — cannot be constructed, and the brand pins the
/// resulting expression to this pipeline:
///
/// ```compile_fail
/// use pgorm::pgorm_query::Alias;
/// use pgorm::pipeline::{Expr, Pipeline, col};
///
/// let mut smuggled: Option<Expr<'static>> = None;
/// let _ = Pipeline::from(Alias::new("cake"))
///     .filter(|binder| {
///         let bound = binder.bind(1_i32);
///         smuggled = Some(bound.clone());
///         col(Alias::new("cake"), Alias::new("id")).eq(bound)
///     });
/// ```
///
/// The same pipeline without the escape compiles: drop the `smuggled`
/// assignment and the closure is an ordinary bound filter.
///
/// ```
/// use pgorm::pgorm_query::Alias;
/// use pgorm::pipeline::{Pipeline, col};
///
/// let (sql, values) = Pipeline::from(Alias::new("cake"))
///     .filter(|binder| {
///         let bound = binder.bind(1_i32);
///         col(Alias::new("cake"), Alias::new("id")).eq(bound)
///     })
///     .into_sql()
///     .expect("the pipeline resolves");
/// assert_eq!(sql, "SELECT * FROM cake WHERE id = $1");
/// assert_eq!(values.0.len(), 1);
/// ```
// [spec:pgorm:req:pipeline.params]
#[derive(Debug)]
pub struct Binder<'brand> {
    values: &'brand mut Vec<Value>,
    _brand: PhantomData<fn(&'brand ()) -> &'brand ()>,
}

impl<'brand> Binder<'brand> {
    pub(super) fn new(values: &'brand mut Vec<Value>) -> Self {
        Binder {
            values,
            _brand: PhantomData,
        }
    }

    /// Bind `value` and return its `$N` placeholder.
    ///
    /// Placeholders number in bind order across the whole pipeline, and
    /// prqlc carries them through lowering untouched, so position `N` in the
    /// emitted SQL is position `N` in the pipeline's
    /// [`Values`](pgorm_query::Values).
    pub fn bind(&mut self, value: impl Into<Value>) -> Expr<'brand> {
        self.values.push(value.into());
        branded(adapter::param(self.values.len()))
    }
}
