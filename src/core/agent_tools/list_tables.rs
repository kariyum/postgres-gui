use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

use super::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTablesArgs {
    pub schema: String,
}

pub struct ListTables {
    pool: PgPool,
}

impl ListTables {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ListTables {
    const NAME: &'static str = "list_tables";

    type Error = ToolError;
    type Args = ListTablesArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "List all base tables in a given schema. \
                          Returns a JSON object with schema name and an array of table names."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schema": {
                        "type": "string",
                        "description": "The schema name to list tables from"
                    }
                },
                "required": ["schema"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(&args.schema)
        .fetch_all(&self.pool)
        .await?;

        Ok(json!({
            "schema": args.schema,
            "tables": tables
        })
        .to_string())
    }
}
