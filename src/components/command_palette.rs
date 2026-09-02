use iced::{
    Element, Length, Task,
    widget::{
        Column, Container, button, column, container, mouse_area, operation, row, rule, scrollable,
        svg, text, text_input,
    },
};

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            is_visible: false,
            search_query: String::from(""),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Hide,
    InputChanged(String),
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        if self.is_visible {
            Some(
                container(column![
                    text_input("Search", self.search_query.as_str())
                        .on_input(Message::InputChanged)
                        .id("search_box"),
                    text("Hi, I'm a command palette.")
                ])
                .height(300)
                .width(500)
                .style(|theme: &iced::Theme| {
                    let palette = theme.palette();
                    container::Style {
                        background: Some(iced::Background::Color(palette.background.weakest.color)),
                        ..Default::default()
                    }
                })
                .into(),
            )
        } else {
            None
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        tracing::info!("Got message {:?}", message);
        match message {
            Message::Toggle => {
                self.is_visible = !self.is_visible;
                if self.is_visible {
                    operation::focus("search_box")
                } else {
                    Task::none()
                }
            }
            Message::Hide => {
                self.is_visible = false;
                Task::none()
            }
            Message::InputChanged(str) => {
                self.search_query = str;
                Task::none()
            }
        }
    }
}
