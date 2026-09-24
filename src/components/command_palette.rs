use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::widget::{Column, button, container, rule};
use iced::{
    Alignment, Background, Element, Event, Length, Padding, Rectangle, Size, Task, Theme, Vector,
    keyboard,
    widget::{operation, text, text_input},
};

use crate::widgets::raw_text_input::RawTextInput;

const PALETTE_WIDTH: f32 = 500.0;
const PALETTE_HEIGHT: f32 = 300.0;

#[derive(Debug, Clone)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
    commands: Vec<Command>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            is_visible: false,
            search_query: String::new(),
            commands: vec![
                Command {
                    label: String::from("Explore Schema"),
                    message: Message::ExploreSchema,
                },
                Command {
                    label: String::from("Connect to"),
                    message: Message::ConnectTo,
                },
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Hide,
    InputChanged(String),
    ExploreSchema,
    ConnectTo,
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        self.is_visible
            .then(|| Palette::new(&self.search_query, &self.commands).into())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Toggle => {
                self.is_visible = !self.is_visible;
                Task::none()
            }
            Message::Hide => {
                self.is_visible = false;
                self.search_query.clear();
                Task::none()
            }
            Message::InputChanged(str) => {
                self.search_query = str;
                Task::none()
            }
            Message::ExploreSchema => Task::none(),
            Message::ConnectTo => Task::none(),
        }
    }
}

fn delete_last_word(value: &mut String) {
    let trimmed = value.trim_end().len();
    value.truncate(trimmed);

    match value.rfind(|c: char| c.is_whitespace()) {
        Some(index) => value.truncate(index + 1),
        None => value.clear(),
    }
}

struct Palette<'a> {
    children: Vec<Element<'a, Message>>,
    input_value: String,
    width: Length,
    height: Length,
    padding: Padding,
    spacing: f32,
}

#[derive(Debug, Clone)]
struct Command {
    label: String,
    message: Message,
}

impl<'a> Palette<'a> {
    fn new(search_query: &'a str, commands: &'a Vec<Command>) -> Self {
        Self {
            children: vec![
                container(RawTextInput::new("Search").value(search_query))
                    .padding([0, 4])
                    .into(),
                rule::horizontal(1.0).into(),
                Column::from_vec(
                    commands
                        .iter()
                        .map(|command| {
                            button(text(command.label.clone()).size(12))
                                .on_press(command.message.clone())
                                .width(Length::Fill)
                                .style(|theme, status| button::Style {
                                    ..button::primary(theme, status)
                                })
                                .padding([4, 4])
                                .into()
                        })
                        .collect(),
                )
                .spacing(2)
                .padding(4)
                .into(),
            ],
            input_value: search_query.to_string(),
            width: Length::Fixed(PALETTE_WIDTH),
            height: Length::Fixed(PALETTE_HEIGHT),
            padding: Padding::ZERO,
            spacing: 0.0,
        }
    }
}

impl Widget<Message, Theme, iced::Renderer> for Palette<'_> {
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut self.children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::flex::resolve(
            layout::flex::Axis::Vertical,
            renderer,
            limits,
            self.width,
            self.height,
            self.padding,
            self.spacing,
            Alignment::Start,
            &mut self.children,
            &mut tree.children,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            text,
            modifiers,
            ..
        }) = event
        {
            match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                    if modifiers.control() {
                        delete_last_word(&mut self.input_value);
                    } else {
                        self.input_value.pop();
                    }
                    shell.publish(Message::InputChanged(self.input_value.clone()));
                }
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    // Ignore Enter for now; it will be wired up later.
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    shell.publish(Message::InputChanged(self.input_value.clone()));
                }
                _ if modifiers.control() => {}
                _ => {
                    if let Some(text) = text {
                        self.input_value.push_str(text);
                        shell.publish(Message::InputChanged(self.input_value.clone()));
                    }
                }
            }
        }

        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child
                .as_widget_mut()
                .update(state, event, layout, cursor, renderer, shell, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(clipped_viewport) = bounds.intersection(viewport) else {
            return;
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border {
                    color: theme.palette().background.strongest.color,
                    width: 1.0,
                    radius: iced::border::radius(5.0),
                },
                ..Default::default()
            },
            Background::Color(theme.palette().background.weakest.color),
        );

        renderer.with_layer(bounds, |renderer| {
            for ((child, state), layout) in self
                .children
                .iter()
                .zip(&tree.children)
                .zip(layout.children())
                .filter(|(_, layout)| layout.bounds().intersects(&clipped_viewport))
            {
                child
                    .as_widget()
                    .draw(state, renderer, theme, style, layout, cursor, viewport);
            }
        });
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, iced::Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a> From<Palette<'a>> for Element<'a, Message> {
    fn from(palette: Palette<'a>) -> Self {
        Self::new(palette)
    }
}
