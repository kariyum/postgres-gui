use iced::{Element, Length, Task, widget::container};

use crate::{
    components::editor,
    core::connection_config::{self, ConnectionConfig},
};

#[derive(Debug, Clone)]
pub struct EditorConfig {
    config: ConnectionConfig,
    // database_keeper:
    // query result
    // query filters
    // query state (idle, running, finished ...)
}

impl EditorConfig {
    pub fn new(connection_config: ConnectionConfig) -> Self {
        Self {
            config: connection_config,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Editor {
    windows: Vec<EditorConfig>,
    focused_tab_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Add(EditorConfig),
    Close(EditorConfig),
    Focus(EditorConfig),
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
            focused_tab_index: None,
        }
    }
}

impl Editor {
    fn index_of(&self, editor_config: EditorConfig) -> Option<usize> {
        self.windows.iter().position(|config| {
            config.config.connection_string() == editor_config.config.connection_string()
        })
    }
    pub fn view(&self) -> Option<Element<'_, Message>> {
        if self.windows.is_empty() {
            None
        } else {
            Some(
                container("Hi I'm the editor")
                    .height(Length::Fill)
                    .width(Length::Fill)
                    .into(),
            )
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Add(editor_config) => {
                self.windows.push(editor_config);
                Task::none()
            }
            Message::Close(editor_config) => {
                if let Some(idx) = self.index_of(editor_config) {
                    self.windows.remove(idx);
                }
                Task::none()
            }
            Message::Focus(editor_config) => {
                if let Some(index) = self.index_of(editor_config) {
                    self.focused_tab_index = Some(index);
                }
                Task::none()
            }
        }
    }
}
