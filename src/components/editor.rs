use anyhow::Context;
use iced::{
    Element, Length, Task,
    widget::{Row, column, container, space},
};

use crate::components::editor_config::{self, EditorConfig};

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
    EditorConfigMessage(EditorConfig, editor_config::Message), // TODO track only window ID
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
        self.windows
            .iter()
            .position(|config| config.connection_string() == editor_config.connection_string())
    }

    pub fn view(&self) -> Option<Element<'_, Message>> {
        if self.windows.is_empty() {
            None
        } else {
            Some(
                container(self.view_editor())
                    .height(Length::Fill)
                    .width(Length::Fill)
                    .into(),
            )
        }
    }

    fn view_editor(&self) -> Element<'_, Message> {
        let window = self
            .windows
            .get(self.focused_tab_index.unwrap_or(0))
            .context("Did not find EditorConfig in self.windows");
        match window {
            Ok(window) => column![
                self.view_header(),
                container(
                    window
                        .view_editor()
                        .map(|msg| Message::EditorConfigMessage(window.clone(), msg))
                )
                .width(Length::Fill)
                .height(Length::Fill)
            ]
            .into(),

            Err(err) => {
                tracing::error!("{err}");
                return space().into();
            }
        }
    }

    fn view_header(&self) -> Element<'_, Message> {
        container(
            Row::from_vec(
                self.windows
                    .iter()
                    .map(|window| {
                        window
                            .view_header()
                            .map(|msg| Message::EditorConfigMessage(window.clone(), msg))
                            .into()
                    })
                    .collect(),
            )
            .spacing(1),
        )
        .width(Length::Fill)
        .style(|theme: &iced::Theme| {
            let palette = theme.palette();
            container::Style {
                background: Some(palette.background.weakest.color.into()),
                border: iced::Border::default().width(0),
                ..Default::default()
            }
        })
        .into()
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
            Message::EditorConfigMessage(editor_config, msg) => match msg {
                editor_config::Message::Select => Task::done(Message::Focus(editor_config)),
                editor_config::Message::Close => Task::done(Message::Close(editor_config)),
                _ => {
                    if let Some(idx) = self.windows.iter().position(|win| {
                        win.connection_string() == editor_config.connection_string()
                    }) {
                        self.windows[idx].update(msg).map(move |msg| {
                            Message::EditorConfigMessage(editor_config.clone(), msg)
                        })
                    } else {
                        Task::none()
                    }
                }
            },
        }
    }
}
