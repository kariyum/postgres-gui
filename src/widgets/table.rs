//! A native data table widget with a frozen header row and a sticky
//! row-number gutter, scrolling both vertically and horizontally.
//!
//! Cells are drawn directly through the [`text::Renderer`] â€” there are no
//! child widgets, mirroring how the built-in `Text` widget works.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text::{self, Paragraph, Text};
use iced::advanced::text::paragraph::Plain;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Renderer, Shell};
use iced::alignment;
use iced::{Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, keyboard, window};

/// Width of the row-number gutter column.
const GUTTER_WIDTH: f32 = 48.0;
/// Width of the scrollbar tracks.
const SCROLLBAR_WIDTH: f32 = 10.0;
/// Minimum length of a scrollbar scroller.
const MIN_SCROLLER: f32 = 24.0;

/// A run of the data table widget.
pub struct Table {
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
    gutter: bool,
    font: Font,
    size: f32,
    padding_x: f32,
    padding_y: f32,
    min_col_width: f32,
    max_col_width: f32,
}

impl Table {
    /// Creates a new [`Table`] with the given column names and cell strings.
    ///
    /// Every row must contain at least `columns.len()` cells; missing cells
    /// render as empty.
    pub fn new(columns: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            columns,
            rows,
            gutter: true,
            font: Font::MONOSPACE,
            size: 12.0,
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
        self.size = size;
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
    fn of(table: &Table) -> Self {
        let chars = table.columns.iter().map(String::len).sum::<usize>()
            + table
                .rows
                .iter()
                .flat_map(|row| row.iter().map(String::len))
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
        }
    }
}

#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

fn track(bounds: Rectangle, axis: Axis) -> Rectangle {
    match axis {
        Axis::Vertical => Rectangle {
            x: bounds.x + bounds.width - SCROLLBAR_WIDTH,
            y: bounds.y,
            width: SCROLLBAR_WIDTH,
            height: bounds.height - SCROLLBAR_WIDTH,
        },
        Axis::Horizontal => Rectangle {
            x: bounds.x,
            y: bounds.y + bounds.height - SCROLLBAR_WIDTH,
            width: bounds.width - SCROLLBAR_WIDTH,
            height: SCROLLBAR_WIDTH,
        },
    }
}

fn max_offset_x(state: &State<impl Paragraph>, bounds: Rectangle) -> f32 {
    (state.content_width - bounds.width).max(0.0)
}

fn max_offset_y(state: &State<impl Paragraph>, bounds: Rectangle) -> f32 {
    let body_height = (bounds.height - state.header_height).max(0.0);
    (state.content_height - state.header_height - body_height).max(0.0)
}

