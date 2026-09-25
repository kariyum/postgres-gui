use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::widget::{Column, container, rule, scrollable};
use iced::{
    Alignment, Background, Element, Event, Length, Padding, Point, Rectangle, Size, Theme, Vector,
    keyboard,
};

use crate::widgets::raw_text_input::RawTextInput;

const PALETTE_WIDTH: f32 = 500.0;
const PALETTE_HEIGHT: f32 = 300.0;

/// Height of a single command row. Matches the row button's fixed height in
/// `command_palette.rs` and is used to compute scroll targets.
pub const ROW_HEIGHT: f32 = 24.0;
/// Vertical spacing between command rows.
pub const ROW_SPACING: f32 = 2.0;
/// Vertical padding applied to the command list column.
pub const LIST_PADDING: f32 = 4.0;
/// Distance between the top of one row and the top of the next.
pub const ROW_STRIDE: f32 = ROW_HEIGHT + ROW_SPACING;
/// Widget id for the scrollable command list, used to scroll it into view.
pub const SCROLLABLE_ID: &'static str = "command_palette_scrollable";

pub struct Palette<'a, Message>
where
    Message: 'a,
{
    children: Vec<Element<'a, Message>>,
    input_value: String,
    on_input: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_select_next: Option<Message>,
    on_select_previous: Option<Message>,
    on_hover: Option<Box<dyn Fn(usize) -> Message + 'a>>,
    scroll_offset: f32,
    width: Length,
    height: Length,
    padding: Padding,
    spacing: f32,
}

fn delete_last_word(value: &mut String) {
    let trimmed = value.trim_end().len();
    value.truncate(trimmed);

    match value.rfind(|c: char| c.is_whitespace()) {
        Some(index) => value.truncate(index + 1),
        None => value.clear(),
    }
}

impl<'a, Message> Palette<'a, Message>
where
    Message: Clone + 'a,
{
    pub fn new(
        search_query: &'a str,
        rows: Vec<Element<'a, Message>>,
        on_scroll: impl Fn(scrollable::Viewport) -> Message + 'a,
    ) -> Self {
        Self {
            children: vec![
                container(RawTextInput::new("Search").value(search_query))
                    .padding([0, 4])
                    .into(),
                rule::horizontal(1.0).into(),
                scrollable(
                    Column::from_vec(rows)
                        .spacing(ROW_SPACING)
                        .padding(LIST_PADDING),
                )
                .id(SCROLLABLE_ID)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::hidden(),
                ))
                .height(Length::Fill)
                .on_scroll(on_scroll)
                .into(),
            ],
            input_value: search_query.to_string(),
            on_input: None,
            on_select_next: None,
            on_select_previous: None,
            on_hover: None,
            scroll_offset: 0.0,
            width: Length::Fixed(PALETTE_WIDTH),
            height: Length::Fixed(PALETTE_HEIGHT),
            padding: Padding::ZERO,
            spacing: 0.0,
        }
    }

    pub fn on_input(mut self, on_input: impl Fn(String) -> Message + 'a) -> Self {
        self.on_input = Some(Box::new(on_input));
        self
    }

    pub fn on_select_next(mut self, message: Message) -> Self {
        self.on_select_next = Some(message);
        self
    }

    pub fn on_select_previous(mut self, message: Message) -> Self {
        self.on_select_previous = Some(message);
        self
    }

    pub fn on_hover(mut self, on_hover: impl Fn(usize) -> Message + 'a) -> Self {
        self.on_hover = Some(Box::new(on_hover));
        self
    }

    pub fn scroll_offset(mut self, offset: f32) -> Self {
        self.scroll_offset = offset;
        self
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Palette<'_, Message>
where
    Message: Clone,
{
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

                    if let Some(on_input) = &self.on_input {
                        shell.publish(on_input(self.input_value.clone()));
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    // Ignore Enter for now; it will be wired up later.
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                    if let Some(message) = &self.on_select_next {
                        shell.publish(message.clone());
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                    if let Some(message) = &self.on_select_previous {
                        shell.publish(message.clone());
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    if let Some(on_input) = &self.on_input {
                        shell.publish(on_input(self.input_value.clone()));
                    }
                }
                _ if modifiers.control() => {}
                _ => {
                    if let Some(text) = text {
                        self.input_value.push_str(text);

                        if let Some(on_input) = &self.on_input {
                            shell.publish(on_input(self.input_value.clone()));
                        }
                    }
                }
            }
        }

        if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
            if let Some(position) = cursor.position() {
                let content_position = Point::new(position.x, position.y + self.scroll_offset);

                if let Some(content) = layout.child(2).children().next() {
                    let index = content
                        .children()
                        .enumerate()
                        .find(|(_, row)| row.bounds().contains(content_position))
                        .map(|(index, _)| index);

                    if let Some(index) = index {
                        if let Some(on_hover) = &self.on_hover {
                            shell.publish(on_hover(index));
                        }
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

impl<'a, Message> From<Palette<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(palette: Palette<'a, Message>) -> Self {
        Self::new(palette)
    }
}
