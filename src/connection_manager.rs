use anyhow::Context;
use iced::Task;
use sqlx::PgPool;
use tracing::error;

use crate::components::connection_dialog::DialogMessage;
use crate::components::connection_item::{ConnectionItem, ItemMessage};
use crate::core::config_loader::{self, AppConfig, load_config, save_config};
use crate::core::connection_config::ConnectionConfig;
use crate::db;

#[derive(Debug, Clone)]
pub enum ConnManagerMessage {
    ConnectionItemMessage(String, ItemMessage),
    ConnectCompleted(String, Result<PgPool, String>),
    ConnectionDialogMessage(DialogMessage),
    ConnectionSaved(Result<(), String>),
    ConnectionsLoaded(Vec<ConnectionConfig>),
}

#[derive(Debug)]
pub enum Action {
    None,
    Run(Task<ConnManagerMessage>),
    Dialog(DialogMessage),
}

#[derive(Debug)]
pub struct ConnectionManager {
    pub items: Vec<ConnectionItem>,
    pub active_connection: Option<String>,
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active_connection: None,
        }
    }
}

impl ConnectionManager {
    pub fn update(&mut self, message: ConnManagerMessage) -> Action {
        match message {
            ConnManagerMessage::ConnectionItemMessage(id, msg) => {
                self.handle_item_message(&id, msg)
            }

            ConnManagerMessage::ConnectCompleted(id, result) => {
                self.handle_connect_completed(id, result)
            }

            ConnManagerMessage::ConnectionDialogMessage(msg) => {
                self.handle_dialog_message(msg)
            }

            ConnManagerMessage::ConnectionSaved(Ok(())) => {
                Action::Dialog(DialogMessage::DialogClose)
            }

            ConnManagerMessage::ConnectionSaved(Err(e)) => {
                error!("Failed to save connection: {e}");
                Action::None
            }

            ConnManagerMessage::ConnectionsLoaded(configs) => {
                for cfg in configs {
                    self.items.push(ConnectionItem::new(cfg));
                }
                Action::None
            }
        }
    }

    fn handle_item_message(&mut self, id: &str, msg: ItemMessage) -> Action {
        let task = self.delegate_to_item(id, msg.clone());

        match msg {
            ItemMessage::ConnectRequested => self.handle_connect_requested(id),
            ItemMessage::DisconnectRequested => self.handle_disconnect_requested(id, task),
            ItemMessage::RunQuery => self.handle_run_query(id, task),
            ItemMessage::EditRequested => self.handle_edit_requested(id),
            ItemMessage::DeleteRequested => self.handle_delete_requested(id),
            ItemMessage::DuplicateRequested => self.handle_duplicate_requested(id),
            ItemMessage::CopyStringRequested => self.handle_copy_string_requested(id),
            ItemMessage::Select => {
                self.active_connection = Some(id.to_string());
                Action::None
            }
            _ => Action::Run(task),
        }
    }

    fn handle_connect_requested(&mut self, id: &str) -> Action {
        let cs = match self.items.iter().find(|i| i.cfg.id == id) {
            Some(item) => item.cfg.connection_string(),
            None => return Action::None,
        };
        let id = id.to_string();
        Action::Run(Task::perform(async move { db::connect(&cs).await }, move |result| {
            ConnManagerMessage::ConnectCompleted(id, result)
        }))
    }

    fn handle_disconnect_requested(
        &mut self,
        id: &str,
        task: Task<ConnManagerMessage>,
    ) -> Action {
        if self.active_connection.as_deref() == Some(id) {
            self.active_connection = self
                .items
                .iter()
                .find(|i| i.pool.is_some())
                .map(|i| i.cfg.id.clone());
        }
        Action::Run(task)
    }

    fn handle_run_query(
        &self,
        id: &str,
        task: Task<ConnManagerMessage>,
    ) -> Action {
        let (sql, pool) = match self.items.iter().find(|i| i.cfg.id == id) {
            Some(item) => (item.editor.text(), item.pool.clone()),
            None => {
                return Action::Run(Task::done(ConnManagerMessage::ConnectionItemMessage(
                    id.to_string(),
                    ItemMessage::QueryResult(Err("Connection item deleted?".to_string())),
                )));
            }
        };
        let pool = match pool {
            Some(p) => p,
            None => {
                return Action::Run(Task::done(ConnManagerMessage::ConnectionItemMessage(
                    id.to_string(),
                    ItemMessage::QueryResult(Err(
                        "Did not find connections to run the query".to_string()
                    )),
                )));
            }
        };
        let id = id.to_string();
        Action::Run(Task::batch([
            task,
            Task::perform(
                async move { db::execute_query(&pool, &sql).await },
                move |r| ConnManagerMessage::ConnectionItemMessage(id, ItemMessage::QueryResult(r)),
            ),
        ]))
    }

