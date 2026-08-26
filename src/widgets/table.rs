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
use tracing::instrument;

const GUTTER_WIDTH: f32 = 48.0;
const SCROLLBAR_WIDTH: f32 = 5.0;
const MIN_SCROLLER: f32 = 24.0;

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
    row_height: f32,
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
            row_height: 24.0,
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

        state.gutter_paragraphs[0].update(text_config.with_content("#"));
        for i in 1..=self.rows.len() {
            state.gutter_paragraphs[i].update(text_config.with_content(&i.to_string()));
        }

        for (i, cell) in self.rows.iter().flat_map(|row| row.cells()).enumerate() {
            state.body_paragraphs[i].update(text_config.with_content(cell));
        }

        Node::new(limits.max())
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
        }
    }
}

#[derive(Clone, Copy)]
enum Axis {
    Vertical,
    Horizontal,
}

#[instrument(skip_all, fields(len = %rows_len))]
fn compute_gutter_paragraphs<P>(
    rows_len: usize,
    text_config: Text<&str, <P as Paragraph>::Font>,
) -> Vec<Plain<P>>
where
    P: Paragraph,
    P::Font: std::fmt::Debug,
{
    let start = Instant::now();
    let mut gutter_paragraphs = Vec::with_capacity(rows_len + 1);
    gutter_paragraphs.resize_with(rows_len + 1, Plain::<P>::default);
    gutter_paragraphs[0].update(text_config.with_content("#"));
    for i in 1..=rows_len {
        gutter_paragraphs[i].update(text_config.with_content(&i.to_string()));
    }
    tracing::info!("took {:?}", start.elapsed());
    gutter_paragraphs
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

    fn layout(&mut self, _: &mut Tree, _: &R, limits: &Limits) -> Node {
        Node::new(limits.resolve(
            self.width,
            self.height,
            Size {
                width: self.columns.len() as f32 * (self.min_col_width + self.separator_width)
                    + self.gutter_width,
                height: self.rows.len() as f32 * (self.row_height + self.separator_width),
            },
        ))
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
        let palette = theme.palette();
        let text_config = Text {
            content: "",
            bounds: layout.bounds().size(),
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
        println!("viewport {:?}", _viewport);
        renderer.with_layer(*_viewport, |renderer| {
            let gutter_bounds = iced::Rectangle {
                width: GUTTER_WIDTH,
                ..*_viewport
            };
            renderer.with_layer(gutter_bounds, |renderer| {
                let gutter_paragraphs =
                    compute_gutter_paragraphs::<R::Paragraph>(self.rows.len(), text_config);
                for (i, cell) in gutter_paragraphs.iter().enumerate() {
                    println!(
                        "cell min bounds = {:?}, cell.raw.bounds() = {:?}",
                        cell.min_bounds(),
                        cell.raw().bounds()
                    );
                    renderer.fill_paragraph(
                        cell.raw(),
                        Point {
                            x: gutter_bounds.x,
                            y: i as f32 * cell.min_height(),
                        },
                        Color::WHITE,
                        iced::Rectangle {
                            x: gutter_bounds.x,
                            y: i as f32 * cell.min_height(),
                            width: cell.min_bounds().width,
                            height: cell.min_bounds().height,
                        },
                    );
                }
            });
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
