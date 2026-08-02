use iced::{
    Element, Length, Task,
    widget::{Row, column, container, row, rule},
};

use crate::{
    components::{
        editor,
        editor_config::{self, EditorConfig},
    },
    core::connection_config::{self, ConnectionConfig},
};

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
    EditorConfigMessage(editor_config::Message),
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
        column![
            self.view_header(),
            container("Editor here")
                .width(Length::Fill)
                .height(Length::Fill)
        ]
        .into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        container(
            Row::from_vec(
                self.windows
                    .iter()
                    .map(|window| {
                        window
                            .view_header()
                            .map(Message::EditorConfigMessage)
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
            Message::EditorConfigMessage(msg) => Task::none(), // TODO update me
        }
    }
}
