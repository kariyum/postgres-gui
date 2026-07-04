use iced::border::Radius;
use iced::widget::space::horizontal;
use iced::widget::{
    Row, button, column, container, row, rule, scrollable, text, text_editor, text_input,
};
use iced::{Background, Border, Color, Element, Length, Task, Theme};
use iced_aw::widget::labeled_frame::Catalog;

use crate::app::Message;
use crate::core::connection_config::ConnectionConfig;
use crate::theme;
use crate::ui::input_field::{InputField, InputFieldMessage};

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
    pub fn view(&self) -> Element<'_, AIChatMessage> {
        let layout = column![
            container(text("AI Chat").size(14)).padding([4.0, 8.0]),
            rule::horizontal(1.0),
            horizontal().height(Length::Fill),
            rule::horizontal(1.0),
            text_editor(&self.input)
                .placeholder("How many active users do I have?")
                .on_action(AIChatMessage::EditorAction)
                .style(|_theme: &Theme, _status| text_editor::Style {
                    background: Background::Color(
                        _theme.extended_palette().background.weakest.color
                    ),
                    border: Border {
                        color: Color::TRANSPARENT,
                        radius: Radius::new(0),
                        width: 0.0,
                    },
                    ..text_editor::default(_theme, _status)
                })
                .min_height(80)
                .max_height(200)
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
