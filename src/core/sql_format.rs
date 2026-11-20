//! PostgreSQL SQL formatting powered by the `sqlformat` crate.

use sqlformat::{Dialect, FormatOptions, Indent, QueryParams};

/// Formats a PostgreSQL SQL string with 4-space indentation and uppercase keywords.
pub fn format_sql(sql: &str) -> String {
    if sql.trim().is_empty() {
        return sql.to_string();
    }
    let options = FormatOptions {
        indent: Indent::Spaces(4),
        uppercase: Some(true),
        lines_between_queries: 2,
        dialect: Dialect::PostgreSql,
        ..Default::default()
    };
    sqlformat::format(sql, &QueryParams::None, &options)
}

#[cfg(test)]
mod tests {
    use super::format_sql;

    #[test]
    fn formats_select_query() {
        let input = "select u.id, u.email from users u where u.active = true;";
        let formatted = format_sql(input);
        assert!(formatted.contains("SELECT"));
        assert!(formatted.contains("FROM"));
        assert!(formatted.contains("WHERE"));
    }

    #[test]
    fn preserves_empty_input() {
        assert_eq!(format_sql(""), "");
        assert_eq!(format_sql("   "), "   ");
    }
}
