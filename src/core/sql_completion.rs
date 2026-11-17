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
    haystack
        .to_lowercase()
        .starts_with(&prefix.to_lowercase())
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

/// Splits SQL into tokens. Qualified names (`a.b`) stay a single token, quotes
/// are stripped, strings collapse to `'`, and comments are dropped.
fn tokenize(chars: &[char]) -> Vec<String> {
    let n = chars.len();
    let mut out = Vec::new();
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
            i += 1;
            while i < n && chars[i] != '\'' {
                i += 1;
            }
            i += 1;
            out.push("'".to_string());
        } else if is_word(c) || c == '"' {
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
        } else {
            out.push(c.to_string());
            i += 1;
        }
    }
    out
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

fn column_items(schema_name: &str, table: &TableInfo, with_table: bool) -> Vec<CompletionItem> {
    let _ = schema_name;
    table
        .columns
        .iter()
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

fn table_items(schema: &Schema) -> Vec<CompletionItem> {
    let mut public = Vec::new();
    let mut other = Vec::new();
    let mut views = Vec::new();
    let mut schemas = Vec::new();
    for s in &schema.schemas {
        schemas.push(CompletionItem {
            label: s.name.clone(),
            insert: quote_ident(&s.name),
            detail: "schema".into(),
            kind: ItemKind::Schema,
        });
        for t in &s.tables {
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

    // Statement around the caret (split on `;`).
    let all: Vec<char> = content.text().chars().collect();
    let offset: usize = (0..cursor.line)
        .map(|i| content.line(i).map_or(0, |l| l.chars().count()) + 1)
        .sum::<usize>()
        + col;
    let prefix_offset = offset - (col - start);
    let stmt_start = all[..prefix_offset]
        .iter()
        .rposition(|c| *c == ';')
        .map_or(0, |i| i + 1);
    let stmt_end = all[offset..]
        .iter()
        .position(|c| *c == ';')
        .map_or(all.len(), |i| offset + i);
    let before_tokens = tokenize(&all[stmt_start..prefix_offset]);
    let all_tokens = tokenize(&all[stmt_start..stmt_end]);
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
            items.extend(column_items(&s.name, t, false));
        } else if let Some(s) = schema
            .schemas
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&q))
        {
            // 2. A schema: list its tables and views.
            for t in &s.tables {
                items.push(CompletionItem {
                    label: t.name.clone(),
                    insert: quote_ident(&t.name),
                    detail: "table".into(),
                    kind: ItemKind::Table,
                });
            }
            for v in &s.views {
                items.push(CompletionItem {
                    label: v.name.clone(),
                    insert: quote_ident(&v.name),
                    detail: "view".into(),
                    kind: ItemKind::View,
                });
            }
        } else if let Some((s, t)) = find_table(schema, None, &q) {
            // 3. A table name that is not (yet) in the FROM clause.
            items.extend(column_items(&s.name, t, false));
        }
        items.retain(|i| starts_with_ci(&i.label, &prefix));
        items.sort_by_key(|i| i.label.to_lowercase());
    } else {
        // Decide between table context and column context.
        let lower: Vec<String> = before_tokens.iter().map(|t| t.to_lowercase()).collect();
        let mut table_context = false;
        for (idx, tok) in lower.iter().enumerate().rev() {
            if TABLE_KWS.contains(&tok.as_str()) {
                table_context = !(tok == "into"
                    && before_tokens[idx + 1..].iter().any(|t| t == "("));
                break;
            }
            if COLUMN_KWS.contains(&tok.as_str()) {
                break;
            }
        }

        if table_context {
            if let Some(schema) = schema {
                items.extend(table_items(schema));
            }
        } else if let Some(schema) = schema {
            let mut seen = std::collections::HashSet::new();
            for r in &refs {
                if let Some((s, t)) = find_table(schema, r.schema.as_deref(), &r.name) {
                    for item in column_items(&s.name, t, true) {
                        if seen.insert(item.label.clone()) {
                            items.push(item);
                        }
                    }
                }
            }
            for r in &refs {
                let name = r.alias.clone().unwrap_or_else(|| r.name.clone());
                items.push(CompletionItem {
                    label: name.clone(),
                    insert: quote_ident(&name),
                    detail: if r.alias.is_some() {
                        format!("alias of {}", r.name)
                    } else {
                        "table".into()
                    },
                    kind: ItemKind::Alias,
                });
            }
        }
        items.retain(|i| starts_with_ci(&i.label, &prefix));

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
