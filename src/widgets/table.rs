use std::collections::HashMap;
use std::time::Instant;

use iced::advanced::Shell;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::mouse;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{self, Paragraph, Text};
use iced::advanced::widget::{self, Tree, Widget};
use iced::alignment;
use iced::{Color, Element, Event, Font, Length, Pixels, Point, Size, keyboard, window};
use tracing::instrument;

const GUTTER_WIDTH: f32 = 48.0;
const SCROLLBAR_WIDTH: f32 = 5.0;
const MIN_SCROLLER: f32 = 24.0;
const SCROLL_LERP_FACTOR: f32 = 0.12;
const SCROLL_EPSILON: f32 = 0.01;

pub trait TableColumn {
    /// The header text of this column.
    fn name(&self) -> &str;
}

pub trait TableRow {
    fn cells(&self) -> &[String];
}

trait Rectangle {
    fn with_padding(self) -> Self;
}

impl Rectangle for iced::Rectangle {
    fn with_padding(self) -> Self {
        todo!()
    }
}

struct TableStyle {
    header_bg: Color,
    header_text: Color,
    even_row_bg: Color,
    odd_row_bg: Color,
    row_text: Color,
    gutter_color: Color,
    gutter_even_row_bg: Color,
    gutter_odd_row_bg: Color,
    separator_color: Color,
}

trait ColorExt {
    fn darken(self, factor: f32) -> Color;
}

impl ColorExt for Color {
    fn darken(self, amount: f32) -> Color {
        let factor = (1.0 - amount).max(0.0);
        Color {
            r: self.r * factor,
            g: self.g * factor,
            b: self.b * factor,
            a: self.a,
        }
    }
}

impl TableStyle {
    fn from_theme(theme: &iced::Theme) -> Self {
        let palette = theme.palette();

        Self {
            header_bg: palette.background.weak.color,
            header_text: palette.background.weak.text,
            even_row_bg: palette.background.weakest.color.darken(0.05),
            gutter_even_row_bg: palette.background.weak.color.darken(0.15),
            odd_row_bg: palette.background.weaker.color.darken(0.05),
            gutter_odd_row_bg: palette.background.weak.color,
            row_text: palette.background.weaker.text,
            gutter_color: palette.background.strong.color,
            separator_color: palette.background.strong.color,
        }
    }
}

pub struct Table<'a, Col, Row>
where
    Col: TableColumn,
    Row: TableRow,
{
    columns: &'a [Col],
    rows: &'a [Row],
    font: Font,
    text_size: f32,
    width: Length,
    height: Length,
    gutter_width: f32,
    padding_x: f32,
    padding_y: f32,
    min_col_width: f32,
    max_col_width: f32,
    separator_width: f32,
}

