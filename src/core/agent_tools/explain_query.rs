use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

use super::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainQueryArgs {
    pub sql: String,
}

pub struct ExplainQuery {
    pool: PgPool,
}

impl ExplainQuery {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
                          Returns the plan as a JSON object. \
                          Note: This does not execute the query (no ANALYZE)."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "sql": {
                        "type": "string",
                        "description": "The SQL query to explain"
                    }
                },
                "required": ["sql"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let explain_sql = format!("EXPLAIN (FORMAT JSON) {}", args.sql);
        let rows = sqlx::query(&explain_sql).fetch_all(&self.pool).await?;

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
