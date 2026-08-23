//! A native data table widget with a frozen header row and a sticky
//! row-number gutter, scrolling both vertically and horizontally.
//!
//! Cells are drawn directly through the [`text::Renderer`] â€” there are no
//! child widgets, mirroring how the built-in `Text` widget works.
//!
//! The widget borrows its data rather than copying it: column names and rows
//! are handed in as slices and must outlive the widget. Columns and rows are
//! exposed through the [`TableColumn`] and [`TableRow`] traits, so any struct
//! can be displayed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::paragraph::Plain;
use iced::advanced::text::{self, Paragraph, Text};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Renderer, Shell};
use iced::alignment;
use iced::theme::Palette;
use iced::{Color, Element, Event, Font, Length, Pixels, Point, Size, keyboard, window};
use iced_renderer::graphics::text::paragraph;

/// Width of the row-number gutter column.
const GUTTER_WIDTH: f32 = 48.0;
/// Width of the scrollbar tracks.
const SCROLLBAR_WIDTH: f32 = 5.0;
/// Minimum length of a scrollbar scroller.
const MIN_SCROLLER: f32 = 24.0;

/// A column whose header name can be read for display.
pub trait TableColumn {
    /// The header text of this column.
    fn name(&self) -> &str;
}

/// A row whose display cells can be read.
pub trait TableRow {
    /// The display cells of this row.
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

/// A run of the data table widget.
///
/// `Col` is the column type and `Row` the row type; both only need to expose
/// their displayable data through [`Column`] and [`Row`]. The widget borrows
/// the data, so `columns` and `rows` must outlive it.
pub struct Table<'a, Col, Row>
where
    Col: TableColumn,
    Row: TableRow,
{
    columns: &'a [Col],
    rows: &'a [Row],
    gutter: bool,
    font: Font,
    text_size: f32,
    padding_x: f32,
    padding_y: f32,
    min_col_width: f32,
    max_col_width: f32,
}

impl<'a, Col, Row> Table<'a, Col, Row>
where
    Col: TableColumn,
    Row: TableRow,
{
    /// Creates a new [`Table`] borrowing the given column names and row cells.
    ///
    /// Every row must contain at least `columns.len()` cells; missing cells
    /// render as empty.
    pub fn new(columns: &'a [Col], rows: &'a [Row]) -> Self {
        Self {
            columns,
            rows,
            gutter: true,
            font: Font::MONOSPACE,
            text_size: 12.0,
            padding_x: 8.0,
            padding_y: 5.0,
            min_col_width: 80.0,
            max_col_width: 400.0,
        }
    }

    /// Sets whether the row-number gutter column is shown.
    pub fn gutter(mut self, gutter: bool) -> Self {
        self.gutter = gutter;
        self
    }

    /// Sets the font of the table cells.
    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    /// Sets the text size of the table cells.
    pub fn size(mut self, size: f32) -> Self {
        self.text_size = size;
        self
    }

    /// Sets the horizontal padding of every cell.
    pub fn padding_x(mut self, padding_x: f32) -> Self {
        self.padding_x = padding_x;
        self
    }

    /// Sets the vertical padding of every cell.
    pub fn padding_y(mut self, padding_y: f32) -> Self {
        self.padding_y = padding_y;
        self
    }

    fn refresh_measurements<P: Paragraph<Font = iced::Font>>(
        &self,
        renderer: &impl text::Renderer<Paragraph = P, Font = iced::Font>,
        state: &mut State<P>,
    ) {
        let hint_factor = renderer.hint_factor();

        // Sample the height of a single text line.
        measure_width(&mut state.measurer, self, hint_factor, "Xg");
        let line_height = state.measurer.min_bounds().height;
        state.row_height = line_height + self.padding_y * 2.0;
        state.header_height = state.row_height;

        let mut widths = Vec::with_capacity(self.columns.len());

        for (index, column) in self.columns.iter().enumerate() {
            let mut max = measure_width(&mut state.measurer, self, hint_factor, column.name());

            for row in self.rows.iter() {
                if let Some(cell) = row.cells().get(index) {
                    max = max.max(measure_width(
                        &mut state.measurer,
                        self,
                        hint_factor,
                        cell.as_str(),
                    ));
                }
            }

            widths.push((max + self.padding_x * 2.0).clamp(self.min_col_width, self.max_col_width));
        }

        state.column_widths = widths;
        state.content_width = state.column_widths.iter().sum::<f32>();
        state.content_height = state.header_height + self.rows.len() as f32 * state.row_height;
    }

    fn draw_gutter<R>(
        &self,
        state: &State<<R>::Paragraph>,
        renderer: &mut R,
        gutter_bounds: iced::Rectangle,
        palette: &Palette,
        cache: &mut std::cell::RefMut<'_, HashMap<(usize, usize), Plain<<R>::Paragraph>>>,
    ) where
        R: text::Renderer<Font = iced::Font>,
    {
        let body_top = gutter_bounds.y + state.header_height;
        let gutter_bg = Color::from_rgba(1.0, 1.0, 1.0, 0.06);
        let body_height = (gutter_bounds.height - state.header_height).max(0.0);
        let muted = crate::theme::TEXT_MUTED;

        renderer.with_layer(gutter_bounds, |renderer| {
            draw_separator(renderer, gutter_bounds, palette);

            draw_cell(
                cache,
                renderer,
                self,
                (HEADER, HEADER),
                "#",
                content_rect(
                    iced::Rectangle {
                        x: gutter_bounds.x,
                        y: gutter_bounds.y,
                        width: GUTTER_WIDTH,
                        height: state.header_height,
                    },
                    self,
                ),
                muted,
                gutter_bounds,
            );

            // Sticky gutter cells are drawn last so they always sit on top of
            // the clipped columns at the boundary.
            let rows = visible_rows(
                state.offset_y,
                state.row_height,
                body_height,
                self.rows.len(),
            );
            let gutter_indices_bounds = iced::Rectangle {
                x: gutter_bounds.x,
                y: gutter_bounds.y + state.header_height,
                width: gutter_bounds.width,
                height: gutter_bounds.height - state.header_height,
            };
            renderer.with_layer(gutter_indices_bounds, |renderer| {
                for row_index in rows {
                    let y = body_top + row_index as f32 * state.row_height - state.offset_y;

                    fill_quad(
                        renderer,
                        iced::Rectangle {
                            x: gutter_indices_bounds.x,
                            y,
                            width: GUTTER_WIDTH,
                            height: state.row_height,
                        },
                        gutter_bg,
                    );

                    draw_cell(
                        cache,
                        renderer,
                        self,
                        (row_index, GUTTER),
                        &(row_index + 1).to_string(),
                        content_rect(
                            iced::Rectangle {
                                x: gutter_indices_bounds.x,
                                y,
                                width: GUTTER_WIDTH,
                                height: state.row_height,
                            },
                            self,
                        ),
                        muted,
                        gutter_indices_bounds,
                    );
                }
            });
        })
    }
}

/// A cheap fingerprint of the table data, used to detect when cell
/// measurements must be recomputed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Signature {
    columns: usize,
    rows: usize,
    chars: u64,
}

