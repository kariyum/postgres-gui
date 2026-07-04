use iced::border::Radius;
use iced::widget::space::{horizontal, vertical};
use iced::widget::{
    Row, button, column, container, row, rule, scrollable, svg, text, text_editor, text_input,
};
use iced::{Background, Border, Color, Element, Length, Task, Theme};

use crate::app::Message;

#[derive(Clone, Default, Debug)]
pub struct AIChat {
    visible: bool,
    input: text_editor::Content,
    error: Option<String>,
    messages: Vec<ChatMsg>,
}

#[derive(Clone, Debug)]
pub struct ChatMsg {
    pub role: Role,
    pub content: String,
}

#[derive(Clone, Debug)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug)]
pub enum AIChatMessage {
    TogglePanel,
    EditorAction(text_editor::Action),
    Send,
}

impl AIChat {
    fn view_messages(&self) -> Element<'_, AIChatMessage> {
        vertical().height(Length::Fill).into()
    }

    fn view_actions(&self) -> Element<'_, AIChatMessage> {
        container(row![
            horizontal(),
            button(
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/send.svg"
                )))
                .height(14)
                .width(14)
            )
            .on_press(AIChatMessage::Send)
            .style(|_theme, _status| button::Style {
                background: Some(iced::Background::Color(Color::TRANSPARENT)),
                ..Default::default()
            })
        ])
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(
                _theme.extended_palette().background.weakest.color,
            )),
            ..Default::default()
        })
        .into()
    }

    fn view_editor(&self) -> Element<'_, AIChatMessage> {
        text_editor(&self.input)
            .placeholder("How many active users do I have?")
            .on_action(AIChatMessage::EditorAction)
            .style(|_theme: &Theme, _status| text_editor::Style {
                background: Background::Color(_theme.extended_palette().background.weakest.color),
                border: Border {
                    color: Color::TRANSPARENT,
                    radius: Radius::new(0),
                    width: 0.0,
                },
                ..text_editor::default(_theme, _status)
            })
            .min_height(80)
            .max_height(200)
            .into()
    }

    pub fn view(&self) -> Element<'_, AIChatMessage> {
        let layout = column![
            container(text("AI Chat").size(14)).padding([4.0, 8.0]),
            rule::horizontal(1.0),
            scrollable(self.view_messages()).height(Length::Fill),
            rule::horizontal(1.0),
            self.view_editor(),
            self.view_actions()
        ];
        layout.into()
    }

    pub fn update(&mut self, message: AIChatMessage) -> Task<Message> {
        match message {
            AIChatMessage::TogglePanel => {
                self.visible = !self.visible;
                Task::none()
            }
            AIChatMessage::EditorAction(action) => {
                self.input.perform(action);
                Task::none()
            }
            AIChatMessage::Send => {
                todo!()
            }
        }
    }
}
