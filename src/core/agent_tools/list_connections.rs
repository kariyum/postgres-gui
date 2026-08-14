use rig_core::tool::PortableTool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use iced::futures::channel::mpsc::Sender;

use super::{DatabaseKeeperMessage, ToolError, get_connections};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListConnectionsArgs {}

pub struct ListConnections {
    db_actor: Sender<DatabaseKeeperMessage>,
}

impl Clone for ListConnections {
    fn clone(&self) -> Self {
        Self {
            db_actor: self.db_actor.clone(),
        }
    }
}

impl ListConnections {
    pub fn new(db_actor: Sender<DatabaseKeeperMessage>) -> Self {
        Self { db_actor }
    }
}

impl PortableTool for ListConnections {
    const NAME: &'static str = "list_connections";

    type Error = ToolError;
    type Args = ListConnectionsArgs;
    type Output = String;

    fn description(&self) -> String {
        "List all saved database connection configurations. \
                          Returns a JSON array of connections, each with 'id', 'name', and 'database'. \
                          Use the 'id' or 'name' as the database_name argument in other tools \
                          if the connection is already active, or present options to the user to connect."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let connections = get_connections(&mut self.db_actor.clone()).await?;

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
