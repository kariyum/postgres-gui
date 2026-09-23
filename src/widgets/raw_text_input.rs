use iced::advanced::layout::Node;
use iced::advanced::text;
use iced::advanced::{Layout, Widget};
use iced::{Element, Length, Size, Theme};

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
}

impl<Message, R> Widget<Message, iced::Theme, R> for RawTextInput
where
    R: text::Renderer<Font = iced::Font>,
{
    fn size(&self) -> iced::Size<iced::Length> {
        Size {
            width: Length::Fill,
            height: Length::Shrink,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &R,
        limits: &iced::advanced::layout::Limits,
    ) -> iced::advanced::layout::Node {
        Node::new(iced::Size::new(limits.max().width, 30.0))
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        renderer: &mut R,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        _cursor: iced::advanced::mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        let content = if self.value.is_empty() {
            &self.placeholder
        } else {
            &self.value
        };

        renderer.fill_text(
            text::Text {
                content: content.clone(),
                bounds: Size::ZERO,
                size: iced::Pixels(12.0),
                line_height: text::LineHeight::Relative(1.0),
                font: iced::Font::DEFAULT,
                align_x: text::Alignment::Left,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
                ellipsis: text::Ellipsis::None,
                hint_factor: None,
            },
            iced::Point {
                x: layout.bounds().x + 4.0,
                y: layout.bounds().center_y(),
            },
            iced::Color::WHITE,
            layout.bounds(),
        );
    }
}

impl<Message> From<RawTextInput> for Element<'_, Message> {
    fn from(value: RawTextInput) -> Self {
        Self::new(value)
    }
}
