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
struct Line {
    text: String,
    rev: u64,
}

impl Line {
    fn new(text: String) -> Self {
        Self {
            text,
            rev: next_revision(),
        }
    }
}

#[derive(Debug, Clone)]
struct UndoEntry {
    start_line: usize,
    new_line_count: usize,
    old_lines: Vec<String>,
    cursor_before: Pos,
    cursor_after: Pos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Other,
}

#[derive(Debug, Clone)]
pub struct Content {
    lines: Vec<Line>,
    cursor: Pos,
    anchor: Option<Pos>,
    goal_col: Option<usize>,
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
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
            lines: split_lines(text).into_iter().map(Line::new).collect(),
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
        let (a, b) = if completion.start <= self.cursor {
            (completion.start, self.cursor)
        } else {
            (self.cursor, completion.start)
        };
        self.replace_range(a, b, &insert, EditKind::Other);
    }

    pub fn text(&self) -> String {
        let total_len: usize = self
            .lines
            .iter()
            .map(|l| l.text.len())
            .sum::<usize>()
            + self.lines.len().saturating_sub(1);
        let mut out = String::with_capacity(total_len);
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&line.text);
        }
        out
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|l| l.text.as_str())
    }

    pub fn line_revision(&self, index: usize) -> u64 {
        self.lines.get(index).map_or(0, |l| l.rev)
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
            let line = &self.lines[a.line].text;
            return Some(line[byte_idx(line, a.col)..byte_idx(line, b.col)].to_string());
        }
        let mut out = String::new();
        let first = &self.lines[a.line].text;
        out.push_str(&first[byte_idx(first, a.col)..]);
        for line in &self.lines[a.line + 1..b.line] {
            out.push('\n');
            out.push_str(&line.text);
        }
        out.push('\n');
        let last = &self.lines[b.line].text;
        out.push_str(&last[..byte_idx(last, b.col)]);
        Some(out)
    }

    fn clamp(&self, pos: Pos) -> Pos {
        let line = pos.line.min(self.lines.len() - 1);
        Pos {
            line,
            col: pos.col.min(char_len(&self.lines[line].text)),
        }
    }

    fn line_len(&self, line: usize) -> usize {
        char_len(&self.lines[line].text)
    }

    /// Replaces the ordered range `a..b` with `text`, recording a minimal line-delta undo entry.
    fn replace_range(&mut self, a: Pos, b: Pos, text: &str, kind: EditKind) {
        let cursor_before = self.cursor;
        let single_line_input = !text.contains(['\n', '\r', '\t']);
        let is_single_line_edit = a.line == b.line && single_line_input;

        let coalesce = kind == EditKind::Typing
            && self.last_edit == Some(EditKind::Typing)
            && is_single_line_edit
            && a == b
            && self
                .undo
                .last()
                .is_some_and(|u| u.start_line == a.line && u.new_line_count == 1);

        let old_lines = if coalesce {
            Vec::new()
        } else {
            self.lines[a.line..=b.line]
                .iter()
                .map(|l| l.text.clone())
                .collect()
        };

        let new_line_count = if is_single_line_edit {
            let line = &mut self.lines[a.line];
            let start_b = byte_idx(&line.text, a.col);
            let end_b = if a.col == b.col {
                start_b
            } else {
                byte_idx(&line.text, b.col)
            };
            if start_b != end_b {
                line.text.replace_range(start_b..end_b, text);
            } else if !text.is_empty() {
                line.text.insert_str(start_b, text);
            }
            line.rev = next_revision();
            self.cursor = Pos {
                line: a.line,
                col: a.col + char_len(text),
            };
            1
        } else {
            let parts = split_lines(text);
            let head = {
                let line = &self.lines[a.line].text;
                line[..byte_idx(line, a.col)].to_string()
            };
            let tail = {
                let line = &self.lines[b.line].text;
                line[byte_idx(line, b.col)..].to_string()
            };
            if parts.len() == 1 {
                let merged = format!("{head}{}{tail}", parts[0]);
                self.lines.splice(a.line..=b.line, [Line::new(merged)]);
                self.cursor = Pos {
                    line: a.line,
                    col: a.col + char_len(&parts[0]),
                };
                1
            } else {
                let last_index = parts.len() - 1;
                let mut new_lines = Vec::with_capacity(parts.len());
                new_lines.push(Line::new(format!("{head}{}", parts[0])));
                new_lines.extend(parts[1..last_index].iter().cloned().map(Line::new));
                new_lines.push(Line::new(format!("{}{tail}", parts[last_index])));
                self.cursor = Pos {
                    line: a.line + last_index,
                    col: char_len(&parts[last_index]),
                };
                let count = new_lines.len();
                self.lines.splice(a.line..=b.line, new_lines);
                count
            }
        };

        if coalesce {
            if let Some(last) = self.undo.last_mut() {
                last.cursor_after = self.cursor;
            }
        } else {
            self.undo.push(UndoEntry {
                start_line: a.line,
                new_line_count,
                old_lines,
                cursor_before,
                cursor_after: self.cursor,
            });
            if self.undo.len() > UNDO_LIMIT {
                self.undo.remove(0);
            }
        }

        self.redo.clear();
        self.anchor = None;
        self.last_edit = Some(kind);
        self.goal_col = None;
        self.revision = next_revision();
    }

    fn apply_undo_entry(&mut self, entry: UndoEntry) -> UndoEntry {
        let end = entry.start_line + entry.new_line_count;
        let current_lines: Vec<String> = self.lines[entry.start_line..end]
            .iter()
            .map(|l| l.text.clone())
            .collect();
        let restored_count = entry.old_lines.len();
        let restored: Vec<Line> = entry.old_lines.into_iter().map(Line::new).collect();
        self.lines.splice(entry.start_line..end, restored);
        self.cursor = entry.cursor_before;
        self.anchor = None;
        self.goal_col = None;
        self.last_edit = None;
        self.revision = next_revision();
        UndoEntry {
            start_line: entry.start_line,
            new_line_count: restored_count,
            old_lines: current_lines,
            cursor_before: entry.cursor_after,
            cursor_after: entry.cursor_before,
        }
    }

    fn insert_with_kind(&mut self, text: &str, kind: EditKind) {
        let (a, b) = self.selection().unwrap_or((self.cursor, self.cursor));
        self.replace_range(a, b, text, kind);
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
        let chars: Vec<char> = self.lines[from.line].text.chars().collect();
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
        let chars: Vec<char> = self.lines[from.line].text.chars().collect();
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
                    .text
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
        let chars: Vec<char> = self.lines[pos.line].text.chars().collect();
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
                self.insert_with_kind(&text, EditKind::Typing);
            }
            Action::Paste(text) => {
                self.insert_with_kind(&text, EditKind::Other);
            }
            Action::Enter => {
                let (a, b) = self.selection().unwrap_or((self.cursor, self.cursor));
                let indent: String = self.lines[a.line]
                    .text
                    .chars()
                    .take(a.col)
                    .take_while(|c| *c == ' ')
                    .collect();
                self.replace_range(a, b, &format!("\n{indent}"), EditKind::Other);
            }
            Action::Tab => {
                self.insert_with_kind(&" ".repeat(TAB_WIDTH), EditKind::Other);
            }
            Action::Backspace => {
                if let Some((a, b)) = self.selection() {
                    self.replace_range(a, b, "", EditKind::Other);
                } else {
                    self.anchor = None;
                    let c = self.cursor;
                    if c.col > 0 {
                        self.replace_range(Pos { col: c.col - 1, ..c }, c, "", EditKind::Other);
                    } else if c.line > 0 {
                        let start = Pos {
                            line: c.line - 1,
                            col: self.line_len(c.line - 1),
                        };
                        self.replace_range(start, c, "", EditKind::Other);
                    }
                }
            }
            Action::Delete => {
                if let Some((a, b)) = self.selection() {
                    self.replace_range(a, b, "", EditKind::Other);
                } else {
                    self.anchor = None;
                    let c = self.cursor;
                    if c.col < self.line_len(c.line) {
                        self.replace_range(c, Pos { col: c.col + 1, ..c }, "", EditKind::Other);
                    } else if c.line + 1 < self.lines.len() {
                        self.replace_range(
                            c,
                            Pos {
                                line: c.line + 1,
                                col: 0,
                            },
                            "",
                            EditKind::Other,
                        );
                    }
                }
            }
            Action::DeleteWord { backward } => {
                if let Some((a, b)) = self.selection() {
                    self.replace_range(a, b, "", EditKind::Other);
                } else {
                    self.anchor = None;
                    let c = self.cursor;
                    if backward {
                        let target = self.word_left(c);
                        if target < c {
                            self.replace_range(target, c, "", EditKind::Other);
                        }
                    } else {
                        let target = self.word_right(c);
                        if c < target {
                            self.replace_range(c, target, "", EditKind::Other);
                        }
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
                if let Some(entry) = self.undo.pop() {
                    let inverse = self.apply_undo_entry(entry);
                    self.redo.push(inverse);
                }
            }
            Action::Redo => {
                if let Some(entry) = self.redo.pop() {
                    let inverse = self.apply_undo_entry(entry);
                    self.undo.push(inverse);
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
    let bytes = word.as_bytes();
    if bytes.len() > 16 || !word.is_ascii() {
        return false;
    }
    let mut buf = [0u8; 16];
    for (dst, &src) in buf.iter_mut().zip(bytes) {
        *dst = src.to_ascii_lowercase();
    }
    let Ok(lower) = std::str::from_utf8(&buf[..bytes.len()]) else {
        return false;
    };
    matches!(
        lower,
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

/// Advances `in_block` (`/* ... */`) across `line` without allocating.
fn scan_block_comment(line: &str, in_block: &mut bool) {
    let b = line.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        if *in_block {
            if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                *in_block = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        match b[i] {
            b'-' if i + 1 < n && b[i + 1] == b'-' => break,
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                *in_block = true;
                i += 2;
            }
            b'\'' => {
                i += 1;
                while i < n {
                    if b[i] == b'\'' {
                        if i + 1 < n && b[i + 1] == b'\'' {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
}

/// Tokenizes one line. `in_block` carries `/* ... */` state across lines.
/// Returned ranges are UTF-8 byte offsets into `line`.
fn tokenize(line: &str, in_block: &mut bool) -> Vec<(usize, usize, Tok)> {
    let b = line.as_bytes();
    let n = b.len();
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

    let mut i = 0;
    while i < n {
        if *in_block {
            let mut j = i;
            let mut closed = false;
            while j < n {
                if b[j] == b'*' && j + 1 < n && b[j + 1] == b'/' {
                    j += 2;
                    closed = true;
                    break;
                }
                j += 1;
            }
            push_tok(&mut out, i, j, Tok::Comment);
            if closed {
                *in_block = false;
            }
            i = j;
            continue;
        }
        let byte = b[i];
        if byte == b'-' && i + 1 < n && b[i + 1] == b'-' {
            push_tok(&mut out, i, n, Tok::Comment);
            break;
        } else if byte == b'/' && i + 1 < n && b[i + 1] == b'*' {
            *in_block = true;
            push_tok(&mut out, i, i + 2, Tok::Comment);
            i += 2;
        } else if byte == b'\'' {
            let mut j = i + 1;
            while j < n {
                if b[j] == b'\'' {
                    if j + 1 < n && b[j + 1] == b'\'' {
                        j += 2;
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            push_tok(&mut out, i, j, Tok::Str);
            i = j;
        } else if byte.is_ascii_digit() {
            let mut j = i;
            while j < n && (b[j].is_ascii_digit() || b[j] == b'.') {
                j += 1;
            }
            push_tok(&mut out, i, j, Tok::Number);
            i = j;
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let mut j = i + 1;
            while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            let word = &line[i..j];
            push_tok(
                &mut out,
                i,
                j,
                if is_keyword(word) { Tok::Keyword } else { Tok::Plain },
            );
            i = j;
        } else {
            let step = if byte < 0x80 {
                1
            } else {
                line[i..].chars().next().map_or(1, char::len_utf8)
            };
            push_tok(&mut out, i, i + step, Tok::Plain);
            i += step;
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

    fn visible_range(&self, scroll_y: f32, viewport_h: f32) -> (usize, usize) {
        let line_count = self.content.line_count();
        let first = ((scroll_y / self.line_height).floor().max(0.0) as usize).min(line_count - 1);
        let last = (((scroll_y + viewport_h) / self.line_height).ceil().max(0.0) as usize + 1)
            .min(line_count);
        (first, last)
    }

    fn shape_line<R>(&self, line: &str, in_block: bool, hint: Option<f32>) -> R::Paragraph
    where
        R: text::Renderer<Font = Font>,
    {
        if line.is_empty() {
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

        let mut block = in_block;
        let tokens = tokenize(line, &mut block);
        if tokens.len() == 1 && tokens[0].2 == Tok::Plain && line.is_ascii() {
            return R::Paragraph::with_text(Text {
                content: line,
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

        let spans: Vec<text::Span<'_, (), Font>> = tokens
            .into_iter()
            .map(|(s, e, tok)| {
                let mut span = text::Span::new(&line[s..e]);
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

    fn sync_metrics<R>(&self, state: &mut State<R::Paragraph>, hint: Option<f32>)
    where
        R: text::Renderer<Font = Font>,
    {
        if state.shaped_size == self.size
            && state.shaped_font == self.font
            && state.shaped_hint == hint
        {
            return;
        }
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
        state.paragraphs.clear();
        state.gutter_cache = None;
        state.max_line_width = 0.0;
        state.block_states_rev = None;
        state.shaped_size = self.size;
        state.shaped_font = self.font;
        state.shaped_hint = hint;
    }

    fn sync_block_states<P>(&self, state: &mut State<P>) {
        let rev = self.content.revision();
        let line_count = self.content.line_count();
        if state.block_states_rev == Some(rev) && state.block_states.len() == line_count {
            return;
        }

        state.block_states.clear();
        state.block_states.reserve(line_count);
        let mut in_block = false;
        let mut max_chars = 0usize;
        for i in 0..line_count {
            state.block_states.push(in_block);
            let line = self.content.line(i).unwrap_or("");
            max_chars = max_chars.max(line.len());
            scan_block_comment(line, &mut in_block);
        }
        state.paragraphs.resize_with(line_count, || None);
        state.max_line_width = max_chars as f32 * state.char_width;
        state.block_states_rev = Some(rev);
    }

    fn ensure_line_shaped<R>(
        &self,
        state: &mut State<R::Paragraph>,
        index: usize,
        hint: Option<f32>,
    ) where
        R: text::Renderer<Font = Font>,
    {
        if index >= state.paragraphs.len() {
            return;
        }
        let line_rev = self.content.line_revision(index);
        let in_block = state.block_states.get(index).copied().unwrap_or(false);
        if let Some(cached) = &state.paragraphs[index]
            && cached.rev == line_rev
            && cached.in_block == in_block
        {
            return;
        }
        let line = self.content.line(index).unwrap_or("");
        let paragraph = self.shape_line::<R>(line, in_block, hint);
        state.max_line_width = state.max_line_width.max(paragraph.min_width());
        state.paragraphs[index] = Some(CachedLine {
            paragraph,
            rev: line_rev,
            in_block,
        });
    }

    fn sync_visible<R>(&self, state: &mut State<R::Paragraph>, bounds_h: f32, hint: Option<f32>)
    where
        R: text::Renderer<Font = Font>,
    {
        self.sync_metrics::<R>(state, hint);
        self.sync_block_states(state);

        let line_count = self.content.line_count();
        let (first, last) = self.visible_range(state.scroll.y, bounds_h);
        let buf_start = first.saturating_sub(4);
        let buf_end = (last + 4).min(line_count);
        for index in buf_start..buf_end {
            self.ensure_line_shaped::<R>(state, index, hint);
        }
        let cursor_line = self.content.cursor().line;
        self.ensure_line_shaped::<R>(state, cursor_line, hint);

        let gutter_w = self.gutter_width(state.char_width);
        let key = (first, last, cursor_line, gutter_w.to_bits());
        let needs_gutter = state
            .gutter_cache
            .as_ref()
            .is_none_or(|g| g.key != key);
        if needs_gutter {
            let mut muted_text = String::with_capacity((last - first) * 6);
            for index in first..last {
                if index > first {
                    muted_text.push('\n');
                }
                if index != cursor_line {
                    use std::fmt::Write;
                    let _ = write!(&mut muted_text, "{}", index + 1);
                }
            }
            let text_w = (gutter_w - GUTTER_PAD).max(1.0);
            let muted = R::Paragraph::with_text(Text {
                content: &muted_text,
                bounds: Size::new(text_w, f32::INFINITY),
                size: Pixels(self.size),
                line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
                font: self.font,
                align_x: text::Alignment::Right,
                align_y: alignment::Vertical::Top,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
                ellipsis: text::Ellipsis::None,
                hint_factor: hint,
            });
            let active = if cursor_line >= first && cursor_line < last {
                let label = (cursor_line + 1).to_string();
                Some(R::Paragraph::with_text(Text {
                    content: &label,
                    bounds: Size::new(text_w, self.line_height),
                    size: Pixels(self.size),
                    line_height: text::LineHeight::Absolute(Pixels(self.line_height)),
                    font: self.font,
                    align_x: text::Alignment::Right,
                    align_y: alignment::Vertical::Top,
                    shaping: text::Shaping::Basic,
                    wrapping: text::Wrapping::None,
                    ellipsis: text::Ellipsis::None,
                    hint_factor: hint,
                }))
            } else {
                None
            };
            state.gutter_cache = Some(GutterCache { key, muted, active });
        }
    }

    fn col_x<P: text::Paragraph<Font = Font>>(&self, state: &State<P>, line_idx: usize, col: usize) -> f32 {
        if col == 0 {
            return 0.0;
        }
        let line = self.content.line(line_idx).unwrap_or("");
        let len = char_len(line);
        let clamped_col = col.min(len);
        let x = if clamped_col == 0 {
            0.0
        } else if let Some(Some(cached)) = state.paragraphs.get(line_idx) {
            if clamped_col == len {
                cached.paragraph.min_width()
            } else {
                cached
                    .paragraph
                    .grapheme_position(0, clamped_col)
                    .map_or(clamped_col as f32 * state.char_width, |p| p.x)
            }
        } else {
            clamped_col as f32 * state.char_width
        };
        if col > len {
            x + (col - len) as f32 * state.char_width
        } else {
            x
        }
    }

    fn max_scroll<P: text::Paragraph>(&self, bounds: Rectangle, state: &State<P>) -> Point {
        let view = self.viewport_size(bounds, state);
        let total_h = self.content.line_count() as f32 * self.line_height;
        let total_w = state.max_line_width + 4.0 * state.char_width;
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
        let col = if let Some(Some(cached)) = state.paragraphs.get(line_idx) {
            if let Some(hit) = cached.paragraph.hit_test(Point::new(x, self.line_height * 0.5)) {
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
}

struct CachedLine<P> {
    paragraph: P,
    rev: u64,
    in_block: bool,
}

struct GutterCache<P> {
    key: (usize, usize, usize, u32),
    muted: P,
    active: Option<P>,
}

struct State<P> {
    scroll: Point,
    char_width: f32,
    paragraphs: Vec<Option<CachedLine<P>>>,
    block_states: Vec<bool>,
    block_states_rev: Option<u64>,
    max_line_width: f32,
    gutter_cache: Option<GutterCache<P>>,
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
            block_states: Vec::new(),
            block_states_rev: None,
            max_line_width: 0.0,
            gutter_cache: None,
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
        let resolved = limits.resolve(self.width, self.height, Size::ZERO);
        self.sync_visible::<R>(state, resolved.height, renderer.hint_factor());
        Node::new(resolved)
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
        let hint = renderer.hint_factor();

        self.sync_visible::<R>(state, bounds.height, hint);

        // Keep the caret in view whenever the content or caret changed.
        let seen = (self.content.cursor(), self.content.revision());
        if state.last_seen != Some(seen) {
            state.last_seen = Some(seen);
            let old_scroll = state.scroll;
            self.reveal_cursor(bounds, state);
            if state.scroll != old_scroll {
                self.sync_visible::<R>(state, bounds.height, hint);
            }
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
                self.sync_visible::<R>(state, bounds.height, hint);
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
        let (first, last) = self.visible_range(state.scroll.y, bounds.height);
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
                if let Some(Some(cached)) = state.paragraphs.get(index)
                    && !line.is_empty()
                {
                    renderer.fill_paragraph(
                        &cached.paragraph,
                        Point::new(x0, y),
                        base_text,
                        text_area,
                    );
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
            if let Some(gutter) = &state.gutter_cache {
                let (g_first, _, g_cursor, _) = gutter.key;
                let right_edge = gutter_area.x + gutter_w - GUTTER_PAD;
                let muted_color = Color {
                    a: 0.45,
                    ..base_text
                };
                renderer.fill_paragraph(
                    &gutter.muted,
                    Point::new(
                        right_edge - gutter.muted.min_width(),
                        origin.y + g_first as f32 * lh - state.scroll.y,
                    ),
                    muted_color,
                    gutter_area,
                );
                if let Some(active) = &gutter.active {
                    renderer.fill_paragraph(
                        active,
                        Point::new(
                            right_edge - active.min_width(),
                            origin.y + g_cursor as f32 * lh - state.scroll.y,
                        ),
                        base_text,
                        gutter_area,
                    );
                }
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
