use iced::advanced::layout::{self, Layout, Limits};
use iced::advanced::renderer::Renderer as _;
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Shell, mouse, overlay, renderer};
use iced::widget::{Column, Id, button, container, rule, text, text_input};
use iced::{
    Background, Color, Element, Event, Length, Padding, Point, Rectangle, Size, Theme, Vector,
    keyboard,
};

use crate::widgets::scrollable::{Scrollable, State as ScrollableState};

const PALETTE_WIDTH: f32 = 500.0;
const PALETTE_HEIGHT: f32 = 300.0;

/// Height of a single command row.
const ROW_HEIGHT: f32 = 24.0;
/// Vertical spacing between command rows.
const ROW_SPACING: f32 = 2.0;
/// Vertical padding applied to the command list column.
const LIST_PADDING: f32 = 4.0;
/// Top/bottom padding around the whole command list.
const LIST_VERTICAL_PADDING: f32 = 4.0;
/// Distance between the top of one row and the top of the next.
const ROW_STRIDE: f32 = ROW_HEIGHT + ROW_SPACING;
/// Text size used for command row labels.
const ROW_TEXT_SIZE: f32 = 12.0;
/// Index of the scrollable list child.
const SCROLLABLE_CHILD: usize = 2;

/// Messages emitted by [`Palette`]. The widget owns its own selection and
/// scrolling, so it only needs to tell its parent about the query changing and
/// the item that was activated.
#[derive(Debug, Clone)]
pub enum PaletteMessage {
    /// The search query changed.
    InputChanged(String),
    /// The item at `index` was activated (Enter key or mouse click).
    Activated(usize),
}

/// A reusable, self-contained command palette: a search input over a scrollable
/// column of buttons. It manages keyboard navigation, selection and scroll
/// entirely on its own and only emits [`PaletteMessage`] to its parent.
pub struct Palette<'a> {
    children: Vec<Element<'a, PaletteMessage>>,
    labels: Vec<&'a str>,
    input_value: String,
    width: Length,
    height: Length,
}

#[derive(Debug, Default)]
struct State {
    selected_index: usize,
    last_query: String,
}

impl<'a> Palette<'a> {
    pub fn new(query: &'a str, labels: Vec<&'a str>) -> Self {
        let buttons: Vec<Element<'a, PaletteMessage>> = labels
            .iter()
            .copied()
            .enumerate()
            .map(|(index, label)| {
                button(text(label).size(ROW_TEXT_SIZE))
                    .on_press(PaletteMessage::Activated(index))
                    .width(Length::Fill)
                    .height(Length::Fixed(ROW_HEIGHT))
                    .padding([4, 4])
                    .style(|theme: &Theme, status| button::Style {
                        background: None,
                        text_color: theme.palette().background.weak.text,
                        ..button::primary(theme, status)
                    })
                    .into()
            })
            .collect();

        let column = Column::from_vec(buttons)
            .spacing(ROW_SPACING)
            .padding(Padding::new(LIST_VERTICAL_PADDING).horizontal(LIST_PADDING))
            .width(Length::Fill);

        let children = vec![
            Self::build_input(query, Id::new("palette_input")),
            rule::horizontal(1.0).into(),
            Scrollable::new(column).line_height(ROW_STRIDE).into(),
        ];

        Self {
            children,
            labels,
            input_value: query.to_string(),
            width: Length::Fixed(PALETTE_WIDTH),
            height: Length::Fixed(PALETTE_HEIGHT),
        }
    }

    fn build_input(value: &str, id: Id) -> Element<'a, PaletteMessage> {
        container(
            text_input("Search", value)
                .id(id)
                .on_input(PaletteMessage::InputChanged)
                .style(|theme, status| text_input::Style {
                    border: iced::Border::default().color(Color::TRANSPARENT),
                    background: iced::Background::Color(Color::TRANSPARENT).into(),
                    ..text_input::default(theme, status)
                })
                .size(12)
                .padding([8, 4]),
        )
        .width(Length::Fill)
        .padding([0, 4])
        .into()
    }

    pub fn input_id(mut self, id: Id) -> Self {
        self.children[0] = Self::build_input(&self.input_value, id);
        self
    }

    fn content_height(&self) -> f32 {
        2.0 * LIST_VERTICAL_PADDING
            + self.labels.len() as f32 * ROW_HEIGHT
            + self.labels.len().saturating_sub(1) as f32 * ROW_SPACING
    }

    fn scroll_target(&self, index: usize, current: f32, viewport_height: f32) -> f32 {
        let row_top = LIST_VERTICAL_PADDING + index as f32 * ROW_STRIDE;
        let row_bottom = row_top + ROW_HEIGHT;

        let viewport_top = current;
        let viewport_bottom = current + viewport_height;

        if viewport_height <= 0.0 {
            row_top
        } else if row_top < viewport_top {
            row_top
        } else if row_bottom > viewport_bottom {
            (row_bottom - viewport_height).max(0.0)
        } else {
            current
        }
    }

    fn row_at(&self, y: f32, list_top: f32, viewport_height: f32, offset: f32) -> Option<usize> {
        if y < list_top || y >= list_top + viewport_height {
            return None;
        }

        let content_y = y - list_top - LIST_VERTICAL_PADDING + offset;
        let index = (content_y / ROW_STRIDE).floor() as isize;

        if index >= 0 && (index as usize) < self.labels.len() {
            Some(index as usize)
        } else {
            None
        }
    }
}