    fn handle_edit_requested(&self, id: &str) -> Action {
        let cfg = match self.items.iter().find(|i| i.cfg.id == id) {
            Some(item) => item.cfg.clone(),
            None => return Action::None,
        };
        Action::Dialog(DialogMessage::OpenEdit(cfg))
    }

    fn handle_delete_requested(&mut self, id: &str) -> Action {
        self.items.retain(|i| i.cfg.id != id);
        if self.active_connection.as_deref() == Some(id) {
            self.active_connection = self
                .items
                .iter()
                .find(|i| i.pool.is_some())
                .map(|i| i.cfg.id.clone());
        }
        Action::Run(persist_connections(AppConfig::default(), &self.items)) // FIXME
    }

    fn handle_duplicate_requested(&mut self, id: &str) -> Action {
        if let Some(item) = self.items.iter().find(|i| i.cfg.id == id) {
            let mut new_cfg = item.cfg.clone();
            new_cfg.id = uuid::Uuid::new_v4().to_string();
            new_cfg.name = format!("{} (copy)", new_cfg.name);
            self.items.push(ConnectionItem::new(new_cfg));
            Action::Run(persist_connections(AppConfig::default(), &self.items)) // FIXME
        } else {
            Action::None
        }
    }

    fn handle_copy_string_requested(&self, id: &str) -> Action {
        if let Some(item) = self.items.iter().find(|i| i.cfg.id == id) {
            Action::Run(iced::clipboard::write(item.cfg.connection_string()))
        } else {
            Action::None
        }
    }

    fn handle_connect_completed(
        &mut self,
        id: String,
        result: Result<PgPool, String>,
    ) -> Action {
        match result {
            Ok(pool) => {
                self.active_connection = Some(id.clone());
                let id2 = id.clone();
                Action::Run(Task::batch([
                    self.delegate_to_item(&id, ItemMessage::ConnectSucceeded(pool.clone())),
                    Task::perform(
                        async move { db::fetch_schema_tree(&pool).await },
                        move |r| {
                            ConnManagerMessage::ConnectionItemMessage(
                                id2,
                                ItemMessage::SchemaLoaded(r),
                            )
                        },
                    ),
                ]))
            }
            Err(e) => Action::Run(self.delegate_to_item(&id, ItemMessage::ConnectFailed(e))),
        }
    }

    fn handle_dialog_message(
        &mut self,
        msg: DialogMessage,
    ) -> Action {
        if let DialogMessage::DialogSaved(cfg) = &msg {
            if let Some(existing) = self.items.iter_mut().find(|i| i.cfg.id == cfg.id) {
                let _ = existing.update(ItemMessage::UpdateConfig(cfg.clone()));
            } else {
                self.items.push(ConnectionItem::new(cfg.clone()));
            }

            Action::Run(persist_connections(AppConfig::default(), &self.items)) // FIXME
        } else {
            Action::Dialog(msg)
        }
    }

    fn delegate_to_item(&mut self, id: &str, msg: ItemMessage) -> Task<ConnManagerMessage> {
        let id = id.to_string();
        if let Some(item) = self.items.iter_mut().find(|i| i.cfg.id == id) {
            item.update(msg)
                .map(move |m| ConnManagerMessage::ConnectionItemMessage(id.clone(), m))
        } else {
            Task::none()
        }
    }
}

pub fn persist_connections(
    app_config: AppConfig,
    items: &[ConnectionItem],
) -> Task<ConnManagerMessage> {
    let configs: Vec<ConnectionConfig> = items.iter().map(|i| i.cfg.clone()).collect();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let updated_config = AppConfig {
                    connections: configs,
                    ..app_config
                };
                config_loader::save_config(&updated_config)
            })
            .await
            .context("Background task failed")
            .flatten()
            .map_err(|err| err.to_string())
        },
        ConnManagerMessage::ConnectionSaved,
    )
}
