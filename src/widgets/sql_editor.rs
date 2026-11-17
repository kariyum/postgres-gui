//! A custom multi-line SQL editor widget.
//!
//! * [`Content`] is the pure text model (lines, cursor, selection, undo/redo).
//! * [`Action`] is what the widget publishes; apply it with [`Content::perform`].
//! * [`SqlEditor`] is the widget. It scrolls to keep the caret visible, draws a
//!   line-number gutter, a selection, a blinking caret and basic SQL highlighting.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use iced::advanced::clipboard;
use iced::advanced::layout::{Layout, Limits, Node};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::{self, Paragraph as _, Text};
use iced::advanced::widget::{self, Id, Operation, Tree, Widget, operation};
use iced::advanced::{Shell, mouse, overlay};
use iced::{
    Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Size, Theme, Vector, alignment,
    keyboard, window,
};

const CARET_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const MULTI_CLICK_INTERVAL: Duration = Duration::from_millis(400);
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 6.0;
const GUTTER_PAD: f32 = 8.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const TAB_WIDTH: usize = 4;
const UNDO_LIMIT: usize = 200;

static REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    REVISION.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// A position in the text. `col` counts characters, not bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    /// Move by N lines (negative = up).
    Lines(isize),
    WordLeft,
    WordRight,
    Home,
    End,
    DocumentStart,
    DocumentEnd,
}

#[derive(Debug, Clone)]
pub enum Action {
    /// Typed text (coalesced for undo).
    Type(String),
    Paste(String),
    Enter,
    Tab,
    Backspace,
    Delete,
    DeleteWord { backward: bool },
    Move { motion: Motion, select: bool },
    Click { pos: Pos, select: bool },
    Drag(Pos),
    SelectWord(Pos),
    SelectLine(Pos),
    SelectAll,
    Undo,
    Redo,
    /// Ctrl+Space: ask the owner to compute suggestions now.
    RequestCompletion,
    CompletionMove(isize),
    CompletionAccept,
    CompletionPick(usize),
    CompletionDismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Keyword,
    Function,
    Schema,
    Table,
    View,
    Column,
    Alias,
}

