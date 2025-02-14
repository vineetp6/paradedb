use async_std::task::block_on;
use rstest::*;
use sqlx::{self, PgConnection};

pub use shared::fixtures::db::*;
pub use shared::fixtures::tables::*;

#[fixture]
pub fn database() -> Db {
    block_on(async { Db::new().await })
}

#[fixture]
pub fn conn(database: Db) -> PgConnection {
    block_on(async {
        let mut conn = database.connection().await;
        sqlx::query("CREATE EXTENSION pg_bm25;")
            .execute(&mut conn)
            .await
            .expect("could not create extension pg_bm25");
        conn
    })
}
