//! Context-aware SQL suggestions (keywords, schemas, tables, columns).
//!
//! This is a lightweight token scan, not a full parser: it resolves `FROM` /
//! `JOIN` / `UPDATE` / `INTO` table references (with aliases) in the current
//! statement and decides whether to offer tables or columns.

use crate::types::{Schema, SchemaInfo, TableInfo};
use crate::widgets::sql_editor::{Completion, CompletionItem, Content, ItemKind, Pos};

const MAX_ITEMS: usize = 200;

const KEYWORDS: &[&str] = &[
    "ADD", "ALL", "ALTER", "ANALYZE", "AND", "ANY", "AS", "ASC", "BEGIN", "BETWEEN", "BY",
    "CASE", "CAST", "CHECK", "COLUMN", "COMMIT", "CONSTRAINT", "CREATE", "CROSS", "DATABASE",
    "DEFAULT", "DELETE", "DESC", "DISTINCT", "DROP", "ELSE", "END", "EXCEPT", "EXISTS",
    "EXPLAIN", "FALSE", "FOREIGN", "FROM", "FULL", "GROUP", "HAVING", "ILIKE", "IN", "INDEX",
    "INNER", "INSERT", "INTERSECT", "INTO", "IS", "JOIN", "KEY", "LATERAL", "LEFT", "LIKE",
    "LIMIT", "NATURAL", "NOT", "NULL", "OFFSET", "ON", "OR", "ORDER", "OUTER", "OVER",
    "PARTITION", "PRIMARY", "RECURSIVE", "REFERENCES", "RETURNING", "RIGHT", "ROLLBACK",
    "SCHEMA", "SELECT", "SET", "TABLE", "THEN", "TRUE", "TRUNCATE", "UNION", "UNIQUE",
    "UPDATE", "USING", "VALUES", "VIEW", "WHEN", "WHERE", "WITH",
];

const FUNCTIONS: &[&str] = &[
    "avg", "coalesce", "concat", "count", "current_date", "date_trunc", "extract", "greatest",
    "least", "length", "lower", "max", "min", "now", "nullif", "round", "row_number", "substring",
    "sum", "to_char", "trim", "upper",
];

/// Keywords after which a table name is expected.
const TABLE_KWS: &[&str] = &["from", "join", "into", "update", "table", "truncate"];
/// Keywords after which an expression / column is expected.
const COLUMN_KWS: &[&str] = &[
    "select", "where", "on", "by", "having", "set", "returning", "values", "and", "or", "using",
    "when", "then", "case", "else",
];

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_keyword(lower: &str) -> bool {
    KEYWORDS.iter().any(|k| k.eq_ignore_ascii_case(lower))
}

fn starts_with_ci(haystack: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    if haystack.is_ascii() && prefix.is_ascii() {
        return haystack.len() >= prefix.len()
            && haystack.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes());
    }
    let mut h = haystack.chars().flat_map(char::to_lowercase);
    for p in prefix.chars().flat_map(char::to_lowercase) {
        if h.next() != Some(p) {
            return false;
        }
    }
    true
}

