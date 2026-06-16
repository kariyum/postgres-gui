use sqlx::{Column, PgPool, Row, TypeInfo};
use sqlx::postgres::PgPoolOptions;

use crate::types::{QueryResult, ResultColumn, ResultRow, TreeNode};

/// Connect to a PostgreSQL database and return a pool.
pub async fn connect(connection_string: &str) -> Result<PgPool, String> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(connection_string)
        .await
        .map_err(|e| e.to_string())
}

/// Execute a SQL query and return results.
pub async fn execute_query(pool: &PgPool, sql: &str) -> Result<QueryResult, String> {
    let trimmed = sql.trim().to_uppercase();

    // For SELECT-like queries, fetch rows
    if trimmed.starts_with("SELECT")
        || trimmed.starts_with("WITH")
        || trimmed.starts_with("SHOW")
        || trimmed.starts_with("EXPLAIN")
        || trimmed.starts_with("TABLE")
    {
        let rows = sqlx::query(sql)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;

        if rows.is_empty() {
            return Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                rows_affected: 0,
                message: "Query returned 0 rows.".to_string(),
            });
        }

        let columns: Vec<ResultColumn> = rows[0]
            .columns()
            .iter()
            .map(|c| ResultColumn {
                name: c.name().to_string(),
            })
            .collect();

        let result_rows: Vec<ResultRow> = rows
            .iter()
            .map(|row| {
                let cells = row
                    .columns()
                    .iter()
                    .map(|col| {
                        let idx = col.ordinal();
                        let type_name = col.type_info().name().to_string();
                        cell_to_string(row, idx, &type_name)
                    })
                    .collect();
                ResultRow { cells }
            })
            .collect();

        let count = result_rows.len() as u64;
        Ok(QueryResult {
            columns,
            rows: result_rows,
            rows_affected: count,
            message: format!("{} row(s) returned.", count),
        })
    } else {
        // For DML / DDL, just execute and report rows affected
        let result = sqlx::query(sql)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;

        let affected = result.rows_affected();
        Ok(QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected: affected,
            message: format!("Query OK. {} row(s) affected.", affected),
        })
    }
}

fn cell_to_string(row: &sqlx::postgres::PgRow, idx: usize, type_name: &str) -> String {
    match type_name {
        "INT2" => {
            if let Ok(v) = row.try_get::<i16, _>(idx) {
                return v.to_string();
            }
        }
        "INT4" => {
            if let Ok(v) = row.try_get::<i32, _>(idx) {
                return v.to_string();
            }
        }
        "INT8" => {
            if let Ok(v) = row.try_get::<i64, _>(idx) {
                return v.to_string();
            }
        }
        "FLOAT4" => {
            if let Ok(v) = row.try_get::<f32, _>(idx) {
                return v.to_string();
            }
        }
        "FLOAT8" => {
            if let Ok(v) = row.try_get::<f64, _>(idx) {
                return v.to_string();
            }
        }
        "BOOL" => {
            if let Ok(v) = row.try_get::<bool, _>(idx) {
                return v.to_string();
            }
        }
        _ => {}
    }

    // Fallback: try text, then NULL
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.unwrap_or_else(|| "NULL".to_string());
    }
    if let Ok(v) = row.try_get::<Option<&str>, _>(idx) {
        return v.unwrap_or("NULL").to_string();
    }

    "NULL".to_string()
}

/// Fetch schemas and tables for the schema browser.
pub async fn fetch_schema_tree(pool: &PgPool) -> Result<Vec<TreeNode>, String> {
    // Get schemas (excluding system ones)
    let schemas: Vec<String> = sqlx::query_scalar(
        "SELECT schema_name FROM information_schema.schemata \
         WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
         ORDER BY schema_name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut schema_nodes = Vec::new();

    for schema in &schemas {
        // Get tables for this schema
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(schema)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

        let table_nodes: Vec<TreeNode> = tables
            .iter()
            .map(|t| TreeNode {
                kind: crate::types::TreeNodeKind::Table,
                label: t.clone(),
                children: Vec::new(),
                expanded: false,
                schema: Some(schema.clone()),
            })
            .collect();

        let table_group = TreeNode {
            kind: crate::types::TreeNodeKind::TableGroup,
            label: format!("Tables ({})", table_nodes.len()),
            children: table_nodes,
            expanded: false,
            schema: Some(schema.clone()),
        };

        let schema_node = TreeNode {
            kind: crate::types::TreeNodeKind::Schema,
            label: schema.clone(),
            children: vec![table_group],
            expanded: false,
            schema: None,
        };

        schema_nodes.push(schema_node);
    }

    Ok(schema_nodes)
}
