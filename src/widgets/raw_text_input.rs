use std::time::{Duration, Instant};

use iced::advanced::layout::Node;
use iced::advanced::renderer;
use iced::advanced::text::{self, Paragraph as _};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{Layout, Shell, Widget, mouse};
use iced::{Element, Event, Length, Rectangle, Size, Theme, keyboard, window};

const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const TEXT_SIZE: f32 = 12.0;
const HORIZONTAL_PADDING: f32 = 4.0;

#[derive(Debug, Clone, Default)]
pub struct RawTextInput {
    placeholder: String,
    value: String,
}

impl RawTextInput {
    pub fn new(placeholder: impl Into<String>) -> Self {
        Self {
            placeholder: placeholder.into(),
            value: String::new(),
        }
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    fn content(&self) -> &str {
        if self.value.is_empty() {
            self.placeholder.as_str()
        } else {
            self.value.as_str()
        }
    }
}

#[derive(Debug)]
struct State {
    now: Instant,
    updated_at: Instant,
}

impl Default for State {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            now,
            updated_at: now,
        }
    }
}

impl State {
    fn caret_visible(&self) -> bool {
        let elapsed = (self.now - self.updated_at).as_millis();
        (elapsed / CARET_BLINK_INTERVAL.as_millis()) % 2 == 0
    }
}

fn text_config(content: &str) -> text::Text<&str, iced::Font> {
    text::Text {
        content,
        bounds: Size::ZERO,
        size: iced::Pixels(TEXT_SIZE),
        line_height: text::LineHeight::Relative(1.0),
        font: iced::Font::DEFAULT,
        align_x: text::Alignment::Left,
        align_y: iced::alignment::Vertical::Center,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::None,
        hint_factor: None,
    }
}

impl RawTextInput {
    fn draw_text<R>(&self, renderer: &mut R, bounds: Rectangle)
    where
        R: text::Renderer<Font = iced::Font>,
    {
        let content = self.content();
        let position = iced::Point {
            x: bounds.x + HORIZONTAL_PADDING,
            y: bounds.center_y(),
        };

        renderer.fill_text(
            text_config(content).with_content(content.to_owned()),
            position,
            iced::Color::WHITE,
            bounds,
        );
    }

    fn draw_caret<R>(&self, state: &State, renderer: &mut R, bounds: Rectangle)
    where
        R: text::Renderer<Font = iced::Font>,
    {
        if !state.caret_visible() {
            return;
        }

        let caret_x = if self.value.is_empty() {
            bounds.x + HORIZONTAL_PADDING
        } else {
            bounds.x
                + HORIZONTAL_PADDING
                + R::Paragraph::with_text(text_config(self.content())).min_width() + 1.0
        };

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: caret_x,
                    y: bounds.y + 8.0,
                    width: 1.0,
                    height: bounds.height - 16.0,
                },
                ..renderer::Quad::default()
            },
            iced::Color::WHITE,
        );
    }
}

impl<Message, R> Widget<Message, iced::Theme, R> for RawTextInput
where
    R: text::Renderer<Font = iced::Font>,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> iced::Size<iced::Length> {
        Size {
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &R,
        limits: &iced::advanced::layout::Limits,
    ) -> Node {
        Node::new(iced::Size::new(limits.max().width, 30.0))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &R,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;

                let interval = CARET_BLINK_INTERVAL.as_millis();
                let elapsed = (*now - state.updated_at).as_millis();
                let millis_until_next = interval - elapsed % interval;

                shell.request_redraw_at(*now + Duration::from_millis(millis_until_next as u64));
            }
            Event::Keyboard(keyboard::Event::KeyPressed { .. }) => {
                state.updated_at = Instant::now();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &R,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<State>();

        self.draw_text(renderer, bounds);
        self.draw_caret(state, renderer, bounds);
    }
}

impl<Message> From<RawTextInput> for Element<'_, Message> {
    fn from(value: RawTextInput) -> Self {
        Self::new(value)
    }
}