fn quote_ident(name: &str) -> String {
    let simple = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if simple {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
}

/// Splits SQL into tokens in a single pass. Qualified names (`a.b`) stay a
/// single token, quotes are stripped, strings collapse to `'`, and comments
/// are dropped. Also returns the number of tokens that started before
/// `split_at` (a char index into `chars`).
fn tokenize_with_split(chars: &[char], split_at: usize) -> (Vec<String>, usize) {
    let n = chars.len();
    let mut out = Vec::new();
    let mut before_count = 0;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c.is_whitespace() {
            i += 1;
        } else if c == '-' && next == Some('-') {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c == '\'' {
            let start = i;
            i += 1;
            while i < n && chars[i] != '\'' {
                i += 1;
            }
            i += 1;
            out.push("'".to_string());
            if start < split_at {
                before_count = out.len();
            }
        } else if is_word(c) || c == '"' {
            let start = i;
            let mut tok = String::new();
            loop {
                if i < n && chars[i] == '"' {
                    i += 1;
                    while i < n && chars[i] != '"' {
                        tok.push(chars[i]);
                        i += 1;
                    }
                    i += 1;
                } else {
                    while i < n && is_word(chars[i]) {
                        tok.push(chars[i]);
                        i += 1;
                    }
                }
                if i + 1 < n && chars[i] == '.' && (is_word(chars[i + 1]) || chars[i + 1] == '"')
                {
                    tok.push('.');
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(tok);
            if start < split_at {
                before_count = out.len();
            }
        } else {
            let start = i;
            out.push(c.to_string());
            if start < split_at {
                before_count = out.len();
            }
            i += 1;
        }
    }
    (out, before_count)
}

fn is_ident(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
}

#[derive(Debug, Clone)]
struct TableRef {
    schema: Option<String>,
    name: String,
    alias: Option<String>,
}

fn table_refs(tokens: &[String]) -> Vec<TableRef> {
    let lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
    let mut refs = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if !matches!(lower[i].as_str(), "from" | "join" | "update" | "into") {
            i += 1;
            continue;
        }
        let from_like = lower[i] == "from";
        i += 1;
        loop {
            if i >= tokens.len() || !is_ident(&tokens[i]) || is_keyword(&lower[i]) {
                break;
            }
            let full = tokens[i].clone();
            i += 1;
            if i < tokens.len() && lower[i] == "as" {
                i += 1;
            }
            let mut alias = None;
            if i < tokens.len() && is_ident(&tokens[i]) && !is_keyword(&lower[i]) {
                alias = Some(tokens[i].clone());
                i += 1;
            }
            let (schema, name) = match full.split_once('.') {
                Some((s, n)) => (Some(s.to_string()), n.to_string()),
                None => (None, full),
            };
            refs.push(TableRef {
                schema,
                name,
                alias,
            });
            if from_like && i < tokens.len() && tokens[i] == "," {
                i += 1;
                continue;
            }
            break;
        }
    }
    refs
}

fn find_table<'a>(
    schema: &'a Schema,
    schema_name: Option<&str>,
    name: &str,
) -> Option<(&'a SchemaInfo, &'a TableInfo)> {
    let mut found = None;
    for s in &schema.schemas {
        if let Some(wanted) = schema_name
            && !s.name.eq_ignore_ascii_case(wanted)
        {
            continue;
        }
        if let Some(t) = s.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
            if s.name == "public" {
                return Some((s, t));
            }
            found.get_or_insert((s, t));
        }
    }
    found
}

fn column_items(
    schema_name: &str,
    table: &TableInfo,
    with_table: bool,
    prefix: &str,
) -> Vec<CompletionItem> {
    let _ = schema_name;
    table
        .columns
        .iter()
        .filter(|c| starts_with_ci(&c.name, prefix))
        .map(|c| CompletionItem {
            label: c.name.clone(),
            insert: quote_ident(&c.name),
            detail: {
                let key = if c.is_primary_key { " · pk" } else { "" };
                if with_table {
                    format!("{} · {}{key}", table.name, c.data_type)
                } else {
                    format!("{}{key}", c.data_type)
                }
            },
            kind: ItemKind::Column,
        })
        .collect()
}

