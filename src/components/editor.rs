use anyhow::Context;
use iced::{
    Element, Length, Task,
    widget::{self, Row, column, container, scrollable, space},
};

use crate::components::editor_config::{self, EditorConfig, EditorId};

#[derive(Debug, Clone)]
pub struct Editor {
    windows: Vec<EditorConfig>,
    focused_tab_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Add(EditorConfig),
    Close(EditorId),
    Focus(EditorId),
    EditorConfigMessage(EditorId, editor_config::Message),
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
    fn index_of(&self, id: EditorId) -> Option<usize> {
        self.windows.iter().position(|config| config.id() == id)
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
            Ok(window) => {
                let id = window.id();
                column![
                    self.view_header(),
                    window
                        .view()
                        .map(move |msg| Message::EditorConfigMessage(id, msg))
                ]
                .spacing(0)
                .padding(0)
                .into()
            }

            Err(err) => {
                tracing::error!("{err}");
                return space().into();
            }
        }
    }

    fn view_header(&self) -> Element<'_, Message> {
        container(
            scrollable(
                Row::from_vec(
                    self.windows
                        .iter()
                        .map(|window| {
                            let id = window.id();
                            window
                                .view_header()
                                .map(move |msg| Message::EditorConfigMessage(id, msg))
                                .into()
                        })
                        .collect(),
                )
                .spacing(1),
            )
            .direction(widget::scrollable::Direction::Horizontal(
                widget::scrollable::Scrollbar::default(),
            )),
        )
        .width(Length::Fill)
        .height(30)
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
            Message::Close(id) => {
                if let Some(idx) = self.index_of(id) {
                    self.windows.remove(idx);
                }
                Task::none()
            }
            Message::Focus(id) => {
                if let Some(index) = self.index_of(id) {
                    self.focused_tab_index = Some(index);
                }
                Task::none()
            }
            Message::EditorConfigMessage(id, msg) => match msg {
                editor_config::Message::Select => Task::done(Message::Focus(id)),
                editor_config::Message::Close => Task::done(Message::Close(id)),
                _ => {
                    if let Some(idx) = self.index_of(id) {
                        self.windows[idx]
                            .update(msg)
                            .map(move |msg| Message::EditorConfigMessage(id, msg))
                    } else {
                        Task::none()
                    }
                }
            },
        }
    }
}