impl<'a, Col, Row> Table<'a, Col, Row>
where
    Col: TableColumn,
    Row: TableRow,
{
    pub fn new(columns: &'a [Col], rows: &'a [Row]) -> Self {
        Self {
            columns,
            rows,
            font: Font::MONOSPACE,
            text_size: 12.0,
            padding_x: 8.0,
            padding_y: 5.0,
            min_col_width: 80.0,
            max_col_width: 400.0,
            width: Length::Shrink,
            height: Length::Shrink,
            gutter_width: 48.0,
            separator_width: 1.0,
        }
    }

    fn layout<R>(&mut self, renderer: &R, limits: &Limits, state: &mut State<R::Paragraph>) -> Node
    where
        R: text::Renderer<Font = iced::Font>,
    {
        resize_vec(&mut state.header_paragraphs, self.columns.len());
        resize_vec(
            &mut state.body_paragraphs,
            self.rows.len() * self.columns.len(),
        );
        resize_vec(&mut state.gutter_paragraphs, self.rows.len() + 1);

        state.body_width = 0.0;

        let text_config = Text {
            content: "",
            bounds: limits.max(),
            size: Pixels(12.0),
            line_height: text::LineHeight::default(),
            font: Font::MONOSPACE,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Center,
            shaping: text::Shaping::default(),
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::End,
            hint_factor: renderer.hint_factor(),
        };

        for (i, col) in self.columns.iter().enumerate() {
            state.header_paragraphs[i].update(text_config.with_content(col.name()));
            state.body_width += state.header_paragraphs[i].min_width();
        }

        state.gutter_paragraphs[0].update(text_config.with_content("#"));
        state.header_height = state.gutter_paragraphs[0].min_height() + 2.0 * self.padding_y;

        state.text_height = state.gutter_paragraphs[0].min_height();
        state.body_cell_height = state.text_height + 2.0 * self.padding_y;

        let viewport_max_rows_count =
            (limits.max().height / state.body_cell_height).ceil() as usize;

        state.viewport_max_rows_count = viewport_max_rows_count;

        for i in (1..state.gutter_paragraphs.len())
            .skip(state.start_row_index)
            .take(viewport_max_rows_count)
        {
            state.gutter_paragraphs[i].update(text_config.with_content(&i.to_string()));
        }

        for (i, cell) in self
            .rows
            .iter()
            .flat_map(|row| row.cells())
            .enumerate()
            .skip(state.start_row_index * self.columns.len())
            .take(viewport_max_rows_count * self.columns.len())
        {
            let text_config = Text {
                content: cell.as_str(),
                bounds: Size {
                    width: state.header_paragraphs[i % state.header_paragraphs.len()].min_width(),
                    ..limits.max()
                },
                size: Pixels(12.0),
                line_height: text::LineHeight::default(),
                font: Font::MONOSPACE,
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::default(),
                wrapping: text::Wrapping::None,
                ellipsis: text::Ellipsis::End,
                hint_factor: renderer.hint_factor(),
            };
            state.body_paragraphs[i].update(text_config);
        }

        Node::new(limits.max())
    }

    fn fill_gutter_paragraphs<R>(
        &self,
        gutter_bounds: iced::Rectangle,
        state: &State<R::Paragraph>,
        style: &TableStyle,
        renderer: &mut R,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        let cell = &state.gutter_paragraphs[0];
        renderer.fill_paragraph(
            cell.raw(),
            Point {
                x: gutter_bounds.x + self.padding_x,
                y: gutter_bounds.y + self.padding_y,
            },
            style.row_text,
            iced::Rectangle {
                x: gutter_bounds.x + self.padding_x,
                y: gutter_bounds.y + self.padding_y,
                width: cell.min_bounds().width + 2.0 * self.padding_x,
                height: cell.min_bounds().height + 2.0 * self.padding_y,
            },
        );
        let body_bounds = iced::Rectangle {
            y: gutter_bounds.y + state.header_height,
            height: gutter_bounds.height - state.header_height,
            ..gutter_bounds
        };

        renderer.with_layer(body_bounds, |renderer| {
            for (i, cell) in state
                .gutter_paragraphs
                .iter()
                .enumerate()
                .skip(state.start_row_index + 1)
                .take(state.viewport_max_rows_count)
            {
                renderer.fill_paragraph(
                    cell.raw(),
                    Point {
                        x: gutter_bounds.x + self.padding_x,
                        y: gutter_bounds.y
                            + i as f32 * (cell.min_height() + 2.0 * self.padding_y)
                            + self.padding_y
                            - state.scroll_offset.y,
                    },
                    style.row_text,
                    iced::Rectangle {
                        x: gutter_bounds.x + self.padding_x,
                        y: gutter_bounds.y
                            + i as f32 * (cell.min_height() + 2.0 * self.padding_y)
                            + self.padding_y
                            - state.scroll_offset.y,
                        width: cell.min_bounds().width + 2.0 * self.padding_x,
                        height: cell.min_bounds().height + 2.0 * self.padding_y,
                    },
                );
            }
        });
    }

    fn fill_gutter_quads<R>(
        &self,
        gutter_bounds: iced::Rectangle,
        state: &State<R::Paragraph>,
        style: &TableStyle,
        renderer: &mut R,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        renderer.fill_quad(
            renderer::Quad {
                bounds: iced::Rectangle {
                    height: state.header_height,
                    ..gutter_bounds
                },
                ..Default::default()
            },
            style.gutter_color,
        );
        let body_bounds = iced::Rectangle {
            y: gutter_bounds.y + state.header_height,
            height: gutter_bounds.height - state.header_height,
            ..gutter_bounds
        };
        renderer.with_layer(body_bounds, |renderer| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: body_bounds,
                    ..Default::default()
                },
                style.gutter_odd_row_bg,
            );
            for i in (0..state.gutter_paragraphs.len())
                .skip(state.start_row_index)
                .take(state.viewport_max_rows_count)
                .filter(|i| (i >> 0) & 1 == 1)
            {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: iced::Rectangle {
                            x: body_bounds.x,
                            y: body_bounds.y + i as f32 * state.body_cell_height
                                - state.scroll_offset.y,
                            height: state.body_cell_height,
                            ..body_bounds
                        },
                        ..Default::default()
                    },
                    style.gutter_even_row_bg,
                );
            }
        });
    }

    fn draw_gutter<R>(
        &self,
        bounds: iced::Rectangle,
        state: &State<R::Paragraph>,
        style: &TableStyle,
        renderer: &mut R,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        let gutter_bounds = iced::Rectangle {
            width: GUTTER_WIDTH,
            ..bounds
        };
        renderer.with_layer(gutter_bounds, |renderer| {
            self.fill_gutter_quads(gutter_bounds, state, &style, renderer);
            self.fill_gutter_paragraphs(gutter_bounds, state, &style, renderer);
        });
    }

    fn draw_header<R>(
        &self,
        layout_bounds: iced::Rectangle,
        state: &State<R::Paragraph>,
        style: &TableStyle,
        renderer: &mut R,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        let header_bounds = iced::Rectangle {
            x: GUTTER_WIDTH,
            height: state.header_height,
            width: layout_bounds.width - GUTTER_WIDTH,
            ..layout_bounds
        };
        renderer.fill_quad(
            Quad {
                bounds: header_bounds,
                ..Default::default()
            },
            style.header_bg,
        );
        renderer.with_layer(header_bounds, |renderer| {
            let mut running_width_sum = 0.0;
            for (i, col) in state.header_paragraphs.iter().enumerate() {
                renderer.fill_paragraph(
                    col.raw(),
                    Point {
                        x: header_bounds.x
                            + (running_width_sum + i as f32 * 2.0 * self.padding_x)
                            + self.padding_x
                            - state.scroll_offset.x,
                        y: header_bounds.y + self.padding_y,
                    },
                    style.header_text,
                    iced::Rectangle {
                        x: header_bounds.x
                            + (running_width_sum + i as f32 * 2.0 * self.padding_x)
                            + self.padding_x
                            - state.scroll_offset.x,
                        y: header_bounds.y + self.padding_y,
                        width: col.min_width() + 2.0 * self.padding_x,
                        height: state.header_height,
                    },
                );
                running_width_sum += col.min_width();
            }
        });
    }

    fn draw_body<R>(
        &self,
        layout_bounds: iced::Rectangle,
        state: &State<R::Paragraph>,
        style: &TableStyle,
        renderer: &mut R,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        let body_bounds = iced::Rectangle {
            x: GUTTER_WIDTH,
            y: layout_bounds.y + state.header_height,
            width: layout_bounds.width - GUTTER_WIDTH,
            height: layout_bounds.height - state.header_height,
        };

        renderer.with_layer(body_bounds, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: body_bounds,
                    ..Default::default()
                },
                style.odd_row_bg,
            );
            for i in (0..self.rows.len())
                .skip(state.start_row_index)
                .take(state.viewport_max_rows_count)
                .filter(|i| (i >> 0) & 1 == 1)
            {
                renderer.fill_quad(
                    Quad {
                        bounds: iced::Rectangle {
                            y: body_bounds.y + i as f32 * state.body_cell_height
                                - state.scroll_offset.y,
                            height: state.body_cell_height,
                            ..body_bounds
                        },
                        ..Default::default()
                    },
                    style.even_row_bg,
                );
            }
            let mut running_width_sum = 0.0;
            for (i, cell) in state
                .body_paragraphs
                .iter()
                .enumerate()
                .skip(state.start_row_index * self.columns.len())
                .take(state.viewport_max_rows_count * self.columns.len())
            {
                let cell_width =
                    state.header_paragraphs[i % state.header_paragraphs.len()].min_width();
                let line_index = (i / self.columns.len()) as f32;
                let x = body_bounds.x
                    + (running_width_sum + (i % self.columns.len()) as f32 * 2.0 * self.padding_x)
                    + self.padding_x
                    - state.scroll_offset.x;
                let y = body_bounds.y + line_index * state.body_cell_height + self.padding_y
                    - state.scroll_offset.y;
                renderer.fill_paragraph(
                    cell.raw(),
                    Point { x, y },
                    style.row_text,
                    iced::Rectangle {
                        x,
                        y,
                        width: cell_width + 2.0 * self.padding_x,
                        height: state.header_height,
                    },
                );
                running_width_sum = (running_width_sum + cell_width) % state.body_width;
            }
        });
    }

    fn draw_separators<R>(
        &self,
        bounds: iced::Rectangle,
        state: &State<R::Paragraph>,
        style: &TableStyle,
        renderer: &mut R,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        renderer.fill_quad(
            renderer::Quad {
                bounds: iced::Rectangle {
                    x: bounds.x,
                    y: bounds.y + state.header_height,
                    width: bounds.width,
                    height: 1.0,
                },
                ..Default::default()
            },
            style.separator_color,
        );

        renderer.fill_quad(
            renderer::Quad {
                bounds: iced::Rectangle {
                    x: bounds.x + GUTTER_WIDTH - 1.0,
                    y: bounds.y,
                    width: 1.0,
                    height: bounds.height,
                },
                ..Default::default()
            },
            style.separator_color,
        );

        let scrollable_bounds = iced::Rectangle {
            x: bounds.x,
            y: bounds.y + state.header_height,
            width: bounds.width,
            height: bounds.height - state.header_height,
        };

        let body_bounds = iced::Rectangle {
            x: bounds.x + GUTTER_WIDTH,
            y: bounds.y + state.header_height,
            width: bounds.width - GUTTER_WIDTH,
            height: bounds.height - state.header_height,
        };

        let header_bounds = iced::Rectangle {
            x: bounds.x + GUTTER_WIDTH,
            y: bounds.y,
            width: bounds.width - GUTTER_WIDTH,
            height: state.header_height,
        };

        renderer.with_layer(scrollable_bounds, |renderer| {
            for i in (0..self.rows.len())
                .skip(state.start_row_index)
                .take(state.viewport_max_rows_count + 1)
            {
                let y = scrollable_bounds.y + (i + 1) as f32 * state.body_cell_height
                    - state.scroll_offset.y;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: iced::Rectangle {
                            x: scrollable_bounds.x,
                            y,
                            width: scrollable_bounds.width,
                            height: 1.0,
                        },
                        ..Default::default()
                    },
                    style.separator_color,
                );
            }
        });

        let mut running_width_sum = 0.0;
        for (i, col) in state.header_paragraphs.iter().enumerate() {
            running_width_sum += col.min_width();
            let boundary_x = running_width_sum + (i + 1) as f32 * 2.0 * self.padding_x;

            // Draw body column separator (moves with horizontal scroll)
            let body_x = body_bounds.x + boundary_x - state.scroll_offset.x;
            renderer.with_layer(body_bounds, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: iced::Rectangle {
                            x: body_x,
                            y: body_bounds.y,
                            width: 1.0,
                            height: body_bounds.height,
                        },
                        ..Default::default()
                    },
                    style.separator_color,
                );
            });

            let header_x = header_bounds.x + boundary_x;
            renderer.with_layer(header_bounds, |renderer| {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: iced::Rectangle {
                            x: header_x - state.scroll_offset.x,
                            y: header_bounds.y,
                            width: 1.0,
                            height: header_bounds.height,
                        },
                        ..Default::default()
                    },
                    style.separator_color,
                );
            });
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    Vertical(f32),
    Horizontal(f32),
}