fn table_items(schema: &Schema, prefix: &str) -> Vec<CompletionItem> {
    let mut public = Vec::new();
    let mut other = Vec::new();
    let mut views = Vec::new();
    let mut schemas = Vec::new();
    for s in &schema.schemas {
        if starts_with_ci(&s.name, prefix) {
            schemas.push(CompletionItem {
                label: s.name.clone(),
                insert: quote_ident(&s.name),
                detail: "schema".into(),
                kind: ItemKind::Schema,
            });
        }
        for t in &s.tables {
            if !starts_with_ci(&t.name, prefix) {
                continue;
            }
            let item = CompletionItem {
                label: t.name.clone(),
                insert: if s.name == "public" {
                    quote_ident(&t.name)
                } else {
                    format!("{}.{}", quote_ident(&s.name), quote_ident(&t.name))
                },
                detail: format!("{} · table", s.name),
                kind: ItemKind::Table,
            };
            if s.name == "public" {
                public.push(item);
            } else {
                other.push(item);
            }
        }
        for v in &s.views {
            if !starts_with_ci(&v.name, prefix) {
                continue;
            }
            views.push(CompletionItem {
                label: v.name.clone(),
                insert: if s.name == "public" {
                    quote_ident(&v.name)
                } else {
                    format!("{}.{}", quote_ident(&s.name), quote_ident(&v.name))
                },
                detail: format!("{} · view", s.name),
                kind: ItemKind::View,
            });
        }
    }
    let mut out = public;
    out.extend(other);
    out.extend(views);
    out.extend(schemas);
    out
}

/// Extracts only the statement surrounding `(cursor_line, prefix_col..cursor_col)`
/// (delimited by `;`) without joining the entire document. Returns the statement's
/// chars and the char index of `prefix_col` within it.
fn statement_around_caret(
    content: &Content,
    cursor_line: usize,
    prefix_col: usize,
    cursor_col: usize,
) -> (Vec<char>, usize) {
    let cur_str = content.line(cursor_line).unwrap_or("");
    let prefix_byte = cur_str
        .char_indices()
        .nth(prefix_col)
        .map_or(cur_str.len(), |(i, _)| i);
    let cursor_byte = cur_str
        .char_indices()
        .nth(cursor_col)
        .map_or(cur_str.len(), |(i, _)| i);

    // Find start of statement by scanning backward from (cursor_line, prefix_byte).
    let (start_line, start_byte) = if let Some(semi) = cur_str[..prefix_byte].rfind(';') {
        (cursor_line, semi + 1)
    } else {
        let mut l = cursor_line;
        let mut sb = 0;
        while l > 0 {
            l -= 1;
            let s = content.line(l).unwrap_or("");
            if let Some(semi) = s.rfind(';') {
                sb = semi + 1;
                break;
            }
        }
        (l, sb)
    };

    // Find end of statement by scanning forward from (cursor_line, cursor_byte).
    let line_count = content.line_count();
    let (end_line, end_byte) = if let Some(semi) = cur_str[cursor_byte..].find(';') {
        (cursor_line, cursor_byte + semi)
    } else {
        let mut l = cursor_line;
        let mut eb = cur_str.len();
        while l + 1 < line_count {
            l += 1;
            let s = content.line(l).unwrap_or("");
            if let Some(semi) = s.find(';') {
                eb = semi;
                break;
            }
            eb = s.len();
        }
        (l, eb)
    };

    let mut chars = Vec::new();
    let mut prefix_offset = 0;
    for l in start_line..=end_line {
        if l > start_line {
            chars.push('\n');
        }
        let s = content.line(l).unwrap_or("");
        let lo = if l == start_line { start_byte } else { 0 };
        let hi = if l == end_line { end_byte } else { s.len() };
        if l == cursor_line {
            chars.extend(s[lo..prefix_byte].chars());
            prefix_offset = chars.len();
            chars.extend(s[prefix_byte..hi].chars());
        } else {
            chars.extend(s[lo..hi].chars());
        }
    }
    (chars, prefix_offset)
}

