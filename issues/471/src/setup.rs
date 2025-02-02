use pgorm::pgorm_query::{ColumnDef, TableCreateStatement};
use pgorm::{error::*, pgorm_query, ConnectionTrait, DbConn, ExecResult};

async fn create_table(db: &DbConn, stmt: &TableCreateStatement) -> Result<ExecResult, DbErr> {
    let builder = db.get_database_backend();
    db.execute(builder.build(stmt)).await
}

pub async fn create_post_table(db: &DbConn) -> Result<ExecResult, DbErr> {
    let stmt = pgorm_query::Table::create()
        .table(super::post::Entity)
        .if_not_exists()
        .col(
            ColumnDef::new(super::post::Column::Id)
                .integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
        .col(
            ColumnDef::new(super::post::Column::Title)
                .string()
                .not_null(),
        )
        .col(
            ColumnDef::new(super::post::Column::Text)
                .string()
                .not_null(),
        )
        .to_owned();

    create_table(db, &stmt).await
}
