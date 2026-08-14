pub mod connect_to_database;
pub mod describe_table;
pub mod execute_sql;
pub mod explain_query;
pub mod list_connections;
pub mod list_schemas;
pub mod list_tables;
pub mod show_table_stats;

use std::fmt;

use iced::futures::channel::mpsc::Sender;
use serde_json::Value;
use sqlx::Row;
use tracing::info;

use rig_core::completion::ToolDefinition;
use rig_core::tool::{
    IntoToolOutput, PortableDynamicTool, PortableTool, ToolExecutionError,
    portable_tool_definition,
};

pub use connect_to_database::ConnectToDatabase;
pub use describe_table::DescribeTable;
pub use execute_sql::ExecuteSql;
pub use explain_query::ExplainQuery;
pub use list_connections::ListConnections;
pub use list_schemas::ListSchemas;
pub use list_tables::ListTables;
pub use show_table_stats::ShowTableStats;

pub use crate::core::database_keeper::{DatabaseKeeperMessage, get_connections, get_pool};

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
    toolset: std::sync::Arc<Vec<PortableDynamicTool>>,
}

impl std::fmt::Debug for Tools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tools").finish_non_exhaustive()
    }
}

impl Tools {
    pub fn new(sender: Sender<DatabaseKeeperMessage>) -> Self {
        let toolset = vec![
            into_dynamic(ExecuteSql::new(sender.clone())),
            into_dynamic(ListSchemas::new(sender.clone())),
            into_dynamic(ListTables::new(sender.clone())),
            into_dynamic(DescribeTable::new(sender.clone())),
            into_dynamic(ExplainQuery::new(sender.clone())),
            into_dynamic(ShowTableStats::new(sender.clone())),
            into_dynamic(ListConnections::new(sender.clone())),
            into_dynamic(ConnectToDatabase::new(sender.clone())),
        ];

        Self {
            toolset: std::sync::Arc::new(toolset),
        }
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.toolset.iter().map(|tool| tool.definition()).collect()
    }

    pub async fn execute(&self, tool_name: &str, args_json: &str) -> Result<String, ToolError> {
        info!(
            "execute({tool_name}) starting, args_len={}",
            args_json.len()
        );
        let tool = self
            .toolset
            .iter()
            .find(|tool| tool.name() == tool_name)
            .ok_or_else(|| ToolError(format!("Tool not found: {tool_name}")))?;

        let args: Value = serde_json::from_str(args_json)
            .map_err(|e| ToolError(format!("Failed to parse tool arguments: {e}")))?;

        let output = tool
            .execute(args)
            .await
            .map_err(|e| ToolError(e.to_string()))?;
        let rendered = output.render();
        info!("execute({tool_name}) succeeded, output_len={}", rendered.len());
        Ok(rendered)
    }
}

fn into_dynamic<T>(tool: T) -> PortableDynamicTool
where
    T: PortableTool + Clone + 'static,
{
    let definition = portable_tool_definition(&tool);
    PortableDynamicTool::new(
        definition.name,
        definition.description,
        definition.parameters,
        move |args: Value| {
            let tool = tool.clone();
            Box::pin(async move {
                let typed_args: T::Args = serde_json::from_value(args)
                    .map_err(|e| ToolExecutionError::invalid_args(format!("invalid arguments: {e}")))?;
                tool.call(typed_args)
                    .await
                    .map_err(ToolExecutionError::from_error)?
                    .into_tool_output()
            })
        },
    )
}