fn scroller(state: &State<impl Paragraph>, bounds: Rectangle, axis: Axis) -> Option<Rectangle> {
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

            Some(Rectangle {
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

            Some(Rectangle {
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
fn thumb_jump(state: &mut State<impl Paragraph>, bounds: Rectangle, axis: Axis, position: f32) {
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

fn measure_width<P: Paragraph<Font = iced::Font>>(
    measurer: &mut Plain<P>,
    table: &Table,
    hint_factor: Option<f32>,
    content: &str,
) -> f32 {
    measurer.update(Text {
        content,
        bounds: Size::new(f32::MAX, f32::MAX),
        size: Pixels(table.size),
        line_height: text::LineHeight::default(),
        font: table.font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::None,
        hint_factor,
    });

    measurer.min_bounds().width
}

impl Table {
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

        for (index, name) in self.columns.iter().enumerate() {
            let mut max = measure_width(&mut state.measurer, self, hint_factor, name);

            for row in &self.rows {
                if let Some(cell) = row.get(index) {
                    max = max.max(measure_width(&mut state.measurer, self, hint_factor, cell));
                }
            }

            widths.push((max + self.padding_x * 2.0).clamp(self.min_col_width, self.max_col_width));
        }

        state.column_widths = widths;
        state.content_width =
            if self.gutter { GUTTER_WIDTH } else { 0.0 } + state.column_widths.iter().sum::<f32>();
        state.content_height = state.header_height + self.rows.len() as f32 * state.row_height;
    }
}

impl<Message, R> Widget<Message, iced::Theme, R> for Table
where
    R: text::Renderer<Font = iced::Font>,
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

        let signature = Signature::of(self);
        if state.signature != signature {
            state.signature = signature;
            self.refresh_measurements(renderer, state);
            state.offset_x = 0.0;
            state.offset_y = 0.0;
            state.target_x = 0.0;
            state.target_y = 0.0;
            state.paragraphs.borrow_mut().clear();
        }

        let size = limits.max();
        let bounds = Rectangle::new(Point::ORIGIN, size);
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
        _viewport: &Rectangle,
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
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();

        if state.column_widths.is_empty() {
            return;
        }

        let bounds = layout.bounds();
        let palette = theme.palette();

        let scroll_x = state.offset_x.clamp(0.0, max_offset_x(state, bounds));
        let scroll_y = state.offset_y.clamp(0.0, max_offset_y(state, bounds));

        let gutter_width = if self.gutter { GUTTER_WIDTH } else { 0.0 };
        let row_height = state.row_height;
        let body_top = bounds.y + state.header_height;

        let zebra = Color::from_rgba(1.0, 1.0, 1.0, 0.03);
        let gutter_bg = Color::from_rgba(1.0, 1.0, 1.0, 0.06);
        let separator = palette.background.strong.color;
        let header_bg = palette.background.strong.color;
        let body_bg = palette.background.base.color;
        let text_color = crate::theme::TEXT;
        let muted = crate::theme::TEXT_MUTED;
        let track_bg = Color::from_rgba(1.0, 1.0, 1.0, 0.06);
        let scroller_bg = Color::from_rgba(1.0, 1.0, 1.0, 0.25);

        renderer.with_layer(bounds, |renderer| {
            fill_quad(renderer, bounds, body_bg);
            fill_quad(
                renderer,
                Rectangle {
                    x: bounds.x,
                    y: bounds.y,
                    width: bounds.width,
                    height: state.header_height,
                },
                header_bg,
            );

            // Cached, already-shaped paragraphs of the visible cells.
            let mut cache = state.paragraphs.borrow_mut();

            // Column start positions, in content coordinates (the gutter is sticky).
            let mut xs = Vec::with_capacity(state.column_widths.len());
            let mut x = gutter_width;
            for &width in &state.column_widths {
                xs.push(x);
                x += width;
            }

            // Visible column range.
            let Some((first_col, last_col)) =
                visible_columns(&xs, &state.column_widths, scroll_x, bounds.width)
            else {
                return;
            };

            // Gutter separator + vertical column separators, spanning the whole height.
            if self.gutter {
                fill_quad(
                    renderer,
                    quad_rect(bounds, bounds.x + gutter_width - 1.0, 1.0),
                    separator,
                );
            }

            for index in first_col..=last_col {
                let cx = bounds.x + xs[index] - scroll_x;
                fill_quad(renderer, quad_rect(bounds, cx, 1.0), separator);
            }

            // Header row (sticky vertically, scrolls horizontally).
            if self.gutter {
                draw_cell(
                    &mut cache,
                    renderer,
                    self,
                    (HEADER, HEADER),
                    "#",
                    content_rect(
                        Rectangle {
                            x: bounds.x,
                            y: bounds.y,
                            width: gutter_width,
                            height: state.header_height,
                        },
                        self,
                    ),
                    muted,
                    bounds,
                );
            }

            for index in first_col..=last_col {
                let cx = bounds.x + xs[index] - scroll_x;

                draw_cell(
                    &mut cache,
                    renderer,
                    self,
                    (HEADER, index),
                    &self.columns[index],
                    content_rect(
                        Rectangle {
                            x: cx,
                            y: bounds.y,
                            width: state.column_widths[index],
                            height: state.header_height,
                        },
                        self,
                    ),
                    text_color,
                    bounds,
                );
            }

            if self.rows.is_empty() {
                return;
            }

            // Horizontal separator under the header.
            fill_quad(
                renderer,
                Rectangle {
                    x: bounds.x,
                    y: bounds.y + state.header_height - 1.0,
                    width: bounds.width,
                    height: 1.0,
                },
                separator,
            );

            // Body rows (visually culled to the viewport).
            let body_height = (bounds.height - state.header_height).max(0.0);
            let rows = visible_rows(scroll_y, row_height, body_height, self.rows.len());

            for row_index in rows {
                let y = body_top + row_index as f32 * row_height - scroll_y;

                if row_index % 2 == 1 {
                    fill_quad(
                        renderer,
                        Rectangle {
                            x: bounds.x,
                            y,
                            width: bounds.width,
                            height: row_height,
                        },
                        zebra,
                    );
                }

                if self.gutter {
                    fill_quad(
                        renderer,
                        Rectangle {
                            x: bounds.x,
                            y,
                            width: gutter_width,
                            height: row_height,
                        },
                        gutter_bg,
                    );

                    draw_cell(
                        &mut cache,
                        renderer,
                        self,
                        (row_index, GUTTER),
                        &(row_index + 1).to_string(),
                        content_rect(
                            Rectangle {
                                x: bounds.x,
                                y,
                                width: gutter_width,
                                height: row_height,
                            },
                            self,
                        ),
                        muted,
                        bounds,
                    );
                }

                let row = &self.rows[row_index];

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
                            Rectangle {
                                x: bounds.x + xs[index] - scroll_x,
                                y,
                                width: state.column_widths[index],
                                height: row_height,
                            },
                            self,
                        ),
                        color,
                        bounds,
                    );
                }
            }

            // Drop paragraphs that scrolled out of (a margin around) the
            // viewport, keeping the cache bounded by the visible grid.
            let row_margin = 8usize;
            let col_margin = 2usize;

            let first_row = (scroll_y / row_height).floor().max(0.0) as usize;
            let last_row = ((scroll_y + body_height) / row_height).ceil() as usize;
            let row_span = first_row.checked_sub(row_margin).unwrap_or(0)..last_row + row_margin;
            let col_span =
                first_col.checked_sub(col_margin).unwrap_or(0)..last_col + col_margin;

            cache.retain(|&(row, col), _| {
                row == HEADER || col == GUTTER || (row_span.contains(&row) && col_span.contains(&col))
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
        _viewport: &Rectangle,
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

impl<'a, Message, R> From<Table> for Element<'a, Message, iced::Theme, R>
where
    R: text::Renderer<Font = iced::Font> + 'a,
{
    fn from(table: Table) -> Self {
        Element::new(table)
    }
}

/// Returns the inclusive range of columns overlapping the visible window of
/// given width, starting at `scroll_x` (in content coordinates).
fn visible_columns(
    xs: &[f32],
    widths: &[f32],
    scroll_x: f32,
    viewport_width: f32,
) -> Option<(usize, usize)> {
    let mut first = None;
    let mut last = 0;

    for (index, (&cx, &width)) in xs.iter().zip(widths).enumerate() {
        if cx + width <= scroll_x {
            continue;
        }

        if first.is_none() {
            first = Some(index);
        }

        if cx > scroll_x + viewport_width {
            break;
        }

        last = index;
    }

    first.map(|first| (first, last))
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
fn content_rect(cell: Rectangle, table: &Table) -> Rectangle {
    Rectangle {
        x: cell.x + table.padding_x,
        y: cell.y + table.padding_y,
        width: (cell.width - table.padding_x * 2.0).max(0.0),
        height: (cell.height - table.padding_y * 2.0).max(0.0),
    }
}

/// A full-height, 1-pixel separator clipped to the table bounds.
fn quad_rect(bounds: Rectangle, x: f32, width: f32) -> Rectangle {
    Rectangle {
        x,
        y: bounds.y,
        width,
        height: bounds.height,
    }
}

fn fill_quad(renderer: &mut impl Renderer, bounds: Rectangle, color: Color) {
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
fn draw_cell<P: Paragraph<Font = iced::Font>>(
    cache: &mut HashMap<(usize, usize), Plain<P>>,
    renderer: &mut impl text::Renderer<Paragraph = P, Font = iced::Font>,
    table: &Table,
    key: (usize, usize),
    content: &str,
    rect: Rectangle,
    color: Color,
    clip_bounds: Rectangle,
) {
    let paragraph = cache.entry(key).or_default();
    paragraph.update(Text {
        content,
        bounds: Size::new(rect.width, rect.height),
        size: Pixels(table.size),
        line_height: text::LineHeight::default(),
        font: table.font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Center,
        shaping: text::Shaping::default(),
        wrapping: text::Wrapping::None,
        ellipsis: text::Ellipsis::None,
        hint_factor: renderer.hint_factor(),
    });

    renderer.fill_paragraph(
        paragraph.raw(),
        Point::new(rect.x, rect.y),
        color,
        clip_bounds,
    );
}