/// Computes the suggestions for the caret position of `content`.
/// With `force` (Ctrl+Space) suggestions are produced even for an empty prefix.
pub fn suggest(schema: Option<&Schema>, content: &Content, force: bool) -> Option<Completion> {
    let cursor = content.cursor();
    let line: Vec<char> = content.line(cursor.line)?.chars().collect();
    let col = cursor.col.min(line.len());
    let before = &line[..col];

    // No suggestions inside string literals or line comments.
    if before.iter().filter(|c| **c == '\'').count() % 2 == 1 {
        return None;
    }
    if before.windows(2).any(|w| w == ['-', '-']) {
        return None;
    }

    let mut start = col;
    while start > 0 && is_word(line[start - 1]) {
        start -= 1;
    }
    let prefix: String = line[start..col].iter().collect();
    if prefix.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }

    let qualifier: Option<String> = if start > 0 && line[start - 1] == '.' {
        let end = start - 1;
        let name: String = if end > 0 && line[end - 1] == '"' {
            let mut s = end - 1;
            while s > 0 && line[s - 1] != '"' {
                s -= 1;
            }
            line[s..end - 1].iter().collect()
        } else {
            let mut s = end;
            while s > 0 && is_word(line[s - 1]) {
                s -= 1;
            }
            line[s..end].iter().collect()
        };
        if name.is_empty() {
            return None;
        }
        Some(name)
    } else {
        None
    };

    if !force && prefix.is_empty() && qualifier.is_none() {
        return None;
    }

    // Statement around the caret (split on `;`), without joining the full document.
    let (stmt_chars, prefix_offset) = statement_around_caret(content, cursor.line, start, col);
    let (all_tokens, before_count) = tokenize_with_split(&stmt_chars, prefix_offset);
    let before_tokens = &all_tokens[..before_count];
    let refs = table_refs(&all_tokens);

    let mut items: Vec<CompletionItem> = Vec::new();

    if let Some(q) = qualifier {
        let schema = schema?;
        // 1. Alias, or an un-aliased referenced table.
        let by_ref = refs.iter().find(|r| {
            r.alias
                .as_deref()
                .is_some_and(|a| a.eq_ignore_ascii_case(&q))
                || (r.alias.is_none() && r.name.eq_ignore_ascii_case(&q))
        });
        if let Some(r) = by_ref
            && let Some((s, t)) = find_table(schema, r.schema.as_deref(), &r.name)
        {
            items.extend(column_items(&s.name, t, false, &prefix));
        } else if let Some(s) = schema
            .schemas
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&q))
        {
            // 2. A schema: list its tables and views.
            for t in &s.tables {
                if !starts_with_ci(&t.name, &prefix) {
                    continue;
                }
                items.push(CompletionItem {
                    label: t.name.clone(),
                    insert: quote_ident(&t.name),
                    detail: "table".into(),
                    kind: ItemKind::Table,
                });
            }
            for v in &s.views {
                if !starts_with_ci(&v.name, &prefix) {
                    continue;
                }
                items.push(CompletionItem {
                    label: v.name.clone(),
                    insert: quote_ident(&v.name),
                    detail: "view".into(),
                    kind: ItemKind::View,
                });
            }
        } else if let Some((s, t)) = find_table(schema, None, &q) {
            // 3. A table name that is not (yet) in the FROM clause.
            items.extend(column_items(&s.name, t, false, &prefix));
        }
        items.sort_by(|a, b| a.label.to_ascii_lowercase().cmp(&b.label.to_ascii_lowercase()));
    } else {
        // Decide between table context and column context.
        let mut table_context = false;
        for (idx, tok) in before_tokens.iter().enumerate().rev() {
            if TABLE_KWS.iter().any(|kw| tok.eq_ignore_ascii_case(kw)) {
                table_context = !(tok.eq_ignore_ascii_case("into")
                    && before_tokens[idx + 1..].iter().any(|t| t == "("));
                break;
            }
            if COLUMN_KWS.iter().any(|kw| tok.eq_ignore_ascii_case(kw)) {
                break;
            }
        }

        if table_context {
            if let Some(schema) = schema {
                items.extend(table_items(schema, &prefix));
            }
        } else if let Some(schema) = schema {
            let mut seen = std::collections::HashSet::new();
            for r in &refs {
                if let Some((s, t)) = find_table(schema, r.schema.as_deref(), &r.name) {
                    for item in column_items(&s.name, t, true, &prefix) {
                        if seen.insert(item.label.clone()) {
                            items.push(item);
                        }
                    }
                }
            }
            for r in &refs {
                let name = r.alias.as_deref().unwrap_or(&r.name);
                if !starts_with_ci(name, &prefix) {
                    continue;
                }
                items.push(CompletionItem {
                    label: name.to_string(),
                    insert: quote_ident(name),
                    detail: if r.alias.is_some() {
                        format!("alias of {}", r.name)
                    } else {
                        "table".into()
                    },
                    kind: ItemKind::Alias,
                });
            }
        }

        if !table_context {
            for f in FUNCTIONS {
                if starts_with_ci(f, &prefix) {
                    items.push(CompletionItem {
                        label: (*f).to_string(),
                        insert: format!("{f}("),
                        detail: "function".into(),
                        kind: ItemKind::Function,
                    });
                }
            }
        }

        let upper = prefix.is_empty() || prefix.chars().any(|c| c.is_uppercase());
        for k in KEYWORDS {
            if starts_with_ci(k, &prefix) {
                let text = if upper { (*k).to_string() } else { k.to_lowercase() };
                items.push(CompletionItem {
                    label: text.clone(),
                    insert: text,
                    detail: String::new(),
                    kind: ItemKind::Keyword,
                });
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert((i.kind as u8, i.label.clone())));
    items.truncate(MAX_ITEMS);

    // Nothing useful left to suggest when the word is already complete.
    if items.len() == 1 && items[0].insert == prefix {
        return None;
    }
    if items.is_empty() {
        return None;
    }

    Some(Completion {
        items,
        selected: 0,
        start: Pos {
            line: cursor.line,
            col: start,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ColumnInfo;
    use crate::widgets::sql_editor::Action;

    fn col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            column_default: None,
            character_maximum_length: None,
            is_primary_key: name == "id",
            is_foreign_key: false,
            references: None,
        }
    }

    fn schema() -> Schema {
        let table = |name: &str, cols: &[&str]| TableInfo {
            name: name.into(),
            columns: cols.iter().map(|c| col(c)).collect(),
            constraints: vec![],
        };
        Schema {
            connection: "c".into(),
            schemas: vec![SchemaInfo {
                name: "public".into(),
                tables: vec![
                    table("users", &["id", "email"]),
                    table("orders", &["id", "user_id"]),
                ],
                views: vec![],
                routines: vec![],
                indexes: vec![],
            }],
        }
    }

    /// Suggestions with the caret at `col` on the single line `sql`.
    fn labels_at(sql: &str, col: usize) -> Vec<String> {
        let mut content = Content::with_text(sql);
        content.perform(Action::Click {
            pos: Pos { line: 0, col },
            select: false,
        });
        suggest(Some(&schema()), &content, false)
            .map(|c| c.items.into_iter().map(|i| i.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn tables_after_from() {
        let sql = "SELECT * FROM us";
        let l = labels_at(sql, sql.len());
        assert_eq!(l.first().map(String::as_str), Some("users"));
    }

    #[test]
    fn alias_columns() {
        let l = labels_at("SELECT u.em FROM users u", 11);
        assert_eq!(l.first().map(String::as_str), Some("email"));
    }

    #[test]
    fn columns_in_where() {
        let sql = "SELECT * FROM orders o JOIN users u ON u.id = o.user_id WHERE em";
        let l = labels_at(sql, sql.len());
        assert_eq!(l.first().map(String::as_str), Some("email"));
    }

    #[test]
    fn nothing_in_comment_or_string() {
        assert!(labels_at("-- sel", 6).is_empty());
        assert!(labels_at("SELECT 'sel", 11).is_empty());
    }
}
