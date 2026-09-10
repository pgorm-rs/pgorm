//! Owned commands cross the task boundary; database borrows never do.

use crate::{
    entities::backend::{Active, Model, Select, Terminal, Write, Written},
    execution::Database,
    expressions::Compiled,
    graphs::backend::{CursorPlan, Query as Graph, Row as GraphRow},
    pipeline::selected_backend::{Query as Sources, Terminal as SourceTerminal},
};
use pgorm::{ConnectionTrait, Error, ValueHolder};
use tokio_postgres::{Row, types::ToSql};

pub(crate) enum Work {
    Execute(Compiled),
    Fetch(Compiled),
    Entity(Select, Terminal),
    Write(Active, Write),
    Graph(Graph, bool),
    Cursor(Graph, CursorPlan),
    Sources(Sources, SourceTerminal),
}

pub(crate) enum Output {
    Count(u64),
    Rows(Vec<Row>),
    Models(Vec<Model>),
    Written(Written),
    Graph(Vec<GraphRow>),
    Finished,
}

impl Work {
    pub(crate) async fn run(self, db: Database<'_>) -> Result<Output, Error> {
        match self {
            Self::Execute(compiled) => {
                let values: Vec<_> = compiled.values.0.into_iter().map(ValueHolder).collect();
                let params: Vec<_> = values.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
                db.execute(&compiled.sql, &params).await.map(Output::Count)
            }
            Self::Fetch(compiled) => {
                let values: Vec<_> = compiled.values.0.into_iter().map(ValueHolder).collect();
                let params: Vec<_> = values.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
                db.query_all(&compiled.sql, &params).await.map(Output::Rows)
            }
            Self::Entity(query, terminal) => query.run(db, terminal).await.map(Output::Models),
            Self::Write(active, write) => active.run(db, write).await.map(Output::Written),
            Self::Graph(query, optional) => query.run(db, optional).await.map(Output::Graph),
            Self::Cursor(query, cursor) => query.cursor(db, cursor).await.map(Output::Graph),
            Self::Sources(query, terminal) => query.run(db, terminal).await.map(Output::Graph),
        }
    }
}
