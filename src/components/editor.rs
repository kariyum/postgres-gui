use iced::{
    Element, Length, Task,
    widget::{self, Row, button, column, container, operation, row, scrollable, space, text},
};

use crate::components::{
    editor::Message::EditorConfigMessage,
    editor_config::{self, EditorConfig, EditorId},
};

#[derive(Debug, Clone)]
pub struct Editor {
    windows: Vec<EditorConfig>,
    focused: Option<EditorId>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Add(EditorConfig),
    Close(EditorId),
    Focus(EditorId),
    NewRequested,
    EditorConfigMessage(EditorId, editor_config::Message),
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
            focused: None,
        }
    }
}

impl Editor {
    fn index_of(&self, id: EditorId) -> Option<usize> {
        self.windows.iter().position(|config| config.id() == id)
    }

    fn focused_window(&self) -> Option<&EditorConfig> {
        self.focused
            .and_then(|id| self.windows.iter().find(|config| config.id() == id))
            .or(self.windows.last())
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
        match self.focused_window() {
            Some(window) => {
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

            None => {
                tracing::error!("Did not find EditorConfig in self.windows");
                space().into()
            }
        }
    }

    fn view_header(&self) -> Element<'_, Message> {
        let tabs = scrollable(
            Row::from_vec(
                self.windows
                    .iter()
                    .map(|window| {
                        let id = window.id();
                        window
                            .view_header(self.focused == Some(id))
                            .map(move |msg| Message::EditorConfigMessage(id, msg))
                            .into()
                    })
                    .collect(),
            )
            .spacing(1),
        )
        .direction(widget::scrollable::Direction::Horizontal(
            widget::scrollable::Scrollbar::default(),
        ))
        .width(Length::Fill);

        let new_button = button(text("+").size(14))
            .on_press(Message::NewRequested)
            .padding([0, 8])
            .style(button::subtle);

        container(row![tabs, new_button].align_y(iced::Alignment::Center))
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
                let id = (&editor_config).id();
                self.windows.push(editor_config);
                self.focus_editor(id)
            }
            Message::Close(id) => {
                if let Some(idx) = self.index_of(id) {
                    self.windows.remove(idx);
                    if self.focused == Some(id) {
                        let neighbour = if idx > 0 {
                            self.windows.get(idx - 1)
                        } else {
                            self.windows.get(idx)
                        };
                        self.focused = neighbour.map(|config| config.id());
                    }
                }
                Task::none()
            }
            Message::Focus(id) => {
                if self.index_of(id).is_some() {
                    self.focus_editor(id)
                } else {
                    Task::none()
                }
            }
            Message::NewRequested => Task::none(),
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

    fn focus_editor(&mut self, editor_id: EditorId) -> Task<Message> {
        self.focused = Some(editor_id);
        operation::focus(editor_id.id())
    }
}
