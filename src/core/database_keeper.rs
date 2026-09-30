use std::collections::HashMap;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt, StreamExt};
use sqlx::{PgPool, Postgres};
use tokio::sync::oneshot;
use tracing::{info, warn};

use crate::components::connection_config::ConnectionConfig;
use crate::core::agent_tools::ToolError;
use crate::types::{Schema, SchemaInfo};
use crate::{app, db};

#[derive(Debug, Clone)]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub database: String,
}

#[derive(Debug, Clone)]
struct Error(pub String);

impl From<&ConnectionConfig> for SavedConnection {
    fn from(cfg: &ConnectionConfig) -> Self {
        Self {
            id: cfg.id.clone(),
            name: cfg.name.clone(),
            database: cfg.database.clone(),
        }
    }
}

pub enum DatabaseKeeperMessage {
    GetPool {
        database_name: String,
        respond: oneshot::Sender<Result<PgPool, ToolError>>,
    },
    GetConnections {
        respond: oneshot::Sender<Vec<SavedConnection>>,
    },
    GetSchema {
        database_name: String,
        respond: oneshot::Sender<Result<Schema, ToolError>>,
    },
    ConnectDatabase {
        database_name: String,
        respond: oneshot::Sender<Result<String, ToolError>>,
    },
    LoadedConfig {
        configs: Vec<ConnectionConfig>,
    },
    ConnectionAction(ConnectionAction),
    SchemaFetched {
        connection: String,
        result: Result<Vec<SchemaInfo>, String>,
    },
}

#[derive(Debug)]
pub enum ConnectionAction {
    Delete { config: ConnectionConfig },
    Add { config: ConnectionConfig },
}

pub struct DatabaseKeeper {
    configs: Vec<ConnectionConfig>,
    pools: HashMap<String, PgPool>,
    schemas: Vec<Schema>,
    receiver: mpsc::Receiver<DatabaseKeeperMessage>,
    background_worker_sender: mpsc::Sender<app::Message>,
}

impl DatabaseKeeper {
    pub fn new(
        receiver: mpsc::Receiver<DatabaseKeeperMessage>,
        background_worker_sender: mpsc::Sender<app::Message>,
    ) -> Self {
        Self {
            configs: vec![],
            pools: HashMap::new(),
            schemas: vec![],
            receiver,
            background_worker_sender,
        }
    }

    pub async fn run(&mut self) {
        while let Some(req) = self.receiver.next().await {
            match req {
                DatabaseKeeperMessage::GetPool {
                    database_name,
                    respond,
                } => {
                    let pool = match self.pools.get(&database_name).cloned() {
                        Some(pool) => Ok(pool),
                        None => self
                            .connect(&database_name)
                            .await
                            .map_err(|err| ToolError(err.0)),
                    };
                    if let Err(_) = respond.send(pool) {
                        tracing::error!("Oneshot channel was closed before sending a response.");
                    }
                }
                DatabaseKeeperMessage::GetConnections { respond } => {
                    let saved: Vec<SavedConnection> =
                        self.configs.iter().map(SavedConnection::from).collect();
                    let _ = respond.send(saved);
                }
                DatabaseKeeperMessage::GetSchema {
                    database_name,
                    respond,
                } => {
                    let result = self.get_schema(&database_name).await;
                    let _ = respond.send(result);
                }
                DatabaseKeeperMessage::ConnectDatabase {
                    database_name,
                    respond,
                } => {
                    info!("ConnectDatabase: attempting to connect to '{database_name}'");
                    let result = self.handle_connect(&database_name).await;
                    if let Err(ref e) = result {
                        warn!("ConnectDatabase: failed to connect to '{database_name}': {e}");
                    }
                    let _ = respond.send(result);
                }
                DatabaseKeeperMessage::LoadedConfig { configs } => {
                    info!("UpdateConnections: {} config(s)", configs.len(),);
                    self.configs = configs;
                    self.pools
                        .retain(|name, _| self.configs.iter().any(|cfg| cfg.name == *name));
                    self.schemas
                        .retain(|s| self.configs.iter().any(|cfg| cfg.name == s.connection));
                }
                DatabaseKeeperMessage::ConnectionAction(connection_action) => {
                    info!("ConnectionAction: {:?}", connection_action);
                    self.handle_connection_action(connection_action)
                }
                DatabaseKeeperMessage::SchemaFetched { connection, result } => {
                    match result {
                        Ok(infos) => {
                            let schema = Schema {
                                connection: connection.clone(),
                                schemas: infos,
                            };
                            self.schemas.retain(|s| s.connection != connection);
                            self.schemas.push(schema.clone());
                            let _ = self
                                .background_worker_sender
                                .send(app::Message::SchemaLoaded {
                                    connection,
                                    result: Ok(schema),
                                })
                                .await;
                        }
                        Err(e) => {
                            warn!("schema fetch failed for '{connection}': {e}");
                            let _ = self
                                .background_worker_sender
                                .send(app::Message::SchemaLoaded {
                                    connection,
                                    result: Err(e),
                                })
                                .await;
                        }
                    }
                }
            }
        }
    }

