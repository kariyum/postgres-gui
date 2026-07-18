use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

use super::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSchemasArgs {}

pub struct ListSchemas {
    pool: PgPool,
}

impl ListSchemas {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ListSchemas {
    const NAME: &'static str = "list_schemas";

    type Error = ToolError;
    type Args = ListSchemasArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "List all non-system schemas in the connected PostgreSQL database. \
                          Returns a JSON array of schema names."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let schemas: Vec<String> = sqlx::query_scalar(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
             ORDER BY schema_name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(json!({
            "schemas": schemas
        })
        .to_string())
    }
}
