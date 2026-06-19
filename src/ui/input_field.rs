use iced::{Element, Length, Task};

use crate::theme;
use iced::widget::{Column, column, text, text_input};

#[derive(Debug, Clone)]
pub enum InputFieldMessage {
    InputChanged(String),
    Noop,
}

#[derive(Debug, Clone)]
pub struct InputField {
    pub value: String,
    label: String,
    placeholder: String,
    is_secure: bool,
    oninput: fn(String) -> InputFieldMessage,
    width: Length,
}

impl Default for InputField {
    fn default() -> Self {
        Self {
            label: String::default(),
            placeholder: String::default(),
            value: String::default(),
            is_secure: false,
            oninput: InputFieldMessage::InputChanged,
            width: Length::Fill,
        }
    }
}

impl InputField {
    pub fn secure(mut self, value: bool) -> Self {
        self.is_secure = value;
        self
    }

    pub fn oninput(mut self, handler: fn(String) -> InputFieldMessage) -> Self {
        self.oninput = handler;
        self
    }

    pub fn placeholder(mut self, value: String) -> Self {
        self.placeholder = value;
        self
    }

    pub fn label(mut self, value: String) -> Self {
        self.label = value;
        self
    }

    pub fn value(mut self, value: String) -> Self {
        self.value = value;
        self
    }

    pub fn width(mut self, length: Length) -> Self {
        self.width = length;
        self
    }

    pub fn view(&self) -> Element<'_, InputFieldMessage> {
        column![
            text(self.label.as_str()).size(12).color(theme::TEXT_MUTED),
            text_input(self.placeholder.as_str(), self.value.as_str())
                .on_input(self.oninput)
                .secure(self.is_secure)
                .padding(8)
                .size(14)
                .width(self.width),
        ]
        .spacing(4)
        .into()
    }

    pub fn update(&mut self, message: InputFieldMessage) -> Task<InputFieldMessage> {
        match message {
            InputFieldMessage::InputChanged(new_value) => {
                self.value = new_value;
                Task::none()
            }
            InputFieldMessage::Noop => Task::none(),
        }
    }
}