impl Widget<PaletteMessage, Theme, iced::Renderer> for Palette<'_> {
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
        let state = tree.state.downcast_mut::<State>();
        if state.last_query != self.input_value {
            state.last_query = self.input_value.clone();
            state.selected_index = 0;
            if let Some(child) = tree.children.get_mut(SCROLLABLE_CHILD) {
                child.state.downcast_mut::<ScrollableState>().offset = 0.0;
            }
        }

        tree.diff_children(&mut self.children);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.resolve(self.width, self.height, Size::ZERO);

        let full_limits = Limits::new(Size::ZERO, size);
        let input_node =
            self.children[0]
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, &full_limits);
        let rule_node =
            self.children[1]
                .as_widget_mut()
                .layout(&mut tree.children[1], renderer, &full_limits);

        let input_height = input_node.size().height;
        let rule_height = rule_node.size().height;
        let list_height = (size.height - input_height - rule_height).max(0.0);

        let list_limits = Limits::new(Size::ZERO, Size::new(size.width, list_height));
        let scrollable_node = self.children[SCROLLABLE_CHILD].as_widget_mut().layout(
            &mut tree.children[SCROLLABLE_CHILD],
            renderer,
            &list_limits,
        );

        layout::Node::with_children(
            size,
            vec![
                input_node.move_to(Point::new(0.0, 0.0)),
                rule_node.move_to(Point::new(0.0, input_height)),
                scrollable_node.move_to(Point::new(0.0, input_height + rule_height)),
            ],
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
        shell: &mut Shell<'_, PaletteMessage>,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let mut children = layout.children();
        let input_height = children.next().map(|c| c.bounds().height).unwrap_or(0.0);
        let rule_height = children.next().map(|c| c.bounds().height).unwrap_or(0.0);
        let header_height = input_height + rule_height;
        let list_top = bounds.y + header_height;
        let viewport_height = (bounds.height - header_height).max(0.0);

        if let Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) = event {
            match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    let count = self.labels.len();
                    if count > 0 {
                        let index = tree
                            .state
                            .downcast_ref::<State>()
                            .selected_index
                            .min(count - 1);
                        shell.publish(PaletteMessage::Activated(index));
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                    let count = self.labels.len();
                    if count > 0 {
                        let state = tree.state.downcast_mut::<State>();
                        state.selected_index = (state.selected_index + 1).min(count - 1);
                        let index = state.selected_index;

                        let scrollable = tree.children[SCROLLABLE_CHILD]
                            .state
                            .downcast_mut::<ScrollableState>();
                        let target = self.scroll_target(index, scrollable.offset, viewport_height);
                        let max_scroll = (self.content_height() - viewport_height).max(0.0);
                        scrollable.offset = target.clamp(0.0, max_scroll);

                        shell.request_redraw();
                    }
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                    let state = tree.state.downcast_mut::<State>();
                    if state.selected_index > 0 {
                        state.selected_index -= 1;
                        let index = state.selected_index;

                        let scrollable = tree.children[SCROLLABLE_CHILD]
                            .state
                            .downcast_mut::<ScrollableState>();
                        let target = self.scroll_target(index, scrollable.offset, viewport_height);
                        let max_scroll = (self.content_height() - viewport_height).max(0.0);
                        scrollable.offset = target.clamp(0.0, max_scroll);

                        shell.request_redraw();
                    }
                }
                _ => {}
            }
        }

        if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
            if let Some(position) = cursor.position() {
                let offset = tree.children[SCROLLABLE_CHILD]
                    .state
                    .downcast_ref::<ScrollableState>()
                    .offset;

                if let Some(index) = self.row_at(position.y, list_top, viewport_height, offset) {
                    let state = tree.state.downcast_mut::<State>();
                    if index != state.selected_index {
                        state.selected_index = index;
                        shell.request_redraw();
                    }
                }
            }
        }

        for ((child, child_state), child_layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                child_state,
                event,
                child_layout,
                cursor,
                renderer,
                shell,
                viewport,
            );
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
        if bounds.intersection(viewport).is_none() {
            return;
        }

        let state = tree.state.downcast_ref::<State>();

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

        let mut children = layout.children();
        let input_layout = children.next().unwrap();
        let rule_layout = children.next().unwrap();
        let scrollable_layout = children.next().unwrap();

        self.children[0].as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            input_layout,
            cursor,
            viewport,
        );
        self.children[1].as_widget().draw(
            &tree.children[1],
            renderer,
            theme,
            style,
            rule_layout,
            cursor,
            viewport,
        );

        let list_area = scrollable_layout.bounds();
        let offset = tree.children[SCROLLABLE_CHILD]
            .state
            .downcast_ref::<ScrollableState>()
            .offset;

        renderer.with_layer(list_area, |renderer| {
            let selected = state
                .selected_index
                .min(self.labels.len().saturating_sub(1));

            if !self.labels.is_empty() {
                let row_rect = Rectangle {
                    x: list_area.x + LIST_PADDING,
                    y: list_area.y + LIST_VERTICAL_PADDING + selected as f32 * ROW_STRIDE - offset,
                    width: list_area.width - 2.0 * LIST_PADDING,
                    height: ROW_HEIGHT,
                };

                if row_rect.intersects(&list_area) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: row_rect,
                            border: iced::Border {
                                radius: iced::border::radius(2.0),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        Background::Color(theme.palette().background.stronger.color),
                    );
                }
            }
        });

        self.children[SCROLLABLE_CHILD].as_widget().draw(
            &tree.children[SCROLLABLE_CHILD],
            renderer,
            theme,
            style,
            scrollable_layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut Tree,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, PaletteMessage, Theme, iced::Renderer>> {
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

impl<'a> From<Palette<'a>> for Element<'a, PaletteMessage> {
    fn from(palette: Palette<'a>) -> Self {
        Self::new(palette)
    }
}
