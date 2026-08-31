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
        }

        if state.column_widths.len() != self.columns.len() {
            state.column_widths = state
                .header_paragraphs
                .iter()
                .map(|header_cell| header_cell.min_width())
                .collect();
        }

        state.body_width = state.column_widths.iter().sum();

        state.gutter_paragraphs[0].update(text_config.with_content("#"));
        state.header_height = state.gutter_paragraphs[0].min_height() + 2.0 * self.padding_y;

        state.text_height = state.gutter_paragraphs[0].min_height();
        state.body_cell_height = state.text_height + 2.0 * self.padding_y;

        let viewport = limits.max();
        let total_padding = self.columns.len() as f32 * 2.0 * self.padding_x;
        state.max_scroll_offset = Point {
            x: (state.body_width + total_padding - (viewport.width - GUTTER_WIDTH)).max(0.0),
            y: if state.body_cell_height > 0.0 {
                (self.rows.len() as f32 * state.body_cell_height
                    - (viewport.height - state.header_height))
                    .max(0.0)
            } else {
                0.0
            },
        };

        state.scroll_target = Point {
            x: state.scroll_target.x.clamp(0.0, state.max_scroll_offset.x),
            y: state.scroll_target.y.clamp(0.0, state.max_scroll_offset.y),
        };
        state.scroll_offset = Point {
            x: state.scroll_offset.x.clamp(0.0, state.max_scroll_offset.x),
            y: state.scroll_offset.y.clamp(0.0, state.max_scroll_offset.y),
        };

        state.start_row_index = (state.scroll_offset.y / state.body_cell_height).floor() as usize;

        let viewport_max_rows_count = (viewport.height / state.body_cell_height).ceil() as usize;

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
                    width: state.column_widths[i % state.column_widths.len()],
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
            for i in (0..self.rows.len())
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
            for (i, (col, col_width)) in state
                .header_paragraphs
                .iter()
                .zip(&state.column_widths)
                .enumerate()
            {
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
                        width: col_width + 2.0 * self.padding_x,
                        height: state.header_height,
                    },
                );
                running_width_sum += col_width;
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
                let col_idx = i % self.columns.len();
                let cell_width = state.column_widths[col_idx];
                let line_index = (i / self.columns.len()) as f32;
                let x = body_bounds.x
                    + (running_width_sum + col_idx as f32 * 2.0 * self.padding_x)
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
        for (i, _col) in state.header_paragraphs.iter().enumerate() {
            running_width_sum += state.column_widths[i];
            let boundary_x = running_width_sum + (i + 1) as f32 * 2.0 * self.padding_x;

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

    fn handle_double_click<P: Paragraph>(
        &mut self,
        state: &mut State<P>,
        position: Point,
    ) -> Option<String> {
        let now = Instant::now();
        let is_double_click = if let Some((last_pos, last_time)) = state.last_click {
            last_pos.distance(position) < 5.0 && (now - last_time).as_millis() <= 300
        } else {
            false
        };
        state.last_click = Some((position, now));
        if is_double_click {
            let clicked_row_f =
                (position.y - state.header_height + state.scroll_offset.y) / state.body_cell_height;
            let clicked_row = clicked_row_f.floor() as usize;
            let click_x_rel = position.x - GUTTER_WIDTH + state.scroll_offset.x;
            if position.x >= GUTTER_WIDTH
                && position.y >= state.header_height
                && clicked_row_f >= 0.0
                && clicked_row < self.rows.len()
            {
                let mut running_width_sum = 0.0;
                for col_idx in 0..self.columns.len() {
                    let col_width = state.column_widths[col_idx];
                    let col_start = running_width_sum + col_idx as f32 * 2.0 * self.padding_x;
                    let col_end = col_start + col_width + 2.0 * self.padding_x;
                    if click_x_rel >= col_start && click_x_rel < col_end {
                        let cells = self.rows[clicked_row].cells();
                        if col_idx < cells.len() {
                            return Some(cells[col_idx].clone());
                        }
                        break;
                    }
                    running_width_sum += col_width;
                }
            }
        }
        None
    }

    fn handle_dragging<P: Paragraph>(
        &mut self,
        state: &mut State<P>,
        bounds: iced::Rectangle,
        position: Point,
    ) -> Option<Drag> {
        let mut running_width_sum = 0.0;
        for i in 0..self.columns.len() {
            let col_width = state.column_widths[i];
            running_width_sum += col_width;
            let boundary_x = running_width_sum + (i + 1) as f32 * 2.0 * self.padding_x;
            let separator_x = bounds.x + GUTTER_WIDTH + boundary_x - state.scroll_offset.x;

            if (position.x - separator_x).abs() <= 5.0 {
                return Some(Drag::ColumnResize(ColumnResize {
                    left_col_index: i,
                    start_x: position.x,
                    left_start_width: col_width,
                    right_start_width: state.column_widths
                        [(i + 1).min(state.column_widths.len() - 1)],
                }));
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
struct ColumnResize {
    left_col_index: usize,
    start_x: f32,
    left_start_width: f32,
    right_start_width: f32,
}

#[derive(Debug, Clone, Copy)]
enum Drag {
    ColumnResize(ColumnResize),
}

struct State<P: Paragraph> {
    header_paragraphs: Vec<Plain<P>>,
    body_paragraphs: Vec<Plain<P>>,
    gutter_paragraphs: Vec<Plain<P>>,
    column_widths: Vec<f32>,
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
    max_scroll_offset: Point<f32>,
    last_frame: Option<Instant>,
    last_click: Option<(Point, Instant)>,
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
            column_widths: Vec::new(),
            text_height: 0.0,
            body_cell_height: 0.0,
            header_height: 0.0,
            body_width: 0.0,
            scroll_offset: Point { x: 0.0, y: 0.0 },
            scroll_target: Point { x: 0.0, y: 0.0 },
            max_scroll_offset: Point { x: 0.0, y: 0.0 },
            last_frame: None,
            last_click: None,
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
        // tracing::info!("took {:?}", start.elapsed());
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

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &iced::Rectangle,
        _renderer: &R,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let bounds = layout.bounds();

        if let Some(position) = cursor.position_in(bounds) {
            if let Some(Drag::ColumnResize(ColumnResize { .. })) = state.dragging {
                return mouse::Interaction::ResizingHorizontally;
            }

            let mut running_width_sum = 0.0;
            for i in 0..self.columns.len() {
                let col_width = if i < state.column_widths.len() {
                    state.column_widths[i]
                } else {
                    return mouse::Interaction::None;
                };
                running_width_sum += col_width;
                let boundary_x = running_width_sum + (i + 1) as f32 * 2.0 * self.padding_x;
                let separator_x = bounds.x + GUTTER_WIDTH + boundary_x - state.scroll_offset.x;

                if (position.x - separator_x).abs() <= 5.0 {
                    return mouse::Interaction::ResizingHorizontally;
                }
            }
        }

        mouse::Interaction::None
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
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if let Some(text) = self.handle_double_click(state, position) {
                        shell.write_clipboard(iced::advanced::clipboard::Content::Text(text));
                        shell.capture_event();
                        return;
                    }

                    state.dragging = self.handle_dragging(state, bounds, position);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(Drag::ColumnResize(ColumnResize { .. })) = state.dragging {
                    state.dragging = None;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(Drag::ColumnResize(column_resize)) = state.dragging {
                    updated_width(state, position, column_resize);

                    state.body_width = state.column_widths.iter().sum();
                    let total_padding = self.columns.len() as f32 * 2.0 * self.padding_x;
                    state.max_scroll_offset.x =
                        (state.body_width + total_padding - (bounds.width - GUTTER_WIDTH)).max(0.0);
                    state.scroll_target.x =
                        state.scroll_target.x.clamp(0.0, state.max_scroll_offset.x);
                    state.scroll_offset.x =
                        state.scroll_offset.x.clamp(0.0, state.max_scroll_offset.x);

                    shell.invalidate_layout();
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
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
                    x: (state.scroll_target.x + dx).clamp(0.0, state.max_scroll_offset.x),
                    y: (state.scroll_target.y + dy).clamp(0.0, state.max_scroll_offset.y),
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

fn updated_width<P: Paragraph>(
    state: &mut State<P>,
    position: &Point,
    ColumnResize {
        left_col_index,
        start_x,
        left_start_width,
        right_start_width,
    }: ColumnResize,
) {
    let delta_x = position.x - start_x;
    if left_start_width + delta_x >= state.header_paragraphs[left_col_index].min_width() {
        state.column_widths[left_col_index] = left_start_width + delta_x;
    } else if left_col_index + 1 < state.column_widths.len() {
        let delta = delta_x.abs() - (left_start_width - state.column_widths[left_col_index]);
        state.column_widths[left_col_index + 1] = right_start_width + delta;
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