struct State<P: Paragraph> {
    header_paragraphs: Vec<Plain<P>>,
    body_paragraphs: Vec<Plain<P>>,
    gutter_paragraphs: Vec<Plain<P>>,
    previous_limits: Option<Limits>,
    horizontal_scroll_position: ScrollPosition,
    vertical_scroll_position: ScrollPosition,
    keyboard_modifiers: keyboard::Modifiers,
    dragging: Option<Drag>,
    text_height: f32,
    body_cell_height: f32,
    header_height: f32,
    body_width: f32,
    scroll_offset: Point<f32>,
    scroll_target: Point<f32>,
    last_frame: Option<Instant>,
    start_row_index: usize,
    viewport_max_rows_count: usize,
}

#[derive(Debug, Default)]
struct ScrollPosition {
    offset: f32,
    target: f32,
}

impl<P: Paragraph> Default for State<P> {
    fn default() -> Self {
        Self {
            horizontal_scroll_position: ScrollPosition::default(),
            vertical_scroll_position: ScrollPosition::default(),
            keyboard_modifiers: keyboard::Modifiers::default(),
            dragging: None,
            previous_limits: None,
            header_paragraphs: Vec::new(),
            body_paragraphs: Vec::new(),
            gutter_paragraphs: Vec::new(),
            text_height: 0.0,
            body_cell_height: 0.0,
            header_height: 0.0,
            body_width: 0.0,
            scroll_offset: Point { x: 0.0, y: 0.0 },
            scroll_target: Point { x: 0.0, y: 0.0 },
            last_frame: None,
            start_row_index: 0,
            viewport_max_rows_count: 0,
        }
    }
}

