use rig_core::tool::PortableTool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use iced::futures::channel::mpsc::Sender;

use super::{DatabaseKeeperMessage, ToolError, get_pool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSchemasArgs {
    pub database_name: String,
}

pub struct ListSchemas {
    db_actor: Sender<DatabaseKeeperMessage>,
}

impl Clone for ListSchemas {
    fn clone(&self) -> Self {
        Self {
            db_actor: self.db_actor.clone(),
        }
    }
}

impl ListSchemas {
    pub fn new(db_actor: Sender<DatabaseKeeperMessage>) -> Self {
        Self { db_actor }
    }
}

impl PortableTool for ListSchemas {
    const NAME: &'static str = "list_schemas";

    type Error = ToolError;
    type Args = ListSchemasArgs;
    type Output = String;

    fn description(&self) -> String {
        "List all non-system schemas in the connected PostgreSQL database. \
                          The database_name must match one of the available connected databases. \
                          Returns a JSON array of schema names."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "database_name": {
                    "type": "string",
                    "description": "The name of the database to list schemas from"
                }
            },
            "required": ["database_name"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pool = get_pool(&mut self.db_actor.clone(), &args.database_name).await?;
        let schemas: Vec<String> = sqlx::query_scalar(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
             ORDER BY schema_name",
        )
        .fetch_all(&pool)
        .await?;

        Ok(json!({
            "schemas": schemas
        })
        .to_string())
    }
}
