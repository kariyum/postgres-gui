use rig_core::tool::PortableTool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use iced::futures::channel::mpsc::Sender;

use super::{DatabaseKeeperMessage, ToolError, get_pool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTablesArgs {
    pub database_name: String,
    pub schema: String,
}

pub struct ListTables {
    db_actor: Sender<DatabaseKeeperMessage>,
}

impl Clone for ListTables {
    fn clone(&self) -> Self {
        Self {
            db_actor: self.db_actor.clone(),
        }
    }
}

impl ListTables {
    pub fn new(db_actor: Sender<DatabaseKeeperMessage>) -> Self {
        Self { db_actor }
    }
}

impl PortableTool for ListTables {
    const NAME: &'static str = "list_tables";

    type Error = ToolError;
    type Args = ListTablesArgs;
    type Output = String;

    fn description(&self) -> String {
        "List all base tables in a given schema. \
                          The database_name must match one of the available connected databases. \
                          Returns a JSON object with schema name and an array of table names."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "database_name": {
                    "type": "string",
                    "description": "The name of the database to list tables from"
                },
                "schema": {
                    "type": "string",
                    "description": "The schema name to list tables from"
                }
            },
            "required": ["database_name", "schema"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pool = get_pool(&mut self.db_actor.clone(), &args.database_name).await?;
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(&args.schema)
        .fetch_all(&pool)
        .await?;

        Ok(json!({
            "schema": args.schema,
            "tables": tables
        })
        .to_string())
    }
}
