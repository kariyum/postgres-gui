use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc::Sender;

use super::{DbRequest, ToolError, get_connections};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListConnectionsArgs {}

pub struct ListConnections {
    db_actor: Sender<DbRequest>,
}

impl ListConnections {
    pub fn new(db_actor: Sender<DbRequest>) -> Self {
        Self { db_actor }
    }
}

impl Tool for ListConnections {
    const NAME: &'static str = "list_connections";

    type Error = ToolError;
    type Args = ListConnectionsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "List all saved database connection configurations. \
                          Returns a JSON array of connections, each with 'id', 'name', and 'database'. \
                          Use the 'id' or 'name' as the database_name argument in other tools \
                          if the connection is already active, or present options to the user to connect."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let connections = get_connections(&self.db_actor).await?;

        let items: Vec<serde_json::Value> = connections
            .iter()
            .map(|c| {
                json!({
                    "id": c.id,
                    "name": c.name,
                    "database": c.database,
                })
            })
            .collect();

        Ok(json!({
            "connections": items
        })
        .to_string())
    }
}
