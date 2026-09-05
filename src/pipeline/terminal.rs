//! The fallible boundary: pipeline → SQL → decoded rows.

use pgorm_query::Values;

use crate::{
    ConnectionTrait, DecodeRaw, Error, FromQueryResult, SelectGetableTuple, SelectModel,
    SelectorRaw, TryGetableMany,
};

use super::adapter;
use super::builder::Pipeline;
use super::error::{PipelineError, RESERVED};

impl Pipeline {
    /// Compile the pipeline to PostgreSQL SQL and the values bound along the
    /// way, in placeholder order.
    ///
    /// This is where everything fallible happens — reserved-alias screening,
    /// then prqlc's name resolution and lowering — and it fails as a
    /// [`PipelineError`], never a panic.
    // [spec:pgorm:req:pipeline.errors+1]
    pub fn into_sql(self) -> Result<(String, Values), PipelineError> {
        let mut aliases = Vec::new();
        for stage in self.bindings.iter().flatten().chain(&self.stages) {
            adapter::collect_aliases(stage, &mut aliases);
        }
        if let Some(name) = aliases
            .into_iter()
            .find(|alias| RESERVED.contains(&alias.as_str()))
        {
            return Err(PipelineError::ReservedAlias(name));
        }
        let sql = adapter::compile(self.bindings, self.stages).map_err(PipelineError::Compile)?;
        Ok((sql, Values(self.values)))
    }

    /// Compile and stage the pipeline for decoding into a
    /// [`FromQueryResult`] type.
    // [spec:pgorm:sem:pipeline.terminal]
    pub fn into_model<M>(self) -> Result<SelectorRaw<SelectModel<M>>, PipelineError>
    where
        M: FromQueryResult,
    {
        Ok(self.into_sql()?.into_model::<M>())
    }

    /// Compile and stage the pipeline for decoding into a tuple by column
    /// position.
    // [spec:pgorm:sem:pipeline.terminal]
    pub fn into_tuple<T>(self) -> Result<SelectorRaw<SelectGetableTuple<T>>, PipelineError>
    where
        T: TryGetableMany,
    {
        Ok(self.into_sql()?.into_tuple::<T>())
    }

    /// Compile, execute and decode every row into `M`.
    // [spec:pgorm:sem:pipeline.terminal]
    pub async fn all<M, C>(self, db: &C) -> Result<Vec<M>, Error>
    where
        M: FromQueryResult,
        C: ConnectionTrait,
    {
        self.into_model::<M>()?.all(db).await
    }

    /// Compile, execute and decode the first row into `M`, failing with
    /// [`Error::RecordNotFound`] when there is none.
    ///
    /// A `take 1` stage is appended, so at most one row leaves the server.
    // [spec:pgorm:sem:pipeline.terminal]
    pub async fn one<M, C>(self, db: &C) -> Result<M, Error>
    where
        M: FromQueryResult,
        C: ConnectionTrait,
    {
        self.take(1).into_model::<M>()?.one(db).await
    }

    /// Compile, execute and decode the first row into `M`, or `None` when
    /// there is none. Appends `take 1` like [`one`](Pipeline::one).
    // [spec:pgorm:sem:pipeline.terminal]
    pub async fn one_opt<M, C>(self, db: &C) -> Result<Option<M>, Error>
    where
        M: FromQueryResult,
        C: ConnectionTrait,
    {
        self.take(1).into_model::<M>()?.one_opt(db).await
    }
}
