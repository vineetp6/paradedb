use deltalake::datafusion::error::DataFusionError;
use deltalake::datafusion::logical_expr::LogicalPlan;
use deltalake::datafusion::sql::parser::DFParser;
use deltalake::datafusion::sql::planner::SqlToRel;
use deltalake::datafusion::sql::sqlparser::dialect::PostgreSqlDialect;
use pgrx::*;
use std::ffi::CStr;

use crate::datafusion::context::ParadeContextProvider;

use crate::errors::{NotSupported, ParadeError};
use crate::hooks::delete::delete;
use crate::hooks::handler::IsColumn;
use crate::hooks::query::Query;
use crate::hooks::select::select;

pub fn executor_run(
    query_desc: PgBox<pg_sys::QueryDesc>,
    direction: pg_sys::ScanDirection,
    count: u64,
    execute_once: bool,
    prev_hook: fn(
        query_desc: PgBox<pg_sys::QueryDesc>,
        direction: pg_sys::ScanDirection,
        count: u64,
        execute_once: bool,
    ) -> HookResult<()>,
) -> Result<(), ParadeError> {
    unsafe {
        let ps = query_desc.plannedstmt;
        let rtable = (*ps).rtable;
        let query = query_desc
            .plannedstmt
            .current_query_string(CStr::from_ptr(query_desc.sourceText))?;

        // Only use this hook for deltalake tables
        // Allow INSERTs to go through
        if rtable.is_null()
            || query_desc.operation == pg_sys::CmdType_CMD_INSERT
            || !rtable.is_column()?
            // Tech Debt: Find a less hacky way to let COPY go through
            || query.to_lowercase().starts_with("copy")
        {
            prev_hook(query_desc, direction, count, execute_once);
            return Ok(());
        }

        // Execute SELECT, DELETE, UPDATE
        match query_desc.operation {
            pg_sys::CmdType_CMD_DELETE => {
                let logical_plan = create_logical_plan(&query)?;
                delete(rtable, query_desc, logical_plan)
            }
            pg_sys::CmdType_CMD_SELECT => {
                let logical_plan = create_logical_plan(&query)?;
                select(query_desc, logical_plan)
            }
            pg_sys::CmdType_CMD_UPDATE => Err(NotSupported::Update.into()),
            _ => {
                prev_hook(query_desc, direction, count, execute_once);
                Ok(())
            }
        }
    }
}

#[inline]
fn create_logical_plan(query: &str) -> Result<LogicalPlan, ParadeError> {
    let dialect = PostgreSqlDialect {};
    let ast = DFParser::parse_sql_with_dialect(query, &dialect)
        .map_err(|err| ParadeError::DataFusion(DataFusionError::SQL(err, None)))?;
    let statement = &ast[0];

    // Convert the AST into a logical plan
    let context_provider = ParadeContextProvider::new()?;
    let sql_to_rel = SqlToRel::new(&context_provider);

    Ok(sql_to_rel.statement_to_plan(statement.clone())?)
}
