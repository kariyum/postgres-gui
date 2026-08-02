use crate::core::connection_config::ConnectionConfig;
use iced::{
    Background, Color, Element, Length, Task, Theme, alignment, theme,
    widget::{button, column, container, row, svg, text, text_editor},
};

#[derive(Debug, Clone)]
pub struct EditorConfig {
    config: ConnectionConfig,
    database: String,
    editor: text_editor::Content,
    // database_keeper:
    // query result
    // query filters
    // query state (idle, running, finished ...)
}

#[derive(Debug, Clone)]
pub enum Message {
    Select,
    Close,
    EditorAction(text_editor::Action),
}

impl EditorConfig {
    pub fn new(connection_config: ConnectionConfig) -> Self {
        Self {
            database: connection_config.database.clone(),
            config: connection_config,
            editor: text_editor::Content::new(),
        }
    }

    pub fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Select => Task::none(),
            Message::Close => Task::none(),
            Message::EditorAction(action) => {
                self.editor.perform(action);
                Task::none()
            }
        }
    }

    pub fn connection_string(&self) -> String {
        self.config.connection_string()
    }

    pub fn view_header(&self) -> Element<'_, Message> {
        let close_btn = button(
            svg(svg::Handle::from_memory(include_bytes!(
                "../resources/x.svg"
            )))
            .height(14)
            .width(14),
        )
        .on_press(Message::Close)
        .height(Length::Fit)
        .width(Length::Fit)
        .padding([2, 2])
        .style(|theme: &Theme, status| {
            let palette = theme.palette();
            button::Style {
                background: if matches!(status, button::Status::Hovered) {
                    Some(palette.background.weaker.color.into())
                } else {
                    None
                },
                ..Default::default()
            }
        });
        button(
            row![text(&self.database), close_btn]
                .align_y(alignment::Alignment::Center)
                .spacing(6),
        )
        .on_press(Message::Select)
        .padding([4, 6])
        .style(|_theme, _status| button::Style {
            border: iced::Border::default().width(0),
            ..button::background(_theme, _status)
        })
        .into()
    }

    pub fn view_editor(&self) -> Element<'_, Message> {
        container(column![self.view_toolbar(), self.view_query_editor()])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_toolbar(&self) -> Element<'_, Message> {
        let run_btn = button(
            svg(svg::Handle::from_memory(include_bytes!(
                "../resources/play.svg"
            )))
            .height(12)
            .width(12),
        );
        container(
            row![
                run_btn,
                iced::widget::Space::new().width(Length::Fill),
                text(self.connection_string())
                    .size(11)
                    .color(crate::theme::TEXT_MUTED),
            ]
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 12])
        .into()
    }

    fn view_query_editor(&self) -> Element<'_, Message> {
        container(
            text_editor(&self.editor)
                .on_action(Message::EditorAction)
                .highlight("sql", iced::highlighter::Theme::Base16Eighties)
                .height(Length::FillPortion(1))
                .font(iced::Font::MONOSPACE)
                .size(14)
                .style(|_theme, _status| text_editor::Style {
                    background: Background::Color(Color::TRANSPARENT),
                    border: iced::Border {
                        color: Color::TRANSPARENT,
                        width: 0.0,
                        radius: iced::border::Radius::new(0),
                    },
                    ..text_editor::default(_theme, _status)
                }),
        )
        .style(|theme: &Theme| {
            let palette = theme.palette();
            iced::widget::container::Style {
                background: Some(palette.background.base.color.into()),
                border: iced::Border {
                    color: Color::TRANSPARENT,
                    width: 0.0,
                    radius: iced::border::Radius::new(0),
                },
                text_color: Some(Color::from_rgb(1.0, 0.0, 0.0)),
                ..Default::default()
            }
        })
        .into()
    }
}