impl Signature {
    fn of<Col, Row>(table: &Table<Col, Row>) -> Self
    where
        Col: TableColumn,
        Row: TableRow,
    {
        let chars = table
            .columns
            .iter()
            .map(|column| column.name().len())
            .sum::<usize>()
            + table
                .rows
                .iter()
                .flat_map(|row| row.cells().iter().map(|cell| cell.as_str().len()))
                .sum::<usize>();

        Self {
            columns: table.columns.len(),
            rows: table.rows.len(),
            chars: chars as u64,
        }
    }
}

/// Which scrollbar is being dragged.
#[derive(Debug, Clone, Copy)]
enum Drag {
    Vertical(f32),
    Horizontal(f32),
}

/// The internal state of a [`Table`].
struct State<P: Paragraph> {
    measurer: Plain<P>,
    signature: Signature,
    column_widths: Vec<f32>,
    row_height: f32,
    header_height: f32,
    content_width: f32,
    content_height: f32,
    offset_x: f32,
    offset_y: f32,
    target_x: f32,
    target_y: f32,
    last_frame: Option<Instant>,
    keyboard_modifiers: keyboard::Modifiers,
    paragraphs: RefCell<HashMap<(usize, usize), Plain<P>>>,
    dragging: Option<Drag>,
    cell_paragraphs: Vec<P>,
}

