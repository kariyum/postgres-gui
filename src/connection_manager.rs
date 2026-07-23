use anyhow::Context;
use iced::Task;
use sqlx::PgPool;

use crate::components::connection_dialog::{self, DialogMessage};
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
    pub fn update(
        &mut self,
        message: ConnManagerMessage,
        dialog: &mut connection_dialog::ConnectionDialog,
    ) -> Task<ConnManagerMessage> {
        match message {
            ConnManagerMessage::ConnectionItemMessage(id, msg) => {
                self.handle_item_message(&id, msg)
            }

            ConnManagerMessage::ConnectCompleted(id, result) => {
                self.handle_connect_completed(id, result)
            }

            ConnManagerMessage::ConnectionDialogMessage(msg) => {
                self.handle_dialog_message(msg, dialog)
            }

            ConnManagerMessage::ConnectionSaved(Ok(())) => Task::done(
                ConnManagerMessage::ConnectionDialogMessage(DialogMessage::DialogClose),
            ),

            ConnManagerMessage::ConnectionSaved(Err(e)) => {
                eprintln!("Failed to save connection: {e}");
                Task::none()
            }

            ConnManagerMessage::ConnectionsLoaded(configs) => {
                for cfg in configs {
                    self.items.push(ConnectionItem::new(cfg));
                }
                Task::none()
            }
        }
    }

    fn handle_item_message(&mut self, id: &str, msg: ItemMessage) -> Task<ConnManagerMessage> {
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
                Task::none()
            }
            _ => task,
        }
    }

    fn handle_connect_requested(&mut self, id: &str) -> Task<ConnManagerMessage> {
        let cs = match self.items.iter().find(|i| i.cfg.id == id) {
            Some(item) => item.cfg.connection_string(),
            None => return Task::none(),
        };
        let id = id.to_string();
        Task::perform(async move { db::connect(&cs).await }, move |result| {
            ConnManagerMessage::ConnectCompleted(id, result)
        })
    }

    fn handle_disconnect_requested(
        &mut self,
        id: &str,
        task: Task<ConnManagerMessage>,
    ) -> Task<ConnManagerMessage> {
        if self.active_connection.as_deref() == Some(id) {
            self.active_connection = self
                .items
                .iter()
                .find(|i| i.pool.is_some())
                .map(|i| i.cfg.id.clone());
        }
        task
    }

    fn handle_run_query(
        &self,
        id: &str,
        task: Task<ConnManagerMessage>,
    ) -> Task<ConnManagerMessage> {
        let (sql, pool) = match self.items.iter().find(|i| i.cfg.id == id) {
            Some(item) => (item.editor.text(), item.pool.clone()),
            None => {
                return Task::done(ConnManagerMessage::ConnectionItemMessage(
                    id.to_string(),
                    ItemMessage::QueryResult(Err("Connection item deleted?".to_string())),
                ));
            }
        };
        let pool = match pool {
            Some(p) => p,
            None => {
                return Task::done(ConnManagerMessage::ConnectionItemMessage(
                    id.to_string(),
                    ItemMessage::QueryResult(Err(
                        "Did not find connections to run the query".to_string()
                    )),
                ));
            }
        };
        let id = id.to_string();
        Task::batch([
            task,
            Task::perform(
                async move { db::execute_query(&pool, &sql).await },
                move |r| ConnManagerMessage::ConnectionItemMessage(id, ItemMessage::QueryResult(r)),
            ),
        ])
    }

    fn handle_edit_requested(&self, id: &str) -> Task<ConnManagerMessage> {
        let cfg = match self.items.iter().find(|i| i.cfg.id == id) {
            Some(item) => item.cfg.clone(),
            None => return Task::none(),
        };
        Task::done(ConnManagerMessage::ConnectionDialogMessage(
            DialogMessage::OpenEdit(cfg),
        ))
    }

    fn handle_delete_requested(&mut self, id: &str) -> Task<ConnManagerMessage> {
        self.items.retain(|i| i.cfg.id != id);
        if self.active_connection.as_deref() == Some(id) {
            self.active_connection = self
                .items
                .iter()
                .find(|i| i.pool.is_some())
                .map(|i| i.cfg.id.clone());
        }
        persist_connections(AppConfig::default(), &self.items) // FIXME
    }

    fn handle_duplicate_requested(&mut self, id: &str) -> Task<ConnManagerMessage> {
        if let Some(item) = self.items.iter().find(|i| i.cfg.id == id) {
            let mut new_cfg = item.cfg.clone();
            new_cfg.id = uuid::Uuid::new_v4().to_string();
            new_cfg.name = format!("{} (copy)", new_cfg.name);
            self.items.push(ConnectionItem::new(new_cfg));
            persist_connections(AppConfig::default(), &self.items) // FIXME
        } else {
            Task::none()
        }
    }

    fn handle_copy_string_requested(&self, id: &str) -> Task<ConnManagerMessage> {
        if let Some(item) = self.items.iter().find(|i| i.cfg.id == id) {
            iced::clipboard::write(item.cfg.connection_string())
        } else {
            Task::none()
        }
    }

    fn handle_connect_completed(
        &mut self,
        id: String,
        result: Result<PgPool, String>,
    ) -> Task<ConnManagerMessage> {
        match result {
            Ok(pool) => {
                self.active_connection = Some(id.clone());
                let id2 = id.clone();
                Task::batch([
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
                ])
            }
            Err(e) => self.delegate_to_item(&id, ItemMessage::ConnectFailed(e)),
        }
    }

    fn handle_dialog_message(
        &mut self,
        msg: DialogMessage,
        dialog: &mut connection_dialog::ConnectionDialog,
    ) -> Task<ConnManagerMessage> {
        if let DialogMessage::DialogSaved(cfg) = &msg {
            if let Some(existing) = self.items.iter_mut().find(|i| i.cfg.id == cfg.id) {
                let _ = existing.update(ItemMessage::UpdateConfig(cfg.clone()));
            } else {
                self.items.push(ConnectionItem::new(cfg.clone()));
            }

            let task = dialog.update(msg);
            Task::batch([
                task.map(ConnManagerMessage::ConnectionDialogMessage),
                persist_connections(AppConfig::default(), &self.items), // FIXME
            ])
        } else {
            let task = dialog.update(msg);
            task.map(ConnManagerMessage::ConnectionDialogMessage)
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