#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

impl<'a, Message, R, Col, Row> Widget<Message, iced::Theme, R> for Table<'a, Col, Row>
where
    R: text::Renderer<Font = iced::Font>,
    Col: TableColumn,
    Row: TableRow,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State<R::Paragraph>>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::<R::Paragraph>::default())
    }

    #[instrument(skip_all)]
    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &Limits) -> Node {
        let state = tree.state.downcast_mut::<State<R::Paragraph>>();
        let start = Instant::now();
        let node = self.layout(renderer, limits, state);
        tracing::info!("took {:?}", start.elapsed());
        node
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let style = TableStyle::from_theme(theme);
        renderer.with_layer(layout.bounds(), |renderer| {
            self.draw_gutter(layout.bounds(), state, &style, renderer);
            self.draw_header(layout.bounds(), state, &style, renderer);
            self.draw_body(layout.bounds(), state, &style, renderer);
            self.draw_separators(layout.bounds(), state, &style, renderer)
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &R,
        shell: &mut Shell<'_, Message>,
        _viewport: &iced::Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<R::Paragraph>>();
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if shell.is_event_captured() || !cursor.is_over(bounds) {
                    return;
                }

                let (dx, dy) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => {
                        let (x, y) = if state.keyboard_modifiers.shift() {
                            (y, x)
                        } else {
                            (x, y)
                        };

                        (-x * 40.0, -y * state.body_cell_height)
                    }
                    mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
                };

                state.scroll_target = Point {
                    x: state.scroll_target.x + dx,
                    y: state.scroll_target.y + dy,
                };

                state.start_row_index =
                    (state.scroll_offset.y / state.body_cell_height).floor() as usize;

                shell.request_redraw();
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
            }
            Event::Window(window::Event::RedrawRequested(instant)) => {
                if state.scroll_offset != state.scroll_target {
                    let elapsed = state
                        .last_frame
                        .map(|prev| instant.duration_since(prev).as_secs_f32())
                        .unwrap_or(0.016)
                        .min(0.1);

                    let decay = (-SCROLL_LERP_FACTOR * elapsed * 60.0).exp();

                    let new_x = state.scroll_target.x
                        + (state.scroll_offset.x - state.scroll_target.x) * decay;
                    let new_y = state.scroll_target.y
                        + (state.scroll_offset.y - state.scroll_target.y) * decay;

                    let reached_x = (new_x - state.scroll_target.x).abs() < SCROLL_EPSILON;
                    let reached_y = (new_y - state.scroll_target.y).abs() < SCROLL_EPSILON;

                    state.scroll_offset.x = if reached_x {
                        state.scroll_target.x
                    } else {
                        new_x
                    };
                    state.scroll_offset.y = if reached_y {
                        state.scroll_target.y
                    } else {
                        new_y
                    };

                    let old_start = state.start_row_index;
                    state.start_row_index =
                        (state.scroll_offset.y / state.body_cell_height).floor() as usize;

                    if old_start != state.start_row_index {
                        shell.invalidate_layout();
                    }

                    if !reached_x || !reached_y {
                        shell.request_redraw();
                        state.last_frame = Some(*instant);
                    } else {
                        state.last_frame = None;
                    }
                } else {
                    state.last_frame = None;
                }
            }
            _ => (),
        }
    }
}

impl<'a, Message, R, Col, Row> From<Table<'a, Col, Row>> for Element<'a, Message, iced::Theme, R>
where
    R: text::Renderer<Font = iced::Font> + 'a,
    Col: TableColumn + 'a,
    Row: TableRow + 'a,
{
    fn from(table: Table<'a, Col, Row>) -> Self {
        Element::new(table)
    }
}

fn resize_vec<T: Default>(vec: &mut Vec<T>, len: usize) {
    if vec.len() < len {
        vec.resize_with(len, T::default);
    } else {
        vec.truncate(len);
    }
}