/// Row/column indices used to address cached paragraphs.
const HEADER: usize = usize::MAX;
const GUTTER: usize = usize::MAX;

impl<P: Paragraph> Default for State<P> {
    fn default() -> Self {
        Self {
            measurer: Plain::default(),
            signature: Signature {
                columns: usize::MAX,
                rows: usize::MAX,
                chars: u64::MAX,
            },
            column_widths: Vec::new(),
            row_height: 0.0,
            header_height: 0.0,
            content_width: 0.0,
            content_height: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            target_x: 0.0,
            target_y: 0.0,
            last_frame: None,
            keyboard_modifiers: keyboard::Modifiers::default(),
            paragraphs: RefCell::new(HashMap::new()),
            dragging: None,
            cell_paragraphs: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

fn track(bounds: iced::Rectangle, axis: Axis) -> iced::Rectangle {
    match axis {
        Axis::Vertical => iced::Rectangle {
            x: bounds.x + bounds.width - SCROLLBAR_WIDTH,
            y: bounds.y,
            width: SCROLLBAR_WIDTH,
            height: bounds.height - SCROLLBAR_WIDTH,
        },
        Axis::Horizontal => iced::Rectangle {
            x: bounds.x,
            y: bounds.y + bounds.height - SCROLLBAR_WIDTH,
            width: bounds.width - SCROLLBAR_WIDTH,
            height: SCROLLBAR_WIDTH,
        },
    }
}

fn max_offset_x(state: &State<impl Paragraph>, bounds: iced::Rectangle) -> f32 {
    (state.content_width + GUTTER_WIDTH - bounds.width).max(0.0)
}

fn max_offset_y(state: &State<impl Paragraph>, bounds: iced::Rectangle) -> f32 {
    let body_height = (bounds.height - state.header_height).max(0.0);
    (state.content_height - state.header_height - body_height).max(0.0)
}

fn scroller(
    state: &State<impl Paragraph>,
    bounds: iced::Rectangle,
    axis: Axis,
) -> Option<iced::Rectangle> {
    match axis {
        Axis::Vertical => {
            let max = max_offset_y(state, bounds);
            if max <= 0.0 {
                return None;
            }

            let track = track(bounds, axis);
            let height = (track.height * (bounds.height / state.content_height))
                .clamp(MIN_SCROLLER, track.height);
            let travel = (track.height - height).max(0.0);

            Some(iced::Rectangle {
                x: track.x,
                y: track.y + travel * (state.offset_y / max),
                width: SCROLLBAR_WIDTH,
                height,
            })
        }
        Axis::Horizontal => {
            let max = max_offset_x(state, bounds);
            if max <= 0.0 {
                return None;
            }

            let track = track(bounds, axis);
            let width = (track.width * (bounds.width / state.content_width))
                .clamp(MIN_SCROLLER, track.width);
            let travel = (track.width - width).max(0.0);

            Some(iced::Rectangle {
                x: track.x + travel * (state.offset_x / max),
                y: track.y,
                width,
                height: SCROLLBAR_WIDTH,
            })
        }
    }
}

/// Snaps the scroll offset so the thumb is centered on the click, if the
/// click fell on the scrollbar track (outside the thumb itself).
fn thumb_jump(
    state: &mut State<impl Paragraph>,
    bounds: iced::Rectangle,
    axis: Axis,
    position: f32,
) {
    let Some(rect) = scroller(state, bounds, axis) else {
        return;
    };

    let track = track(bounds, axis);
    let (travel, start, size) = match axis {
        Axis::Vertical => ((track.height - rect.height).max(0.0), track.y, rect.height),
        Axis::Horizontal => ((track.width - rect.width).max(0.0), track.x, rect.width),
    };

    if travel <= 0.0 {
        return;
    }

    let t = ((position - start - size / 2.0) / travel).clamp(0.0, 1.0);

    match axis {
        Axis::Vertical => {
            state.offset_y = t * max_offset_y(state, bounds);
            state.target_y = state.offset_y;
        }
        Axis::Horizontal => {
            state.offset_x = t * max_offset_x(state, bounds);
            state.target_x = state.offset_x;
        }
    }
}

fn measure_width<P, Col, Row>(
    measurer: &mut Plain<P>,
    table: &Table<Col, Row>,
    hint_factor: Option<f32>,
    content: &str,
) -> f32
where
    P: Paragraph<Font = iced::Font>,
    Col: TableColumn,
    Row: TableRow,
{
    measurer.update(Text {
        content,
        bounds: Size::new(f32::MAX, f32::MAX),
        size: Pixels(table.text_size),
        line_height: text::LineHeight::default(),
        font: Font::MONOSPACE,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::None,
        hint_factor,
    });

    measurer.min_bounds().width
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

    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &Limits) -> Node {
        let state = tree.state.downcast_mut::<State<R::Paragraph>>();

        self.refresh_measurements(renderer, state);
        state.paragraphs.borrow_mut().clear();

        let size = limits.max();
        let bounds = iced::Rectangle::new(Point::ORIGIN, size);
        state.offset_x = state.offset_x.clamp(0.0, max_offset_x(state, bounds));
        state.offset_y = state.offset_y.clamp(0.0, max_offset_y(state, bounds));
        state.target_x = state.target_x.clamp(0.0, max_offset_x(state, bounds));
        state.target_y = state.target_y.clamp(0.0, max_offset_y(state, bounds));

        Node::new(size)
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

                        (-x * 40.0, -y * state.row_height)
                    }
                    mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
                };

                let max_x = max_offset_x(state, bounds);
                let max_y = max_offset_y(state, bounds);

                let target_x = (state.target_x + dx).clamp(0.0, max_x);
                let target_y = (state.target_y + dy).clamp(0.0, max_y);

                if target_x != state.target_x || target_y != state.target_y {
                    state.target_x = target_x;
                    state.target_y = target_y;

                    if state.last_frame.is_none() {
                        state.last_frame = Some(Instant::now());
                    }

                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.keyboard_modifiers = *modifiers;
            }
            Event::Window(window::Event::RedrawRequested(now)) => {
                if state.last_frame.is_none() {
                    return;
                }

                let dt = state
                    .last_frame
                    .map_or(0.0, |last| (*now - last).as_secs_f32().clamp(0.0, 0.05));
                state.last_frame = Some(*now);

                let easing = 1.0 - (-dt * 24.0).exp();
                state.offset_x += (state.target_x - state.offset_x) * easing;
                state.offset_y += (state.target_y - state.offset_y) * easing;

                if (state.target_x - state.offset_x).abs() <= 0.1
                    && (state.target_y - state.offset_y).abs() <= 0.1
                {
                    state.offset_x = state.target_x;
                    state.offset_y = state.target_y;
                    state.last_frame = None;
                } else {
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(position) = cursor.position_over(bounds) else {
                    return;
                };

                let drag = if let Some(rect) = scroller(state, bounds, Axis::Vertical) {
                    if rect.contains(position) {
                        Some(Drag::Vertical(position.y - rect.y))
                    } else if track(bounds, Axis::Vertical).contains(position) {
                        thumb_jump(state, bounds, Axis::Vertical, position.y);
                        state.last_frame = None;
                        Some(Drag::Vertical(rect.height / 2.0))
                    } else {
                        None
                    }
                } else if let Some(rect) = scroller(state, bounds, Axis::Horizontal) {
                    if rect.contains(position) {
                        Some(Drag::Horizontal(position.x - rect.x))
                    } else if track(bounds, Axis::Horizontal).contains(position) {
                        thumb_jump(state, bounds, Axis::Horizontal, position.x);
                        state.last_frame = None;
                        Some(Drag::Horizontal(rect.width / 2.0))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if drag.is_some() {
                    state.dragging = drag;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let Some(drag) = state.dragging else {
                    return;
                };

                match drag {
                    Drag::Vertical(grab) => {
                        let track = track(bounds, Axis::Vertical);
                        let max = max_offset_y(state, bounds);

                        if let Some(rect) = scroller(state, bounds, Axis::Vertical) {
                            let travel = (track.height - rect.height).max(0.0);

                            if travel > 0.0 {
                                let t = ((position.y - track.y - grab) / travel).clamp(0.0, 1.0);
                                state.offset_y = t * max;
                                state.target_y = state.offset_y;
                                state.last_frame = None;
                            }
                        }
                    }
                    Drag::Horizontal(grab) => {
                        let track = track(bounds, Axis::Horizontal);
                        let max = max_offset_x(state, bounds);

                        if let Some(rect) = scroller(state, bounds, Axis::Horizontal) {
                            let travel = (track.width - rect.width).max(0.0);

                            if travel > 0.0 {
                                let t = ((position.x - track.x - grab) / travel).clamp(0.0, 1.0);
                                state.offset_x = t * max;
                                state.target_x = state.offset_x;
                                state.last_frame = None;
                            }
                        }
                    }
                }

                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging.is_some() {
                    state.dragging = None;
                    shell.capture_event();
                }
            }
            _ => {}
        }
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

        if state.column_widths.is_empty() {
            return;
        }

        let bounds = layout.bounds();
        let palette = theme.palette();

        let scroll_x = state.offset_x;
        let scroll_y = state.offset_y;

        let gutter_width = if self.gutter { GUTTER_WIDTH } else { 0.0 };
        let row_height = state.row_height;
        let body_top = bounds.y + state.header_height;
        let body_height = (bounds.height - state.header_height).max(0.0);

        let zebra = Color::from_rgba(1.0, 1.0, 1.0, 0.03);
        let separator_color = palette.background.strong.color;
        let header_bg = palette.background.stronger.color;
        let body_bg = palette.background.base.color;
        let text_color = crate::theme::TEXT;
        let muted = crate::theme::TEXT_MUTED;
        let track_bg = Color::from_rgba(1.0, 1.0, 1.0, 0.06);
        let scroller_bg = Color::from_rgba(1.0, 1.0, 1.0, 0.25);

        renderer.with_layer(bounds, |renderer| {
            fill_quad(renderer, bounds, body_bg);
            fill_header_bg(renderer, state, bounds, header_bg);

            let mut cache = state.paragraphs.borrow_mut();

            let gutter_bounds = iced::Rectangle {
                x: bounds.x,
                y: bounds.y,
                width: GUTTER_WIDTH,
                height: bounds.height,
            };

            self.draw_gutter(state, renderer, gutter_bounds, palette, &mut cache);

            let column_end_xs = compute_column_end_xs(&state.column_widths);
            let (first_col, last_col) = visible_columns(&column_end_xs, scroll_x, bounds.width);

            // Everything that scrolls horizontally — column separators, header
            // columns, and body cells — is clipped to the right of the sticky
            // gutter, so it can never paint over it.
            let body_bounds = iced::Rectangle {
                x: bounds.x + gutter_width,
                y: bounds.y,
                width: (bounds.width - gutter_width).max(0.0),
                height: bounds.height,
            };

            renderer.with_layer(body_bounds, |renderer| {
                for index in first_col..=last_col {
                    let cx = body_bounds.x + column_end_xs[index] - scroll_x;
                    fill_quad(renderer, quad_rect(body_bounds, cx, 1.0), separator_color);
                }

                for index in first_col..=last_col {
                    let cx = body_bounds.x + column_end_xs[index] - scroll_x;

                    draw_cell(
                        &mut cache,
                        renderer,
                        self,
                        (HEADER, index),
                        self.columns[index].name(),
                        content_rect(
                            iced::Rectangle {
                                x: cx,
                                y: body_bounds.y,
                                width: state.column_widths[index],
                                height: state.header_height,
                            },
                            self,
                        ),
                        text_color,
                        body_bounds,
                    );
                }

                if self.rows.is_empty() {
                    return;
                }

                // Body rows (visually culled to the viewport, clipped below
                // the header so scrolled content never paints over it).
                let rows = visible_rows(scroll_y, row_height, body_height, self.rows.len());

                let rows_body_bounds = iced::Rectangle {
                    x: body_bounds.x,
                    y: body_bounds.y + state.header_height,
                    width: body_bounds.width,
                    height: body_bounds.height - state.header_height,
                };
                renderer.with_layer(rows_body_bounds, |renderer| {
                    for row_index in rows {
                        let y = body_top + row_index as f32 * row_height - scroll_y;

                        if row_index % 2 == 1 {
                            fill_quad(
                                renderer,
                                iced::Rectangle {
                                    x: rows_body_bounds.x,
                                    y,
                                    width: rows_body_bounds.width,
                                    height: row_height,
                                },
                                zebra,
                            );
                        }

                        let row = self.rows[row_index].cells();

                        for index in first_col..=last_col {
                            let cell = row.get(index).map(String::as_str).unwrap_or("");
                            let color = if cell == "NULL" { muted } else { text_color };

                            draw_cell(
                                &mut cache,
                                renderer,
                                self,
                                (row_index, index),
                                cell,
                                content_rect(
                                    iced::Rectangle {
                                        x: rows_body_bounds.x + column_end_xs[index] - scroll_x,
                                        y,
                                        width: state.column_widths[index],
                                        height: row_height,
                                    },
                                    self,
                                ),
                                color,
                                rows_body_bounds,
                            );
                        }
                    }
                });
            });

            // Under-header separator, spanning the full width (also under the
            // gutter), drawn after the columns layer.
            if !self.rows.is_empty() {
                fill_quad(
                    renderer,
                    iced::Rectangle {
                        x: bounds.x,
                        y: bounds.y + state.header_height - 1.0,
                        width: bounds.width,
                        height: 1.0,
                    },
                    separator_color,
                );
            }

            // Drop paragraphs that scrolled out of (a margin around) the
            // viewport, keeping the cache bounded by the visible grid.
            let row_margin = 8usize;
            let col_margin = 2usize;

            let first_row = (scroll_y / row_height).floor().max(0.0) as usize;
            let last_row = ((scroll_y + body_height) / row_height).ceil() as usize;
            let row_span = first_row.checked_sub(row_margin).unwrap_or(0)..last_row + row_margin;
            let col_span = first_col.checked_sub(col_margin).unwrap_or(0)..last_col + col_margin;

            cache.retain(|&(row, col), _| {
                (row == HEADER && (col == HEADER || col_span.contains(&col)))
                    || (col == GUTTER && row_span.contains(&row))
                    || (row_span.contains(&row) && col_span.contains(&col))
            });

            // Scrollbars.
            if let Some(rect) = scroller(state, bounds, Axis::Vertical) {
                fill_quad(renderer, track(bounds, Axis::Vertical), track_bg);
                fill_quad(renderer, rect, scroller_bg);
            }

            if let Some(rect) = scroller(state, bounds, Axis::Horizontal) {
                fill_quad(renderer, track(bounds, Axis::Horizontal), track_bg);
                fill_quad(renderer, rect, scroller_bg);
            }
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

        let over = cursor.position_over(bounds).is_some_and(|position| {
            scroller(state, bounds, Axis::Vertical).is_some_and(|rect| rect.contains(position))
                || scroller(state, bounds, Axis::Horizontal)
                    .is_some_and(|rect| rect.contains(position))
        });

        if over {
            mouse::Interaction::AllScroll
        } else {
            mouse::Interaction::None
        }
    }
}

fn compute_column_end_xs(column_widths: &Vec<f32>) -> Vec<f32> {
    let mut xs = Vec::with_capacity(column_widths.len());
    let mut x = 0.0;
    for width in column_widths {
        xs.push(x);
        x += width;
    }
    xs
}

fn fill_header_bg<R>(
    renderer: &mut R,
    state: &State<<R>::Paragraph>,
    bounds: iced::Rectangle,
    header_bg: Color,
) where
    R: text::Renderer<Font = iced::Font>,
{
    fill_quad(
        renderer,
        iced::Rectangle {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: state.header_height,
        },
        header_bg,
    );
}

fn draw_separator<R>(renderer: &mut R, bounds: iced::Rectangle, palette: &Palette)
where
    R: text::Renderer<Font = iced::Font>,
{
    renderer.fill_quad(
        renderer::Quad {
            bounds: iced::Rectangle {
                x: bounds.x + GUTTER_WIDTH - 1.0,
                y: bounds.y,
                width: 1.0,
                height: bounds.height,
            },
            snap: true,
            ..renderer::Quad::default()
        },
        palette.background.strong.color,
    );
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

fn visible_columns(xs: &[f32], scroll_x: f32, viewport_width: f32) -> (usize, usize) {
    let viewport_end = scroll_x + viewport_width;
    let first_column_index = xs
        .iter()
        .position(|x| scroll_x < *x)
        .unwrap_or(0)
        .saturating_sub(1);
    let last_column_index = xs.iter().rposition(|x| *x < viewport_end).unwrap_or(0);

    (first_column_index, last_column_index)
}

/// Returns the range of rows overlapping a body viewport of `viewport_height`
/// pixels, scrolled by `scroll_y`.
fn visible_rows(
    scroll_y: f32,
    row_height: f32,
    viewport_height: f32,
    rows: usize,
) -> std::ops::Range<usize> {
    if rows == 0 || row_height <= 0.0 {
        return 0..0;
    }

    let first = (scroll_y / row_height).floor().max(0.0) as usize;
    let last = ((scroll_y + viewport_height) / row_height).ceil() as usize;

    first.min(rows)..last.min(rows)
}

/// The padded content box of a cell.
fn content_rect<Col, Row>(cell: iced::Rectangle, table: &Table<Col, Row>) -> iced::Rectangle
where
    Col: TableColumn,
    Row: TableRow,
{
    iced::Rectangle {
        x: cell.x + table.padding_x,
        y: cell.y + table.padding_y,
        width: (cell.width - table.padding_x * 2.0).max(0.0),
        height: (cell.height - table.padding_y * 2.0).max(0.0),
    }
}

/// A full-height, 1-pixel separator clipped to the table bounds.
fn quad_rect(bounds: iced::Rectangle, x: f32, width: f32) -> iced::Rectangle {
    iced::Rectangle {
        x,
        y: bounds.y,
        width,
        height: bounds.height,
    }
}

fn fill_quad(renderer: &mut impl Renderer, bounds: iced::Rectangle, color: Color) {
    renderer.fill_quad(
        renderer::Quad {
            bounds,
            snap: true,
            ..renderer::Quad::default()
        },
        color,
    );
}

/// Draws a cell, reusing its shaped [`Paragraph`] across frames so that
/// scrolling only pays for cells entering the viewport.
fn draw_cell<P, R, Col, Row>(
    content: &str,
    clip_bounds: iced::Rectangle,
    cell_bounds: iced::Rectangle,
    renderer: &mut R,
    color: Color,
    paragraph: &mut Plain<P>,
) where
    P: Paragraph<Font = iced::Font>,
    R: text::Renderer<Paragraph = P, Font = iced::Font>,
    Col: TableColumn,
    Row: TableRow,
{
    // why are we doing this here?
    paragraph.update(Text {
        content,
        bounds: Size::new(cell_bounds.width, cell_bounds.height),
        size: Pixels(12.0),
        line_height: text::LineHeight::default(),
        font: Font::MONOSPACE,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Center,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::None,
        hint_factor: renderer.hint_factor(),
    });

    renderer.fill_paragraph(
        paragraph.raw(),
        Point::new(cell_bounds.x, cell_bounds.y),
        color,
        clip_bounds,
    );
}
