use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::{
    Alignment, Background, Element, Event, Length, Padding, Rectangle, Size, Task, Theme, Vector,
    widget::{operation, text, text_input},
};

const SEARCH_BOX_ID: &str = "search_box";
const PALETTE_WIDTH: f32 = 500.0;
const PALETTE_HEIGHT: f32 = 300.0;

#[derive(Debug, Clone, Default)]
pub struct CommandPalette {
    is_visible: bool,
    search_query: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Toggle,
    Hide,
    InputChanged(String),
}

impl CommandPalette {
    pub fn view(&self) -> Option<Element<'_, Message>> {
        self.is_visible
            .then(|| Palette::new(&self.search_query).into())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        tracing::info!("Got message {:?}", message);
        match message {
            Message::Toggle => {
                self.is_visible = !self.is_visible;
                if self.is_visible {
                    operation::focus(SEARCH_BOX_ID)
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

/// The panel of a [`CommandPalette`]: a search box stacked on top of the
/// palette contents, drawn over its own background.
struct Palette<'a> {
    children: Vec<Element<'a, Message>>,
    width: Length,
    height: Length,
    padding: Padding,
    spacing: f32,
}

impl<'a> Palette<'a> {
    fn new(search_query: &'a str) -> Self {
        Self {
            children: vec![
                text_input("Search", search_query)
                    .on_input(Message::InputChanged)
                    .id(SEARCH_BOX_ID)
                    .into(),
                text("Hi, I'm a command palette.").into(),
            ],
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
                ..Default::default()
            },
            Background::Color(theme.palette().background.weakest.color),
        );

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