    async fn connect(&mut self, config_name: &str) -> Result<sqlx::Pool<Postgres>, Error> {
        let config = self
            .configs
            .iter()
            .find(|c| c.name == config_name)
            .ok_or_else(|| {
                Error(format!(
                    "No saved connection named '{}'. Use list_connections to see available connections.",
                    config_name
                ))
            })?
            .clone();

        let pool = db::connect(&config.connection_string())
            .await
            .map_err(|e| Error(format!("Failed to connect: {e}")))?;

        self.pools.insert(config.name.clone(), pool.clone());

        let _ = self
            .background_worker_sender
            .send(app::Message::FetchSchema {
                connection: config.name.clone(),
                pool: pool.clone(),
            })
            .await;

        info!("ConnectDatabase: connected to '{}'", config.name);
        Ok(pool)
    }

    async fn handle_connect(&mut self, database_name: &str) -> Result<String, ToolError> {
        match self.connect(database_name).await {
            Ok(_) => Ok(format!(
                "Connected to '{}'. The database is now available for queries. Please use '{database_name}' for future queries.",
                database_name
            )),
            Err(Error(err)) => Err(ToolError(format!(
                "Failed to connect to '{database_name}'. Failure reason: {err}"
            ))),
        }
    }

    fn handle_connection_action(&mut self, connection_action: ConnectionAction) {
        match connection_action {
            ConnectionAction::Delete { config } => {
                if let Some(index) = self.configs.iter().position(|cfg| cfg.id == config.id) {
                    let removed = self.configs.remove(index);
                    self.pools.remove(&removed.name);
                    self.schemas.retain(|s| s.connection != removed.name);
                }
            }
            ConnectionAction::Add { config } => {
                if let Some(index) = self.configs.iter().position(|cfg| cfg.id == config.id) {
                    self.configs[index] = config;
                } else {
                    self.configs.push(config);
                }
            }
        }
    }

    async fn get_schema(&mut self, name: &str) -> Result<Schema, ToolError> {
        if let Some(schema) = self.schemas.iter().find(|s| s.connection == name) {
            return Ok(schema.clone());
        }

        if let Some(pool) = self.pools.get(name).cloned() {
            let _ = self
                .background_worker_sender
                .send(app::Message::FetchSchema {
                    connection: name.to_string(),
                    pool,
                })
                .await;
        } else {
            self.connect(name).await.map_err(|Error(e)| ToolError(e))?;
        }

        Err(ToolError("Schema is still loading".into()))
    }
}

pub async fn get_pool(
    actor: &mut mpsc::Sender<DatabaseKeeperMessage>,
    database_name: &str,
) -> Result<PgPool, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DatabaseKeeperMessage::GetPool {
            database_name: database_name.to_string(),
            respond: tx,
        })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))?
}

pub async fn get_connections(
    actor: &mut mpsc::Sender<DatabaseKeeperMessage>,
) -> Result<Vec<SavedConnection>, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DatabaseKeeperMessage::GetConnections { respond: tx })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))
}

pub async fn get_schema(
    actor: &mut mpsc::Sender<DatabaseKeeperMessage>,
    database_name: &str,
) -> Result<Schema, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DatabaseKeeperMessage::GetSchema {
            database_name: database_name.to_string(),
            respond: tx,
        })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))?
}

pub async fn connect_database(
    actor: &mut mpsc::Sender<DatabaseKeeperMessage>,
    database_name: &str,
) -> Result<String, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DatabaseKeeperMessage::ConnectDatabase {
            database_name: database_name.to_string(),
            respond: tx,
        })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))?
}
