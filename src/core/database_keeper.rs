use std::collections::HashMap;

use sqlx::PgPool;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use crate::core::agent_tools::ToolError;
use crate::core::connection_config::ConnectionConfig;
use crate::db;

#[derive(Debug, Clone)]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub database: String,
}

impl From<&ConnectionConfig> for SavedConnection {
    fn from(cfg: &ConnectionConfig) -> Self {
        Self {
            id: cfg.id.clone(),
            name: cfg.name.clone(),
            database: cfg.database.clone(),
        }
    }
}

pub enum DbRequest {
    GetPool {
        database_name: String,
        respond: oneshot::Sender<Result<PgPool, ToolError>>,
    },
    GetConnections {
        respond: oneshot::Sender<Vec<SavedConnection>>,
    },
    UpdateConnections {
        configs: Vec<ConnectionConfig>,
        pools: HashMap<String, PgPool>,
    },
    ConnectDatabase {
        database_name: String,
        respond: oneshot::Sender<Result<String, ToolError>>,
    },
}

pub struct DatabaseKeeper {
    configs: Vec<ConnectionConfig>,
    pools: HashMap<String, PgPool>,
    receiver: mpsc::Receiver<DbRequest>,
}

impl DatabaseKeeper {
    pub fn new(receiver: mpsc::Receiver<DbRequest>) -> Self {
        Self {
            configs: Vec::new(),
            pools: HashMap::new(),
            receiver,
        }
    }

    pub async fn run(&mut self) {
        while let Some(req) = self.receiver.recv().await {
            match req {
                DbRequest::GetPool {
                    database_name,
                    respond,
                } => {
                    let result = self
                        .pools
                        .get(&database_name)
                        .cloned()
                        .ok_or_else(|| {
                            warn!(
                                "GetPool: '{}' not found, available: {:?}",
                                database_name,
                                self.pools.keys().collect::<Vec<_>>()
                            );
                            ToolError(format!(
                                "Database '{}' not found. Available: {:?}",
                                database_name,
                                self.pools.keys().collect::<Vec<_>>()
                            ))
                        });
                    let _ = respond.send(result);
                }
                DbRequest::GetConnections { respond } => {
                    let saved: Vec<SavedConnection> =
                        self.configs.iter().map(SavedConnection::from).collect();
                    let _ = respond.send(saved);
                }
                DbRequest::UpdateConnections { configs, pools } => {
                    info!(
                        "UpdateConnections: {} config(s), {} pool(s)",
                        configs.len(),
                        pools.len()
                    );
                    self.configs = configs;
                    self.pools = pools;
                }
                DbRequest::ConnectDatabase {
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
            }
        }
    }

    async fn handle_connect(&mut self, database_name: &str) -> Result<String, ToolError> {
        let config = self
            .configs
            .iter()
            .find(|c| c.name == database_name || c.id == database_name)
            .ok_or_else(|| {
                ToolError(format!(
                    "No saved connection named '{}'. Use list_connections to see available connections.",
                    database_name
                ))
            })?
            .clone();

        let cs = config.connection_string();
        let pool = db::connect(&cs)
            .await
            .map_err(|e| ToolError(format!("Failed to connect: {e}")))?;

        self.pools.insert(config.name.clone(), pool);
        info!("ConnectDatabase: connected to '{}'", config.name);

        Ok(format!(
            "Connected to '{}' (database: '{}', host: '{}:{}'). The database is now available for queries.",
            config.name, config.database, config.host, config.port
        ))
    }
}

pub async fn get_pool(
    actor: &mpsc::Sender<DbRequest>,
    database_name: &str,
) -> Result<PgPool, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DbRequest::GetPool {
            database_name: database_name.to_string(),
            respond: tx,
        })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))?
}

pub async fn get_connections(
    actor: &mpsc::Sender<DbRequest>,
) -> Result<Vec<SavedConnection>, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DbRequest::GetConnections { respond: tx })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))
}

pub async fn connect_database(
    actor: &mpsc::Sender<DbRequest>,
    database_name: &str,
) -> Result<String, ToolError> {
    let (tx, rx) = oneshot::channel();
    actor
        .send(DbRequest::ConnectDatabase {
            database_name: database_name.to_string(),
            respond: tx,
        })
        .await
        .map_err(|_| ToolError("Database actor is not running".into()))?;
    rx.await
        .map_err(|_| ToolError("Database actor did not respond".into()))?
}
