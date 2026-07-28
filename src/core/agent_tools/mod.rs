mod connect_to_database;
mod describe_table;
mod execute_sql;
mod explain_query;
mod list_connections;
mod list_schemas;
mod list_tables;
mod show_table_stats;

use std::collections::HashMap;
use std::fmt;

use serde_json::Value;
use sqlx::{PgPool, Row};
use tokio::sync::mpsc::Sender;
use tracing::info;

use rig_core::completion::ToolDefinition;
use rig_core::tool::ToolSet;

pub use connect_to_database::ConnectToDatabase;
pub use describe_table::DescribeTable;
pub use execute_sql::ExecuteSql;
pub use explain_query::ExplainQuery;
pub use list_connections::ListConnections;
pub use list_schemas::ListSchemas;
pub use list_tables::ListTables;
pub use show_table_stats::ShowTableStats;

pub use crate::core::database_keeper::{DbRequest, get_connections, get_pool};

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
        info!("needs_approval({tool_name}): not execute_sql, no approval needed");
        return false;
    }
    if let Ok(val) = serde_json::from_str::<Value>(args_json) {
        if let Some(sql) = val.get("sql").and_then(|v| v.as_str()) {
            let destructive = is_destructive(sql);
            info!("needs_approval(execute_sql): sql={sql:?} destructive={destructive}");
            return destructive;
        }
        info!("needs_approval(execute_sql): no 'sql' field in args_json");
    } else {
        info!("needs_approval: failed to parse args_json as JSON: {args_json}");
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

pub fn cell_to_value(row: &sqlx::postgres::PgRow, idx: usize, type_name: &str) -> Value {
    let string_val = match type_name {
        "INT2" => row.try_get::<i16, _>(idx).ok().map(|v| v.to_string()),
        "INT4" => row.try_get::<i32, _>(idx).ok().map(|v| v.to_string()),
        "INT8" => row.try_get::<i64, _>(idx).ok().map(|v| v.to_string()),
        "FLOAT4" => row.try_get::<f32, _>(idx).ok().map(|v| v.to_string()),
        "FLOAT8" => row.try_get::<f64, _>(idx).ok().map(|v| v.to_string()),
        "BOOL" => row.try_get::<bool, _>(idx).ok().map(|v| v.to_string()),
        _ => None,
    };

    if let Some(s) = string_val {
        return Value::String(s);
    }

    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.map_or(Value::Null, Value::String);
    }
    if let Ok(v) = row.try_get::<Option<&str>, _>(idx) {
        return v.map_or(Value::Null, |s| Value::String(s.to_string()));
    }

    Value::Null
}

#[derive(Clone)]
pub struct Tools {
    toolset: std::sync::Arc<ToolSet>,
    sender: Sender<DbRequest>,
}

impl std::fmt::Debug for Tools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tools").finish_non_exhaustive()
    }
}

impl Tools {
    pub fn new(sender: Sender<DbRequest>) -> Self {
        let mut toolset = ToolSet::default();
        toolset.add_tool(ExecuteSql::new(sender.clone()));
        toolset.add_tool(ListSchemas::new(sender.clone()));
        toolset.add_tool(ListTables::new(sender.clone()));
        toolset.add_tool(DescribeTable::new(sender.clone()));
        toolset.add_tool(ExplainQuery::new(sender.clone()));
        toolset.add_tool(ShowTableStats::new(sender.clone()));
        toolset.add_tool(ListConnections::new(sender.clone()));
        toolset.add_tool(ConnectToDatabase::new(sender.clone()));

        Self {
            toolset: std::sync::Arc::new(toolset),
            sender,
        }
    }

    pub fn update_connections(
        &self,
        configs: Vec<crate::core::connection_config::ConnectionConfig>,
        pools: HashMap<String, PgPool>,
    ) {
        let _ = self
            .sender
            .try_send(DbRequest::UpdateConnections { configs, pools });
    }

    pub async fn definitions(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        self.toolset
            .get_tool_definitions()
            .await
            .map_err(|e| ToolError(e.to_string()))
    }

    pub async fn execute(&self, tool_name: &str, args_json: &str) -> Result<String, ToolError> {
        info!(
            "execute({tool_name}) starting, args_len={}",
            args_json.len()
        );
        let result = self
            .toolset
            .call(tool_name, args_json.to_string())
            .await
            .map_err(|e| ToolError(e.to_string()));
        match &result {
            Ok(out) => info!("execute({tool_name}) succeeded, output_len={}", out.len()),
            Err(e) => info!("execute({tool_name}) failed: {e}"),
        }
        result
    }
}