impl ItemKind {
    fn badge(self) -> &'static str {
        match self {
            ItemKind::Keyword => "k",
            ItemKind::Function => "f",
            ItemKind::Schema => "s",
            ItemKind::Table => "T",
            ItemKind::View => "V",
            ItemKind::Column => "c",
            ItemKind::Alias => "a",
        }
    }

    fn color(self) -> Color {
        match self {
            ItemKind::Keyword => Color::from_rgb(0.78, 0.57, 0.92),
            ItemKind::Function => Color::from_rgb(0.45, 0.75, 0.95),
            ItemKind::Schema => Color::from_rgb(0.85, 0.75, 0.45),
            ItemKind::Table | ItemKind::View => Color::from_rgb(0.95, 0.65, 0.40),
            ItemKind::Column => Color::from_rgb(0.60, 0.80, 0.50),
            ItemKind::Alias => Color::from_rgb(0.80, 0.80, 0.80),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    /// Text that replaces the typed prefix when accepted.
    pub insert: String,
    pub detail: String,
    pub kind: ItemKind,
}

/// The suggestion popup state. Computed by the owner, rendered by the widget.
#[derive(Debug, Clone)]
pub struct Completion {
    pub items: Vec<CompletionItem>,
    pub selected: usize,
    /// Where the typed prefix starts (replaced on accept).
    pub start: Pos,
}

const MAX_VISIBLE_ITEMS: usize = 8;

/// First visible item index so that `selected` is always inside the window.
fn popup_first(selected: usize, len: usize) -> usize {
    let visible = len.min(MAX_VISIBLE_ITEMS);
    selected
        .saturating_sub(visible.saturating_sub(1))
        .min(len - visible)
}

#[derive(Debug, Clone)]
struct Snapshot {
    lines: Vec<String>,
    cursor: Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Other,
}

#[derive(Debug, Clone)]
pub struct Content {
    lines: Vec<String>,
    cursor: Pos,
    anchor: Option<Pos>,
    goal_col: Option<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<EditKind>,
    revision: u64,
    completion: Option<Completion>,
}

impl Default for Content {
    fn default() -> Self {
        Self::new()
    }
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn byte_idx(s: &str, col: usize) -> usize {
    s.char_indices().nth(col).map(|(i, _)| i).unwrap_or(s.len())
}

fn split_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', &" ".repeat(TAB_WIDTH))
        .split('\n')
        .map(String::from)
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Content {
    pub fn new() -> Self {
        Self::with_text("")
    }

    pub fn with_text(text: &str) -> Self {
        Self {
            lines: split_lines(text),
            cursor: Pos::default(),
            anchor: None,
            goal_col: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            revision: next_revision(),
            completion: None,
        }
    }

    pub fn completion(&self) -> Option<&Completion> {
        self.completion.as_ref()
    }

    pub fn set_completion(&mut self, completion: Option<Completion>) {
        self.completion = completion.filter(|c| !c.items.is_empty());
    }

    fn accept_completion(&mut self, index: Option<usize>) {
        let Some(completion) = self.completion.take() else {
            return;
        };
        let Some(item) = completion.items.get(index.unwrap_or(completion.selected)) else {
            return;
        };
        let insert = item.insert.clone();
        self.begin_edit(EditKind::Other);
        self.anchor = Some(completion.start);
        self.insert(&insert);
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(String::as_str)
    }

    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Ordered selection range, if any.
    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let anchor = self.anchor?;
        if anchor == self.cursor {
            None
        } else if anchor < self.cursor {
            Some((anchor, self.cursor))
        } else {
            Some((self.cursor, anchor))
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        if a.line == b.line {
            let line = &self.lines[a.line];
            return Some(line[byte_idx(line, a.col)..byte_idx(line, b.col)].to_string());
        }
        let mut out = String::new();
        let first = &self.lines[a.line];
        out.push_str(&first[byte_idx(first, a.col)..]);
        for line in &self.lines[a.line + 1..b.line] {
            out.push('\n');
            out.push_str(line);
        }
        out.push('\n');
        let last = &self.lines[b.line];
        out.push_str(&last[..byte_idx(last, b.col)]);
        Some(out)
    }

    fn clamp(&self, pos: Pos) -> Pos {
        let line = pos.line.min(self.lines.len() - 1);
        Pos {
            line,
            col: pos.col.min(char_len(&self.lines[line])),
        }
    }

    fn line_len(&self, line: usize) -> usize {
        char_len(&self.lines[line])
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor: self.cursor,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.lines = snapshot.lines;
        self.cursor = snapshot.cursor;
        self.anchor = None;
        self.goal_col = None;
        self.last_edit = None;
        self.revision = next_revision();
    }

    /// Call before every mutation: records undo state and bumps the revision.
    fn begin_edit(&mut self, kind: EditKind) {
        let coalesce = kind == EditKind::Typing && self.last_edit == Some(EditKind::Typing);
        if !coalesce {
            self.undo.push(self.snapshot());
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_edit = Some(kind);
        self.goal_col = None;
        self.revision = next_revision();
    }

    fn delete_range(&mut self, a: Pos, b: Pos) {
        let head = {
            let line = &self.lines[a.line];
            line[..byte_idx(line, a.col)].to_string()
        };
        let tail = {
            let line = &self.lines[b.line];
            line[byte_idx(line, b.col)..].to_string()
        };
        self.lines.splice(a.line..=b.line, [head + &tail]);
        self.cursor = a;
        self.anchor = None;
    }

    fn delete_selection(&mut self) -> bool {
        if let Some((a, b)) = self.selection() {
            self.delete_range(a, b);
            true
        } else {
            self.anchor = None;
            false
        }
    }

    fn insert(&mut self, text: &str) {
        self.delete_selection();
        let parts = split_lines(text);
        let c = self.cursor;
        let (head, tail) = {
            let line = &self.lines[c.line];
            let i = byte_idx(line, c.col);
            (line[..i].to_string(), line[i..].to_string())
        };
        if parts.len() == 1 {
            self.lines[c.line] = format!("{head}{}{tail}", parts[0]);
            self.cursor.col += char_len(&parts[0]);
        } else {
            let last_index = parts.len() - 1;
            let mut new_lines = Vec::with_capacity(parts.len());
            new_lines.push(format!("{head}{}", parts[0]));
            new_lines.extend(parts[1..last_index].iter().cloned());
            new_lines.push(format!("{}{tail}", parts[last_index]));
            self.cursor = Pos {
                line: c.line + last_index,
                col: char_len(&parts[last_index]),
            };
            self.lines.splice(c.line..=c.line, new_lines);
        }
    }

    fn word_left(&self, from: Pos) -> Pos {
        if from.col == 0 {
            return if from.line > 0 {
                Pos {
                    line: from.line - 1,
                    col: self.line_len(from.line - 1),
                }
            } else {
                from
            };
        }
        let chars: Vec<char> = self.lines[from.line].chars().collect();
        let mut col = from.col;
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        if col > 0 {
            let word = is_word_char(chars[col - 1]);
            while col > 0
                && !chars[col - 1].is_whitespace()
                && is_word_char(chars[col - 1]) == word
            {
                col -= 1;
            }
        }
        Pos {
            line: from.line,
            col,
        }
    }

    fn word_right(&self, from: Pos) -> Pos {
        let len = self.line_len(from.line);
        if from.col >= len {
            return if from.line + 1 < self.lines.len() {
                Pos {
                    line: from.line + 1,
                    col: 0,
                }
            } else {
                from
            };
        }
        let chars: Vec<char> = self.lines[from.line].chars().collect();
        let mut col = from.col;
        let word = is_word_char(chars[col]);
        while col < len && !chars[col].is_whitespace() && is_word_char(chars[col]) == word {
            col += 1;
        }
        while col < len && chars[col].is_whitespace() {
            col += 1;
        }
        Pos {
            line: from.line,
            col,
        }
    }

    fn move_cursor(&mut self, motion: Motion, select: bool) {
        if select {
            self.anchor.get_or_insert(self.cursor);
        } else if let Some((a, b)) = self.selection()
            && matches!(motion, Motion::Left | Motion::Right)
        {
            self.cursor = if motion == Motion::Left { a } else { b };
            self.anchor = None;
            self.goal_col = None;
            return;
        } else {
            self.anchor = None;
        }

        let mut vertical = false;
        let c = self.cursor;
        match motion {
            Motion::Left => {
                if c.col > 0 {
                    self.cursor.col -= 1;
                } else if c.line > 0 {
                    self.cursor = Pos {
                        line: c.line - 1,
                        col: self.line_len(c.line - 1),
                    };
                }
            }
            Motion::Right => {
                if c.col < self.line_len(c.line) {
                    self.cursor.col += 1;
                } else if c.line + 1 < self.lines.len() {
                    self.cursor = Pos {
                        line: c.line + 1,
                        col: 0,
                    };
                }
            }
            Motion::Lines(n) => {
                vertical = true;
                let goal = *self.goal_col.get_or_insert(c.col);
                let target = (c.line as isize + n).clamp(0, self.lines.len() as isize - 1) as usize;
                self.cursor = Pos {
                    line: target,
                    col: goal.min(self.line_len(target)),
                };
                // Up on first line / Down on last line jump to the line boundary.
                if target == c.line {
                    self.cursor.col = if n < 0 { 0 } else { self.line_len(target) };
                    self.goal_col = None;
                }
            }
            Motion::WordLeft => self.cursor = self.word_left(c),
            Motion::WordRight => self.cursor = self.word_right(c),
            Motion::Home => {
                let first_non_ws = self.lines[c.line]
                    .chars()
                    .take_while(|ch| ch.is_whitespace())
                    .count();
                self.cursor.col = if c.col == first_non_ws { 0 } else { first_non_ws };
            }
            Motion::End => self.cursor.col = self.line_len(c.line),
            Motion::DocumentStart => self.cursor = Pos::default(),
            Motion::DocumentEnd => {
                let line = self.lines.len() - 1;
                self.cursor = Pos {
                    line,
                    col: self.line_len(line),
                };
            }
        }
        if !vertical {
            self.goal_col = None;
        }
        self.last_edit = None;
    }

    fn word_bounds(&self, pos: Pos) -> (Pos, Pos) {
        let pos = self.clamp(pos);
        let chars: Vec<char> = self.lines[pos.line].chars().collect();
        if chars.is_empty() {
            return (pos, pos);
        }
        let probe = pos.col.min(chars.len() - 1);
        let class = |c: char| {
            if is_word_char(c) {
                0
            } else if c.is_whitespace() {
                1
            } else {
                2
            }
        };
        let kind = class(chars[probe]);
        let mut start = probe;
        while start > 0 && class(chars[start - 1]) == kind {
            start -= 1;
        }
        let mut end = probe + 1;
        while end < chars.len() && class(chars[end]) == kind {
            end += 1;
        }
        (
            Pos {
                line: pos.line,
                col: start,
            },
            Pos {
                line: pos.line,
                col: end,
            },
        )
    }

    pub fn perform(&mut self, action: Action) {
        match action {
            Action::Type(text) => {
                self.begin_edit(EditKind::Typing);
                self.insert(&text);
            }
            Action::Paste(text) => {
                self.begin_edit(EditKind::Other);
                self.insert(&text);
            }
            Action::Enter => {
                self.begin_edit(EditKind::Other);
                let indent: String = self.lines[self.cursor.line]
                    .chars()
                    .take(self.cursor.col)
                    .take_while(|c| *c == ' ')
                    .collect();
                self.insert(&format!("\n{indent}"));
            }
            Action::Tab => {
                self.begin_edit(EditKind::Other);
                self.insert(&" ".repeat(TAB_WIDTH));
            }
            Action::Backspace => {
                self.begin_edit(EditKind::Other);
                if !self.delete_selection() {
                    let c = self.cursor;
                    if c.col > 0 {
                        self.delete_range(Pos { col: c.col - 1, ..c }, c);
                    } else if c.line > 0 {
                        let start = Pos {
                            line: c.line - 1,
                            col: self.line_len(c.line - 1),
                        };
                        self.delete_range(start, c);
                    }
                }
            }
            Action::Delete => {
                self.begin_edit(EditKind::Other);
                if !self.delete_selection() {
                    let c = self.cursor;
                    if c.col < self.line_len(c.line) {
                        self.delete_range(c, Pos { col: c.col + 1, ..c });
                    } else if c.line + 1 < self.lines.len() {
                        self.delete_range(
                            c,
                            Pos {
                                line: c.line + 1,
                                col: 0,
                            },
                        );
                    }
                }
            }
            Action::DeleteWord { backward } => {
                self.begin_edit(EditKind::Other);
                if !self.delete_selection() {
                    let c = self.cursor;
                    if backward {
                        let target = self.word_left(c);
                        self.delete_range(target, c);
                    } else {
                        let target = self.word_right(c);
                        self.delete_range(c, target);
                    }
                }
            }
            Action::Move { motion, select } => self.move_cursor(motion, select),
            Action::Click { pos, select } => {
                if select {
                    self.anchor.get_or_insert(self.cursor);
                } else {
                    self.anchor = None;
                }
                self.cursor = self.clamp(pos);
                self.goal_col = None;
                self.last_edit = None;
            }
            Action::Drag(pos) => {
                self.anchor.get_or_insert(self.cursor);
                self.cursor = self.clamp(pos);
                self.goal_col = None;
            }
            Action::SelectWord(pos) => {
                let (a, b) = self.word_bounds(pos);
                self.anchor = Some(a);
                self.cursor = b;
                self.last_edit = None;
            }
            Action::SelectLine(pos) => {
                let pos = self.clamp(pos);
                self.anchor = Some(Pos {
                    line: pos.line,
                    col: 0,
                });
                self.cursor = Pos {
                    line: pos.line,
                    col: self.line_len(pos.line),
                };
                self.last_edit = None;
            }
            Action::SelectAll => {
                self.anchor = Some(Pos::default());
                let line = self.lines.len() - 1;
                self.cursor = Pos {
                    line,
                    col: self.line_len(line),
                };
                self.last_edit = None;
            }
            Action::Undo => {
                if let Some(previous) = self.undo.pop() {
                    self.redo.push(self.snapshot());
                    self.restore(previous);
                }
            }
            Action::Redo => {
                if let Some(next) = self.redo.pop() {
                    self.undo.push(self.snapshot());
                    self.restore(next);
                }
            }
            Action::RequestCompletion => {}
            Action::CompletionMove(delta) => {
                if let Some(c) = &mut self.completion {
                    let len = c.items.len() as isize;
                    c.selected = (c.selected as isize + delta).rem_euclid(len) as usize;
                }
            }
            Action::CompletionAccept => self.accept_completion(None),
            Action::CompletionPick(index) => self.accept_completion(Some(index)),
            Action::CompletionDismiss => self.completion = None,
        }
    }
}

// ---------------------------------------------------------------------------
// Syntax highlighting
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Plain,
    Keyword,
    Str,
    Number,
    Comment,
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "select" | "from" | "where" | "and" | "or" | "not" | "insert" | "into" | "values"
            | "update" | "set" | "delete" | "create" | "table" | "drop" | "alter" | "add"
            | "column" | "index" | "view" | "as" | "join" | "left" | "right" | "inner"
            | "outer" | "full" | "cross" | "on" | "group" | "by" | "order" | "having"
            | "limit" | "offset" | "distinct" | "union" | "all" | "intersect" | "except"
            | "case" | "when" | "then" | "else" | "end" | "null" | "is" | "in" | "like"
            | "ilike" | "between" | "exists" | "primary" | "key" | "foreign" | "references"
            | "default" | "unique" | "check" | "constraint" | "with" | "returning" | "asc"
            | "desc" | "true" | "false" | "cast" | "begin" | "commit" | "rollback"
            | "explain" | "analyze" | "truncate" | "schema" | "database" | "if" | "using"
            | "natural" | "over" | "partition" | "recursive" | "lateral" | "any" | "some"
    )
}

/// Tokenizes one line. `in_block` carries `/* ... */` state across lines.
/// Returned ranges are in characters.
fn tokenize(line: &str, in_block: &mut bool) -> Vec<(usize, usize, Tok)> {
    let chars: Vec<char> = line.chars().collect();
    let n = chars.len();
    let mut out: Vec<(usize, usize, Tok)> = Vec::new();
    fn push_tok(out: &mut Vec<(usize, usize, Tok)>, s: usize, e: usize, t: Tok) {
        if let Some(last) = out.last_mut()
            && last.2 == t
            && last.1 == s
            && t == Tok::Plain
        {
            last.1 = e;
        } else {
            out.push((s, e, t));
        }
    }
    let push = |out: &mut Vec<(usize, usize, Tok)>, s: usize, e: usize, t: Tok| {
        push_tok(out, s, e, t);
    };

    let mut i = 0;
    while i < n {
        if *in_block {
            let mut j = i;
            let mut closed = false;
            while j < n {
                if chars[j] == '*' && j + 1 < n && chars[j + 1] == '/' {
                    j += 2;
                    closed = true;
                    break;
                }
                j += 1;
            }
            push(&mut out, i, j, Tok::Comment);
            if closed {
                *in_block = false;
            }
            i = j;
            continue;
        }
        let c = chars[i];
        if c == '-' && i + 1 < n && chars[i + 1] == '-' {
            push(&mut out, i, n, Tok::Comment);
            break;
        } else if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            *in_block = true;
            // Consume the opener, then continue as a block comment.
            push(&mut out, i, i + 2, Tok::Comment);
            i += 2;
        } else if c == '\'' {
            let mut j = i + 1;
            while j < n {
                if chars[j] == '\'' {
                    if j + 1 < n && chars[j + 1] == '\'' {
                        j += 2;
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            push(&mut out, i, j, Tok::Str);
            i = j;
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < n && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            push(&mut out, i, j, Tok::Number);
            i = j;
        } else if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < n && is_word_char(chars[j]) {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            push(
                &mut out,
                i,
                j,
                if is_keyword(&word) { Tok::Keyword } else { Tok::Plain },
            );
            i = j;
        } else {
            push(&mut out, i, i + 1, Tok::Plain);
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Widget
// ---------------------------------------------------------------------------

pub struct SqlEditor<'a, Message> {
    content: &'a Content,
    on_action: Box<dyn Fn(Action) -> Message + 'a>,
    id: Option<Id>,
    font: Font,
    size: f32,
    line_height: f32,
    width: Length,
    height: Length,
}

impl<'a, Message> SqlEditor<'a, Message> {
    pub fn new(content: &'a Content, on_action: impl Fn(Action) -> Message + 'a) -> Self {
        Self {
            content,
            on_action: Box::new(on_action),
            id: None,
            font: Font::MONOSPACE,
            size: 14.0,
            line_height: 20.0,
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    pub fn id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        let size = size.into().0;
        self.size = size;
        self.line_height = (size * 1.4).round();
        self
    }

    pub fn font(mut self, font: Font) -> Self {
        self.font = font;
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

    fn gutter_width(&self, char_width: f32) -> f32 {
        let digits = self.content.line_count().to_string().len().max(2);
        digits as f32 * char_width + 2.0 * GUTTER_PAD
    }

    fn text_origin<P>(&self, bounds: Rectangle, state: &State<P>) -> Point {
        Point::new(
            bounds.x + self.gutter_width(state.char_width) + PAD_X,
            bounds.y + PAD_Y,
        )
    }

    fn viewport_size<P>(&self, bounds: Rectangle, state: &State<P>) -> Size {
        Size::new(
            (bounds.width - self.gutter_width(state.char_width) - PAD_X - SCROLLBAR_WIDTH)
                .max(1.0),
            (bounds.height - 2.0 * PAD_Y).max(1.0),
        )
    }

    fn shape_line<R>(&self, line: &str, in_block: &mut bool, hint: Option<f32>) -> R::Paragraph
    where
        R: text::Renderer<Font = Font>,
    {
        if line.is_empty() {
            let _ = tokenize(line, in_block);
            return R::Paragraph::with_text(Text {
                content: "",
                bounds: Size::new(f32::INFINITY, self.line_height),
                size: Pixels(self.size),
                line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
                font: self.font,
                align_x: text::Alignment::Left,
                align_y: alignment::Vertical::Top,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
                ellipsis: text::Ellipsis::None,
                hint_factor: hint,
            });
        }

        let chars: Vec<char> = line.chars().collect();
        let tokens = tokenize(line, in_block);
        let spans: Vec<text::Span<'_, (), Font>> = tokens
            .into_iter()
            .map(|(s, e, tok)| {
                let run: String = chars[s..e].iter().collect();
                let mut span = text::Span::new(run);
                if tok != Tok::Plain {
                    span = span.color(token_color(tok, Color::WHITE));
                }
                span
            })
            .collect();

        R::Paragraph::with_spans(Text {
            content: &spans,
            bounds: Size::new(f32::INFINITY, self.line_height),
            size: Pixels(self.size),
            line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Advanced,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: hint,
        })
    }

    fn sync_paragraphs<R>(&self, state: &mut State<R::Paragraph>, hint: Option<f32>)
    where
        R: text::Renderer<Font = Font>,
    {
        let rev = self.content.revision();
        let needs_rebuild = state.shaped_revision != Some(rev)
            || state.shaped_size != self.size
            || state.shaped_font != self.font
            || state.shaped_hint != hint
            || state.paragraphs.len() != self.content.line_count();

        if !needs_rebuild {
            return;
        }

        state.paragraphs.clear();
        state.paragraphs.reserve(self.content.line_count());
        let mut in_block = false;
        for i in 0..self.content.line_count() {
            let line = self.content.line(i).unwrap_or("");
            state.paragraphs.push(self.shape_line::<R>(line, &mut in_block, hint));
        }
        state.shaped_revision = Some(rev);
        state.shaped_size = self.size;
        state.shaped_font = self.font;
        state.shaped_hint = hint;
    }

    fn col_x<P: text::Paragraph<Font = Font>>(&self, state: &State<P>, line_idx: usize, col: usize) -> f32 {
        if col == 0 {
            return 0.0;
        }
        let line = self.content.line(line_idx).unwrap_or("");
        let len = char_len(line);
        let clamped_col = col.min(len);
        let prefix = &line[..byte_idx(line, clamped_col)];
        let shaping = if prefix.is_ascii() {
            text::Shaping::Basic
        } else {
            text::Shaping::Advanced
        };
        let x = P::with_text(Text {
            content: prefix,
            bounds: Size::new(f32::INFINITY, self.line_height),
            size: Pixels(self.size),
            line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: state.shaped_hint,
        })
        .min_width();
        if col > len {
            x + (col - len) as f32 * state.char_width
        } else {
            x
        }
    }

    fn max_scroll<P: text::Paragraph>(&self, bounds: Rectangle, state: &State<P>) -> Point {
        let view = self.viewport_size(bounds, state);
        let total_h = self.content.line_count() as f32 * self.line_height;
        let max_line_w = state
            .paragraphs
            .iter()
            .map(P::min_width)
            .fold(0.0_f32, f32::max);
        let total_w = max_line_w + 4.0 * state.char_width;
        Point::new((total_w - view.width).max(0.0), (total_h - view.height).max(0.0))
    }

    fn clamp_scroll<P: text::Paragraph>(&self, bounds: Rectangle, state: &mut State<P>) {
        let max = self.max_scroll(bounds, state);
        state.scroll.x = state.scroll.x.clamp(0.0, max.x);
        state.scroll.y = state.scroll.y.clamp(0.0, max.y);
    }

    /// Scrolls so that the caret is visible.
    fn reveal_cursor<P: text::Paragraph<Font = Font>>(&self, bounds: Rectangle, state: &mut State<P>) {
        let view = self.viewport_size(bounds, state);
        let cursor = self.content.cursor();
        let cx = self.col_x(state, cursor.line, cursor.col);
        let cy = cursor.line as f32 * self.line_height;

        if cy < state.scroll.y {
            state.scroll.y = cy;
        } else if cy + self.line_height > state.scroll.y + view.height {
            state.scroll.y = cy + self.line_height - view.height;
        }

        let margin = 4.0 * state.char_width;
        if cx < state.scroll.x {
            state.scroll.x = (cx - margin).max(0.0);
        } else if cx + state.char_width > state.scroll.x + view.width {
            state.scroll.x = cx + state.char_width + margin - view.width;
        }
        self.clamp_scroll(bounds, state);
    }

    fn pos_at<P: text::Paragraph<Font = Font>>(&self, point: Point, bounds: Rectangle, state: &State<P>) -> Pos {
        let origin = self.text_origin(bounds, state);
        let y = point.y - origin.y + state.scroll.y;
        let line_idx = (y / self.line_height)
            .floor()
            .clamp(0.0, (self.content.line_count() - 1) as f32) as usize;
        let line = self.content.line(line_idx).unwrap_or("");
        let x = (point.x - origin.x + state.scroll.x).max(0.0);
        let col = if let Some(p) = state.paragraphs.get(line_idx) {
            if let Some(hit) = p.hit_test(Point::new(x, self.line_height * 0.5)) {
                let byte_offset = hit.cursor().min(line.len());
                line[..byte_offset].chars().count()
            } else if x <= 0.0 {
                0
            } else {
                char_len(line)
            }
        } else {
            (x / state.char_width).round().max(0.0) as usize
        };
        Pos {
            line: line_idx,
            col: col.min(char_len(line)),
        }
    }

    fn text_config(&self, content: String, width: f32, shaping: text::Shaping, hint: Option<f32>) -> Text<String, Font> {
        Text {
            content,
            bounds: Size::new(width, self.line_height),
            size: Pixels(self.size),
            line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: hint,
        }
    }
}

struct State<P> {
    scroll: Point,
    char_width: f32,
    paragraphs: Vec<P>,
    shaped_revision: Option<u64>,
    shaped_size: f32,
    shaped_font: Font,
    shaped_hint: Option<f32>,
    focused: bool,
    dragging: bool,
    modifiers: keyboard::Modifiers,
    now: Instant,
    updated_at: Instant,
    last_seen: Option<(Pos, u64)>,
    last_click: Option<(Instant, Pos)>,
    click_count: u8,
    pasting: bool,
}

impl<P> Default for State<P> {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            scroll: Point::ORIGIN,
            char_width: 8.0,
            paragraphs: Vec::new(),
            shaped_revision: None,
            shaped_size: 0.0,
            shaped_font: Font::DEFAULT,
            shaped_hint: None,
            focused: false,
            dragging: false,
            modifiers: keyboard::Modifiers::default(),
            now,
            updated_at: now,
            last_seen: None,
            last_click: None,
            click_count: 0,
            pasting: false,
        }
    }
}

impl<P> State<P> {
    fn caret_visible(&self) -> bool {
        let elapsed = self.now.saturating_duration_since(self.updated_at).as_millis();
        (elapsed / CARET_BLINK_INTERVAL.as_millis()) % 2 == 0
    }
}

impl<P> operation::Focusable for State<P> {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
        self.updated_at = Instant::now();
        self.now = self.updated_at;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

fn token_color(tok: Tok, base: Color) -> Color {
    match tok {
        Tok::Plain => base,
        Tok::Keyword => Color::from_rgb(0.78, 0.57, 0.92),
        Tok::Str => Color::from_rgb(0.60, 0.80, 0.50),
        Tok::Number => Color::from_rgb(0.95, 0.65, 0.40),
        Tok::Comment => Color::from_rgb(0.45, 0.50, 0.55),
    }
}

impl<'a, Message, R> Widget<Message, Theme, R> for SqlEditor<'a, Message>
where
    R: text::Renderer<Font = Font>,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State<R::Paragraph>>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::<R::Paragraph>::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &R, limits: &Limits) -> Node {
        let state = tree.state.downcast_mut::<State<R::Paragraph>>();
        let hint = renderer.hint_factor();
        let probe = Text {
            content: "M",
            bounds: Size::ZERO,
            size: Pixels(self.size),
            line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: hint,
        };
        let width = R::Paragraph::with_text(probe).min_width();
        if width > 0.0 {
            state.char_width = width;
        }
        self.sync_paragraphs::<R>(state, hint);
        Node::new(limits.resolve(self.width, self.height, Size::ZERO))
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &R,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State<R::Paragraph>>();
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &R,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<R::Paragraph>>();
        let bounds = layout.bounds();

        self.sync_paragraphs::<R>(state, renderer.hint_factor());

        // Keep the caret in view whenever the content or caret changed.
        let seen = (self.content.cursor(), self.content.revision());
        if state.last_seen != Some(seen) {
            state.last_seen = Some(seen);
            self.reveal_cursor(bounds, state);
            shell.request_redraw();
        }

        match event {
            Event::Window(window::Event::RedrawRequested(now)) => {
                state.now = *now;
                if state.focused {
                    let interval = CARET_BLINK_INTERVAL.as_millis();
                    let elapsed = now.saturating_duration_since(state.updated_at).as_millis();
                    let until_next = interval - elapsed % interval;
                    shell.request_redraw_at(*now + Duration::from_millis(until_next as u64));
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor.position_over(bounds) {
                    state.focused = true;
                    state.updated_at = Instant::now();
                    state.now = state.updated_at;
                    let pos = self.pos_at(point, bounds, state);

                    let now = Instant::now();
                    state.click_count = match state.last_click {
                        Some((at, p))
                            if now.duration_since(at) < MULTI_CLICK_INTERVAL && p == pos =>
                        {
                            state.click_count % 3 + 1
                        }
                        _ => 1,
                    };
                    state.last_click = Some((now, pos));

                    let action = match state.click_count {
                        2 => Action::SelectWord(pos),
                        3 => Action::SelectLine(pos),
                        _ => {
                            state.dragging = true;
                            Action::Click {
                                pos,
                                select: state.modifiers.shift(),
                            }
                        }
                    };
                    shell.publish((self.on_action)(action));
                    shell.request_redraw();
                    shell.capture_event();
                } else if !shell.is_event_captured() {
                    state.focused = false;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.dragging {
                    let pos = self.pos_at(*position, bounds, state);
                    if pos != self.content.cursor() {
                        shell.publish((self.on_action)(Action::Drag(pos)));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if !cursor.is_over(bounds) || shell.is_event_captured() {
                    return;
                }
                let (mut dx, mut dy) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => {
                        (-x * state.char_width * 4.0, -y * self.line_height * 3.0)
                    }
                    mouse::ScrollDelta::Pixels { x, y } => (-x, -y),
                };
                if state.modifiers.shift() && dx == 0.0 {
                    std::mem::swap(&mut dx, &mut dy);
                }
                state.scroll.x += dx;
                state.scroll.y += dy;
                self.clamp_scroll(bounds, state);
                shell.request_redraw();
                shell.capture_event();
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
            }
            Event::Clipboard(clipboard::Event::Read(result)) => {
                if state.focused
                    && state.pasting
                    && let Ok(content) = result
                    && let clipboard::Content::Text(text) = content.as_ref()
                {
                    state.pasting = false;
                    shell.publish((self.on_action)(Action::Paste(text.clone())));
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                text,
                modifiers,
                ..
            }) if state.focused => {
                use keyboard::Key;
                use keyboard::key::Named;

                state.modifiers = *modifiers;
                let ctrl = modifiers.command();
                let shift = modifiers.shift();
                let mv = |motion| Action::Move { motion, select: shift };
                let page = ((self.viewport_size(bounds, state).height / self.line_height) as isize
                    - 1)
                .max(1);

                let popup = self.content.completion().is_some();

                let action: Option<Action> = match key.as_ref() {
                    Key::Named(Named::ArrowDown) if popup => Some(Action::CompletionMove(1)),
                    Key::Named(Named::ArrowUp) if popup => Some(Action::CompletionMove(-1)),
                    Key::Named(Named::PageDown) if popup => Some(Action::CompletionMove(8)),
                    Key::Named(Named::PageUp) if popup => Some(Action::CompletionMove(-8)),
                    Key::Named(Named::Enter | Named::Tab) if popup && !ctrl => {
                        Some(Action::CompletionAccept)
                    }
                    Key::Named(Named::Escape) if popup => Some(Action::CompletionDismiss),
                    Key::Named(Named::Space) if ctrl => Some(Action::RequestCompletion),
                    Key::Named(Named::ArrowLeft) => {
                        Some(mv(if ctrl { Motion::WordLeft } else { Motion::Left }))
                    }
                    Key::Named(Named::ArrowRight) => {
                        Some(mv(if ctrl { Motion::WordRight } else { Motion::Right }))
                    }
                    Key::Named(Named::ArrowUp) => Some(mv(Motion::Lines(-1))),
                    Key::Named(Named::ArrowDown) => Some(mv(Motion::Lines(1))),
                    Key::Named(Named::PageUp) => Some(mv(Motion::Lines(-page))),
                    Key::Named(Named::PageDown) => Some(mv(Motion::Lines(page))),
                    Key::Named(Named::Home) => {
                        Some(mv(if ctrl { Motion::DocumentStart } else { Motion::Home }))
                    }
                    Key::Named(Named::End) => {
                        Some(mv(if ctrl { Motion::DocumentEnd } else { Motion::End }))
                    }
                    Key::Named(Named::Backspace) => Some(if ctrl {
                        Action::DeleteWord { backward: true }
                    } else {
                        Action::Backspace
                    }),
                    Key::Named(Named::Delete) => Some(if ctrl {
                        Action::DeleteWord { backward: false }
                    } else {
                        Action::Delete
                    }),
                    // Ctrl+Enter is reserved for running the query.
                    Key::Named(Named::Enter) if !ctrl => Some(Action::Enter),
                    Key::Named(Named::Tab) if !shift => Some(Action::Tab),
                    Key::Character(c) if ctrl && !modifiers.alt() => match c.to_lowercase().as_str()
                    {
                        "a" => Some(Action::SelectAll),
                        "z" if shift => Some(Action::Redo),
                        "z" => Some(Action::Undo),
                        "y" => Some(Action::Redo),
                        "c" => {
                            if let Some(selected) = self.content.selected_text() {
                                shell.write_clipboard(clipboard::Content::Text(selected));
                            }
                            shell.capture_event();
                            None
                        }
                        "x" => {
                            if let Some(selected) = self.content.selected_text() {
                                shell.write_clipboard(clipboard::Content::Text(selected));
                                Some(Action::Backspace)
                            } else {
                                None
                            }
                        }
                        "v" => {
                            state.pasting = true;
                            shell.read_clipboard(clipboard::Kind::Text);
                            shell.capture_event();
                            None
                        }
                        _ => None,
                    },
                    _ if ctrl && !modifiers.alt() => None,
                    _ => text
                        .as_ref()
                        .map(|t| t.chars().filter(|c| !c.is_control()).collect::<String>())
                        .filter(|t| !t.is_empty())
                        .map(Action::Type),
                };

                if let Some(action) = action {
                    state.updated_at = Instant::now();
                    state.now = state.updated_at;
                    shell.publish((self.on_action)(action));
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            _ => {}
        }
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
        let gutter = self.gutter_width(state.char_width);
        let text_area = Rectangle {
            x: bounds.x + gutter,
            width: (bounds.width - gutter).max(0.0),
            ..bounds
        };
        if cursor.is_over(text_area) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &R,
        _viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, R>> {
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let completion = self.content.completion()?;
        if !state.focused {
            return None;
        }
        let bounds = layout.bounds() + translation;
        let origin = self.text_origin(bounds, state);
        let cw = state.char_width;
        let cursor_line = self.content.cursor().line;
        let top = origin.y + cursor_line as f32 * self.line_height - state.scroll.y;
        let start_x = self.col_x(state, completion.start.line, completion.start.col);
        let x = origin.x + start_x - state.scroll.x;

        Some(overlay::Element::new(Box::new(CompletionOverlay {
            completion,
            on_action: &*self.on_action,
            anchor_x: x,
            line_top: top,
            line_height: self.line_height,
            char_width: cw,
            font: self.font,
            size: self.size,
        })))
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut R,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<R::Paragraph>>();
        let bounds = layout.bounds();
        let palette = theme.palette();
        let base_text = palette.background.base.text;
        let cw = state.char_width;
        let lh = self.line_height;
        let gutter_w = self.gutter_width(cw);
        let hint = renderer.hint_factor();
        let origin = self.text_origin(bounds, state);
        let view = self.viewport_size(bounds, state);

        let text_area = Rectangle {
            x: bounds.x + gutter_w,
            y: bounds.y,
            width: (bounds.width - gutter_w).max(0.0),
            height: bounds.height,
        };
        let gutter_area = Rectangle {
            width: gutter_w,
            ..bounds
        };

        let line_count = self.content.line_count();
        let first = ((state.scroll.y / lh).floor() as usize).min(line_count - 1);
        let last = (((state.scroll.y + bounds.height) / lh).ceil() as usize + 1).min(line_count);
        let selection = self.content.selection();
        let cursor_pos = self.content.cursor();

        renderer.with_layer(text_area, |renderer| {
            let selection_color = Color {
                a: 0.35,
                ..palette.primary.base.color
            };
            for index in first..last {
                let line = self.content.line(index).unwrap_or("");
                let y = origin.y + index as f32 * lh - state.scroll.y;
                let x0 = origin.x - state.scroll.x;
                let len = char_len(line);

                // Selection.
                if let Some((a, b)) = selection
                    && index >= a.line
                    && index <= b.line
                {
                    let start = if index == a.line { a.col } else { 0 };
                    let end = if index == b.line { b.col } else { len + 1 };
                    if end > start {
                        let start_x = self.col_x(state, index, start);
                        let end_x = self.col_x(state, index, end);
                        renderer.fill_quad(
                            Quad {
                                bounds: Rectangle {
                                    x: x0 + start_x,
                                    y,
                                    width: (end_x - start_x).max(0.0),
                                    height: lh,
                                },
                                ..Quad::default()
                            },
                            selection_color,
                        );
                    }
                }

                // Text paragraph for the line.
                if let Some(paragraph) = state.paragraphs.get(index)
                    && !line.is_empty()
                {
                    renderer.fill_paragraph(paragraph, Point::new(x0, y), base_text, text_area);
                }

                // Caret.
                if state.focused && index == cursor_pos.line && state.caret_visible() {
                    let caret_x = self.col_x(state, index, cursor_pos.col);
                    renderer.fill_quad(
                        Quad {
                            bounds: Rectangle {
                                x: x0 + caret_x,
                                y,
                                width: 1.5,
                                height: lh,
                            },
                            ..Quad::default()
                        },
                        base_text,
                    );
                }
            }
        });

        // Gutter.
        renderer.with_layer(gutter_area, |renderer| {
            renderer.fill_quad(
                Quad {
                    bounds: gutter_area,
                    ..Quad::default()
                },
                Color {
                    a: 0.5,
                    ..palette.background.weak.color
                },
            );
            for index in first..last {
                let y = origin.y + index as f32 * lh - state.scroll.y;
                let label = (index + 1).to_string();
                let x = gutter_area.x + gutter_w - GUTTER_PAD - label.len() as f32 * cw;
                let color = if index == cursor_pos.line {
                    base_text
                } else {
                    Color {
                        a: 0.45,
                        ..base_text
                    }
                };
                renderer.fill_text(
                    self.text_config(label, gutter_w, text::Shaping::Basic, hint),
                    Point::new(x, y),
                    color,
                    gutter_area,
                );
            }
        });

        // Vertical scrollbar.
        let total_h = line_count as f32 * lh;
        if total_h > view.height {
            let track_h = bounds.height;
            let thumb_h = (view.height / total_h * track_h).max(24.0).min(track_h);
            let max_scroll = (total_h - view.height).max(1.0);
            let thumb_y = bounds.y + (state.scroll.y / max_scroll) * (track_h - thumb_h);
            renderer.with_layer(bounds, |renderer| {
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle {
                            x: bounds.x + bounds.width - SCROLLBAR_WIDTH,
                            y: thumb_y,
                            width: SCROLLBAR_WIDTH,
                            height: thumb_h,
                        },
                        border: iced::Border {
                            radius: (SCROLLBAR_WIDTH / 2.0).into(),
                            ..Default::default()
                        },
                        ..Quad::default()
                    },
                    Color {
                        a: 0.25,
                        ..base_text
                    },
                );
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Completion popup (overlay)
// ---------------------------------------------------------------------------

struct CompletionOverlay<'a, Message> {
    completion: &'a Completion,
    on_action: &'a dyn Fn(Action) -> Message,
    anchor_x: f32,
    line_top: f32,
    line_height: f32,
    char_width: f32,
    font: Font,
    size: f32,
}

const POPUP_PAD: f32 = 4.0;

impl<'a, Message> CompletionOverlay<'a, Message> {
    fn row_height(&self) -> f32 {
        self.line_height + 4.0
    }

    fn visible(&self) -> usize {
        self.completion.items.len().min(MAX_VISIBLE_ITEMS)
    }

    fn first(&self) -> usize {
        popup_first(self.completion.selected, self.completion.items.len())
    }

    fn text(&self, content: String, width: f32, hint: Option<f32>) -> Text<String, Font> {
        Text {
            content,
            bounds: Size::new(width, self.line_height),
            size: Pixels(self.size),
            line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
            font: self.font,
            align_x: text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
            ellipsis: text::Ellipsis::None,
            hint_factor: hint,
        }
    }
}

impl<'a, Message, R> overlay::Overlay<Message, Theme, R> for CompletionOverlay<'a, Message>
where
    R: text::Renderer<Font = Font>,
{
    fn layout(&mut self, _renderer: &R, bounds: Size) -> Node {
        let cw = self.char_width;
        let first = self.first();
        let window = &self.completion.items[first..first + self.visible()];
        let label_cols = window.iter().map(|i| char_len(&i.label)).max().unwrap_or(0);
        let detail_cols = window.iter().map(|i| char_len(&i.detail)).max().unwrap_or(0);
        let width = ((3 + label_cols + 2 + detail_cols) as f32 * cw + 2.0 * POPUP_PAD)
            .clamp(180.0, 560.0)
            .min(bounds.width);
        let height = self.visible() as f32 * self.row_height() + 2.0 * POPUP_PAD;

        let x = self.anchor_x.min(bounds.width - width).max(0.0);
        let below = self.line_top + self.line_height;
        let y = if below + height > bounds.height {
            (self.line_top - height).max(0.0)
        } else {
            below
        };
        Node::new(Size::new(width, height)).move_to(Point::new(x, y))
    }

    fn draw(
        &self,
        renderer: &mut R,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        let bounds = layout.bounds();
        let palette = theme.palette();
        let text_color = palette.background.base.text;
        let hint = renderer.hint_factor();
        let cw = self.char_width;
        let row_h = self.row_height();
        let first = self.first();

        renderer.fill_quad(
            Quad {
                bounds,
                border: iced::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..Quad::default()
            },
            palette.background.weak.color,
        );

        for (row, index) in (first..first + self.visible()).enumerate() {
            let item = &self.completion.items[index];
            let y = bounds.y + POPUP_PAD + row as f32 * row_h;
            let row_bounds = Rectangle {
                x: bounds.x + 2.0,
                y,
                width: bounds.width - 4.0,
                height: row_h,
            };
            if index == self.completion.selected {
                renderer.fill_quad(
                    Quad {
                        bounds: row_bounds,
                        border: iced::Border {
                            radius: 3.0.into(),
                            ..Default::default()
                        },
                        ..Quad::default()
                    },
                    Color {
                        a: 0.45,
                        ..palette.primary.base.color
                    },
                );
            }
            let ty = y + 2.0;
            let mut put = |content: String, x: f32, color: Color| {
                renderer.fill_text(
                    self.text(content, bounds.width, hint),
                    Point::new(x, ty),
                    color,
                    row_bounds,
                );
            };
            put(
                item.kind.badge().to_string(),
                bounds.x + POPUP_PAD + 2.0,
                item.kind.color(),
            );
            put(
                item.label.clone(),
                bounds.x + POPUP_PAD + 3.0 * cw,
                text_color,
            );
            if !item.detail.is_empty() {
                let dx = bounds.x + bounds.width
                    - POPUP_PAD
                    - (char_len(&item.detail) as f32 * cw)
                    - 4.0;
                let min_x = bounds.x + POPUP_PAD + (4 + char_len(&item.label)) as f32 * cw;
                put(
                    item.detail.clone(),
                    dx.max(min_x),
                    Color {
                        a: 0.5,
                        ..text_color
                    },
                );
            }
        }
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &R,
        shell: &mut Shell<'_, Message>,
    ) {
        let bounds = layout.bounds();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor.position_over(bounds) {
                    let row = ((point.y - bounds.y - POPUP_PAD) / self.row_height())
                        .floor()
                        .max(0.0) as usize;
                    let index = self.first() + row.min(self.visible() - 1);
                    shell.publish((self.on_action)(Action::CompletionPick(index)));
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => *y,
                    mouse::ScrollDelta::Pixels { y, .. } => *y / self.row_height(),
                };
                let step = if y > 0.0 { -1 } else { 1 };
                shell.publish((self.on_action)(Action::CompletionMove(step)));
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &R,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

impl<'a, Message, R> From<SqlEditor<'a, Message>> for Element<'a, Message, Theme, R>
where
    Message: 'a,
    R: text::Renderer<Font = Font> + 'a,
{
    fn from(editor: SqlEditor<'a, Message>) -> Self {
        Element::new(editor)
    }
}
