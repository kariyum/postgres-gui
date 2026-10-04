use sqlx::{Column, PgPool, Row, TypeInfo};
use sqlx::postgres::PgPoolOptions;

use crate::types::{ColumnInfo, QueryResult, ResultColumn, ResultRow, SchemaInfo, TableInfo, TreeNode};

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
        return match v {
            Some(s) => s.replace('\r', "\\r").replace('\n', "\\n"),
            None => "NULL".to_string(),
        };
    }
    if let Ok(v) = row.try_get::<Option<&str>, _>(idx) {
        return match v {
            Some(s) => s.replace('\r', "\\r").replace('\n', "\\n"),
            None => "NULL".to_string(),
        };
    }

    "NULL".to_string()
}

/// Fetch the full schema (schemas, tables, and columns) for the schema explorer.
pub async fn fetch_schema(pool: &PgPool) -> Result<Vec<SchemaInfo>, String> {
    let rows = sqlx::query(
        "SELECT c.table_schema, c.table_name, c.column_name, c.data_type, c.is_nullable \
         FROM information_schema.columns c \
         JOIN information_schema.tables t \
           ON t.table_schema = c.table_schema AND t.table_name = c.table_name \
         WHERE c.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
           AND t.table_type = 'BASE TABLE' \
         ORDER BY c.table_schema, c.table_name, c.ordinal_position",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut schemas: Vec<SchemaInfo> = Vec::new();
    for row in &rows {
        let schema_name: String = row.try_get("table_schema").map_err(|e| e.to_string())?;
        let table_name: String = row.try_get("table_name").map_err(|e| e.to_string())?;
        let column_name: String = row.try_get("column_name").map_err(|e| e.to_string())?;
        let data_type: String = row.try_get("data_type").map_err(|e| e.to_string())?;
        let nullable: String = row.try_get("is_nullable").map_err(|e| e.to_string())?;

        let schema = match schemas.iter_mut().find(|s| s.name == schema_name) {
            Some(schema) => schema,
            None => {
                schemas.push(SchemaInfo {
                    name: schema_name.clone(),
                    tables: Vec::new(),
                });
                schemas.last_mut().unwrap()
            }
        };

        let table = match schema.tables.iter_mut().find(|t| t.name == table_name) {
            Some(table) => table,
            None => {
                schema.tables.push(TableInfo {
                    name: table_name.clone(),
                    columns: Vec::new(),
                });
                schema.tables.last_mut().unwrap()
            }
        };

        table.columns.push(ColumnInfo {
            name: column_name,
            data_type,
            nullable: nullable == "YES",
        });
    }

    Ok(schemas)
}
