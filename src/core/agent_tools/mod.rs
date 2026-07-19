mod describe_table;
mod execute_sql;
mod explain_query;
mod list_schemas;
mod list_tables;
mod show_table_stats;

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use sqlx::PgPool;

use rig_core::completion::ToolDefinition;
use rig_core::tool::ToolSet;

pub use describe_table::DescribeTable;
pub use execute_sql::ExecuteSql;
pub use explain_query::ExplainQuery;
pub use list_schemas::ListSchemas;
pub use list_tables::ListTables;
pub use show_table_stats::ShowTableStats;

#[derive(Debug, Clone)]
pub struct ToolError(pub String);

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolError {}

impl From<serde_json::Error> for ToolError {
    fn from(e: serde_json::Error) -> Self {
        ToolError(e.to_string())
    }
}

impl From<sqlx::Error> for ToolError {
    fn from(e: sqlx::Error) -> Self {
        ToolError(e.to_string())
    }
}

pub fn needs_approval(tool_name: &str, args_json: &str) -> bool {
    if tool_name != "execute_sql" {
        eprintln!("[pgeru:tools] needs_approval({tool_name}): not execute_sql, no approval needed");
        return false;
    }
    if let Ok(val) = serde_json::from_str::<Value>(args_json) {
        if let Some(sql) = val.get("sql").and_then(|v| v.as_str()) {
            let destructive = is_destructive(sql);
            eprintln!(
                "[pgeru:tools] needs_approval(execute_sql): sql={sql:?} destructive={destructive}"
            );
            return destructive;
        }
        eprintln!("[pgeru:tools] needs_approval(execute_sql): no 'sql' field in args_json");
    } else {
        eprintln!("[pgeru:tools] needs_approval: failed to parse args_json as JSON: {args_json}");
    }
    false
}

pub fn is_destructive(sql: &str) -> bool {
    let trimmed = sql.trim().to_uppercase();
    trimmed.starts_with("INSERT")
        || trimmed.starts_with("UPDATE")
        || trimmed.starts_with("DELETE")
        || trimmed.starts_with("DROP")
        || trimmed.starts_with("TRUNCATE")
        || trimmed.starts_with("ALTER")
        || trimmed.starts_with("CREATE")
        || trimmed.starts_with("REINDEX")
        || trimmed.starts_with("VACUUM")
        || trimmed.starts_with("CLUSTER")
}

#[derive(Clone)]
pub struct ToolManager {
    toolset: Arc<ToolSet>,
}

impl std::fmt::Debug for ToolManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolManager").finish_non_exhaustive()
    }
}

impl ToolManager {
    pub fn new(pool: PgPool) -> Self {
        let mut toolset = ToolSet::default();
        toolset.add_tool(ExecuteSql::new(pool.clone()));
        toolset.add_tool(ListSchemas::new(pool.clone()));
        toolset.add_tool(ListTables::new(pool.clone()));
        toolset.add_tool(DescribeTable::new(pool.clone()));
        toolset.add_tool(ExplainQuery::new(pool.clone()));
        toolset.add_tool(ShowTableStats::new(pool));

        Self {
            toolset: Arc::new(toolset),
        }
    }

    /// Create a ToolManager with no database tools.
    /// Useful as a placeholder when no connection is active.
    pub fn without_db() -> Self {
        Self {
            toolset: Arc::new(ToolSet::default()),
        }
    }

    pub async fn definitions(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        self.toolset
            .get_tool_definitions()
            .await
            .map_err(|e| ToolError(e.to_string()))
    }

    pub async fn execute(&self, tool_name: &str, args_json: &str) -> Result<String, ToolError> {
        eprintln!(
            "[pgeru:tools] execute({tool_name}) starting, args_len={}",
            args_json.len()
        );
        let result = self
            .toolset
            .call(tool_name, args_json.to_string())
            .await
            .map_err(|e| ToolError(e.to_string()));
        match &result {
            Ok(out) => eprintln!(
                "[pgeru:tools] execute({tool_name}) succeeded, output_len={}",
                out.len()
            ),
            Err(e) => eprintln!("[pgeru:tools] execute({tool_name}) failed: {e}"),
        }
        result
    }
}
