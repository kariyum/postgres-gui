use iced::advanced::layout::{self, Layout, Limits};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Theme, Vector};

/// A minimal, vertical-only scrollable container.
///
/// Unlike `iced::widget::scrollable`, this widget keeps its scroll offset in its
/// own [`State`], so a parent widget can read or set it synchronously (no
/// `operation::scroll_to` [`Task`] required). This is what allows the palette to
/// scroll its selected row into view from inside its own `update`.
pub struct Scrollable<'a, Message> {
    content: Element<'a, Message>,
    line_height: f32,
    width: Length,
    height: Length,
}

/// The scroll state of a [`Scrollable`]. `offset` is the number of pixels the
/// content has been scrolled down (0.0 means the top is visible).
#[derive(Debug, Default)]
pub struct State {
    pub offset: f32,
}

impl<'a, Message> Scrollable<'a, Message> {
    pub fn new(content: impl Into<Element<'a, Message>>) -> Self {
        Self {
            content: content.into(),
            line_height: 60.0,
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    /// Sets the number of pixels scrolled per "line" of wheel input.
    pub fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Scrollable<'_, Message>
where
    Message: Clone,
{
    fn tag(&self) -> iced::advanced::widget::tree::Tag {
        iced::advanced::widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> iced::advanced::widget::tree::State {
        iced::advanced::widget::tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.width, self.height, Size::ZERO);

        let content_limits = Limits::new(Size::ZERO, Size::new(size.width, f32::INFINITY));
        let content_node = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &content_limits,
        );

        let state = tree.state.downcast_mut::<State>();
        let max_scroll = (content_node.size().height - size.height).max(0.0);
        state.offset = state.offset.clamp(0.0, max_scroll);

        layout::Node::with_children(size, vec![content_node])
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
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout.children().next().unwrap(),
                renderer,
                operation,
            );
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
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let content_layout = layout.children().next().unwrap();
        let content_bounds = content_layout.bounds();
        let max_scroll = (content_bounds.height - bounds.height).max(0.0);

        if let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event {
            if cursor.is_over(bounds) {
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y * self.line_height,
                    mouse::ScrollDelta::Pixels { y, .. } => -y,
                };

                let state = tree.state.downcast_mut::<State>();
                state.offset = (state.offset + dy).clamp(0.0, max_scroll);
                shell.request_redraw();
                shell.capture_event();
                return;
            }
        }

        let offset = tree.state.downcast_ref::<State>().offset;

        let cursor = match cursor.position() {
            Some(position) => mouse::Cursor::Available(position + Vector::new(0.0, offset)),
            None => mouse::Cursor::Unavailable,
        };

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            content_layout,
            cursor,
            renderer,
            shell,
            &Rectangle {
                y: bounds.y + offset,
                ..bounds
            },
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let offset = tree.state.downcast_ref::<State>().offset;
        let cursor = match cursor.position() {
            Some(position) => mouse::Cursor::Available(position + Vector::new(0.0, offset)),
            None => mouse::Cursor::Unavailable,
        };

        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        )
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
        let Some(visible_bounds) = bounds.intersection(viewport) else {
            return;
        };

        let offset = tree.state.downcast_ref::<State>().offset;

        renderer.with_layer(visible_bounds, |renderer| {
            renderer.with_translation(Vector::new(0.0, -offset), |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.children().next().unwrap(),
                    cursor,
                    &Rectangle {
                        y: visible_bounds.y + offset,
                        ..visible_bounds
                    },
                );
            });
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
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message> From<Scrollable<'a, Message>> for Element<'a, Message>
where
    Message: Clone + 'a,
{
    fn from(scrollable: Scrollable<'a, Message>) -> Self {
        Self::new(scrollable)
    }
}
