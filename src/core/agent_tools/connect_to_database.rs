use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use iced::futures::channel::mpsc::Sender;

use super::{DatabaseKeeperMessage, ToolError};
use crate::core::database_keeper::connect_database;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectToDatabaseArgs {
    pub database_name: String,
}

pub struct ConnectToDatabase {
    db_actor: Sender<DatabaseKeeperMessage>,
}

impl ConnectToDatabase {
    pub fn new(db_actor: Sender<DatabaseKeeperMessage>) -> Self {
        Self { db_actor }
    }
}

impl Tool for ConnectToDatabase {
    const NAME: &'static str = "connect_to_database";

    type Error = ToolError;
    type Args = ConnectToDatabaseArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Connect to a saved database by its name or id. \
                          Use list_connections first to see available saved connections. \
                          After connecting, the database becomes available for query tools. \
                          Returns a success message with connection details, or an error if the connection fails."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "database_name": {
                        "type": "string",
                        "description": "The name or id of the saved connection to connect to"
                    }
                },
                "required": ["database_name"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        connect_database(&mut self.db_actor.clone(), &args.database_name).await
    }
}
