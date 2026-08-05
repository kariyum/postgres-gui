use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::mpsc::Sender;

use super::{DatabaseKeeperMessage, ToolError, get_pool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainQueryArgs {
    pub database_name: String,
    pub sql: String,
}

pub struct ExplainQuery {
    db_actor: Sender<DatabaseKeeperMessage>,
}

impl ExplainQuery {
    pub fn new(db_actor: Sender<DatabaseKeeperMessage>) -> Self {
        Self { db_actor }
    }
}

impl Tool for ExplainQuery {
    const NAME: &'static str = "explain_query";

    type Error = ToolError;
    type Args = ExplainQueryArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description:
                "Get the query execution plan for a SQL statement using EXPLAIN (FORMAT JSON). \
                          The database_name must match one of the available connected databases. \
                          Returns the plan as a JSON object. \
                          Note: This does not execute the query (no ANALYZE)."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "database_name": {
                        "type": "string",
                        "description": "The name of the database to explain the query on"
                    },
                    "sql": {
                        "type": "string",
                        "description": "The SQL query to explain"
                    }
                },
                "required": ["database_name", "sql"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pool = get_pool(&self.db_actor, &args.database_name).await?;
        let explain_sql = format!("EXPLAIN (FORMAT JSON) {}", args.sql);
        let rows = sqlx::query(&explain_sql).fetch_all(&pool).await?;

        let plan_lines: Vec<String> = rows.iter().map(|row| row.get::<String, _>(0)).collect();

        let plan_json: Value = plan_lines
            .join("")
            .parse()
            .unwrap_or(Value::String(plan_lines.join("\n")));

        Ok(json!({
            "query": args.sql,
            "plan": plan_json
        })
        .to_string())
    }
}
