use std::collections::{HashMap, HashSet};

use sqlx::postgres::{PgHasArrayType, PgPoolOptions, PgRow};
use sqlx::types::chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use sqlx::types::{Decimal, JsonValue, Uuid};
use sqlx::{Column, Decode, PgPool, Postgres, Row, Type, TypeInfo, ValueRef};

use crate::types::{
    ColumnInfo, ForeignKeyRef, QueryResult, ResultColumn, ResultRow, SchemaInfo, TableInfo,
};

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

fn cell_to_string(row: &PgRow, idx: usize, type_name: &str) -> String {
    let decoded = match type_name {
        "BOOL" => scalar::<bool>(row, idx),
        "INT2" => scalar::<i16>(row, idx),
        "INT4" => scalar::<i32>(row, idx),
        "INT8" => scalar::<i64>(row, idx),
        "FLOAT4" => scalar::<f32>(row, idx),
        "FLOAT8" => scalar::<f64>(row, idx),
        "NUMERIC" => scalar::<Decimal>(row, idx),
        "UUID" => scalar::<Uuid>(row, idx),
        "DATE" => scalar::<NaiveDate>(row, idx),
        "TIME" => scalar::<NaiveTime>(row, idx),
        "TIMESTAMP" => scalar::<NaiveDateTime>(row, idx),
        "TIMESTAMPTZ" => scalar::<DateTime<Utc>>(row, idx),
        "JSON" | "JSONB" => scalar::<JsonValue>(row, idx),
        "BYTEA" => decode(row, idx, |bytes: Vec<u8>| {
            format!("\\x{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>())
        }),
        "_BOOL" => array::<bool>(row, idx),
        "_INT2" => array::<i16>(row, idx),
        "_INT4" => array::<i32>(row, idx),
        "_INT8" => array::<i64>(row, idx),
        "_FLOAT4" => array::<f32>(row, idx),
        "_FLOAT8" => array::<f64>(row, idx),
        "_NUMERIC" => array::<Decimal>(row, idx),
        "_UUID" => array::<Uuid>(row, idx),
        "_TEXT" | "_VARCHAR" | "_BPCHAR" | "_NAME" => array::<String>(row, idx),
        _ => None,
    };

    decoded
        .or_else(|| raw_text(row, idx))
        .unwrap_or_else(|| format!("<{}>", type_name.to_lowercase()))
}

/// Decode a cell as `T` and convert it with `to_text`. Returns `None` if the
/// column can't be decoded as `T`, and `"NULL"` for SQL NULL.
fn decode<T>(row: &PgRow, idx: usize, to_text: impl FnOnce(T) -> String) -> Option<String>
where
    T: for<'r> Decode<'r, Postgres> + Type<Postgres>,
{
    match row.try_get::<Option<T>, _>(idx) {
        Ok(Some(v)) => Some(to_text(v)),
        Ok(None) => Some("NULL".to_string()),
        Err(_) => None,
    }
}

fn scalar<T>(row: &PgRow, idx: usize) -> Option<String>
where
    T: ToString + for<'r> Decode<'r, Postgres> + Type<Postgres>,
{
    decode::<T>(row, idx, |v| v.to_string())
}

fn array<T>(row: &PgRow, idx: usize) -> Option<String>
where
    T: ToString + PgHasArrayType + for<'r> Decode<'r, Postgres> + Type<Postgres>,
{
    decode::<Vec<Option<T>>>(row, idx, format_array)
}

/// Fallback: read the raw wire value as UTF-8. Enum labels and other
/// text-like custom types are plain UTF-8 in both text and binary format.
fn raw_text(row: &PgRow, idx: usize) -> Option<String> {
    let raw = row.try_get_raw(idx).ok()?;
    if raw.is_null() {
        return Some("NULL".to_string());
    }
    raw.as_str().ok().map(escape_newlines)
}

fn escape_newlines(s: &str) -> String {
    s.replace('\r', "\\r").replace('\n', "\\n")
}

fn format_array<T: ToString>(items: Vec<Option<T>>) -> String {
    let parts: Vec<String> = items
        .into_iter()
        .map(|item| match item {
            Some(v) => escape_newlines(&v.to_string()),
            None => "NULL".to_string(),
        })
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// Fetch the full schema (schemas, tables, and columns) for the schema explorer.
pub async fn fetch_schema(pool: &PgPool) -> Result<Vec<SchemaInfo>, String> {
    let rows = sqlx::query(
        "SELECT c.table_schema, c.table_name, c.column_name, c.data_type, c.is_nullable, \
                c.column_default, c.character_maximum_length \
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

    // Primary key columns: (schema, table) -> set of column names.
    let pk_rows = sqlx::query(
        "SELECT tc.table_schema, tc.table_name, kcu.column_name \
         FROM information_schema.table_constraints tc \
         JOIN information_schema.key_column_usage kcu \
           ON tc.constraint_name = kcu.constraint_name \
          AND tc.constraint_schema = kcu.constraint_schema \
          AND tc.table_schema = kcu.table_schema \
          AND tc.table_name = kcu.table_name \
         WHERE tc.constraint_type = 'PRIMARY KEY' \
           AND tc.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut primary_keys: HashMap<(String, String), HashSet<String>> = HashMap::new();
    for row in &pk_rows {
        let schema_name: String = row.try_get("table_schema").map_err(|e| e.to_string())?;
        let table_name: String = row.try_get("table_name").map_err(|e| e.to_string())?;
        let column_name: String = row.try_get("column_name").map_err(|e| e.to_string())?;
        primary_keys
            .entry((schema_name, table_name))
            .or_default()
            .insert(column_name);
    }

    // Foreign key columns: (schema, table, column) -> referenced (schema, table, column).
    let fk_rows = sqlx::query(
        "SELECT kcu.table_schema, kcu.table_name, kcu.column_name, \
                ccu.table_schema AS foreign_schema, ccu.table_name AS foreign_table, \
                ccu.column_name AS foreign_column \
         FROM information_schema.key_column_usage kcu \
         JOIN information_schema.referential_constraints rc \
           ON rc.constraint_schema = kcu.constraint_schema \
          AND rc.constraint_name = kcu.constraint_name \
         JOIN information_schema.key_column_usage ccu \
           ON ccu.constraint_schema = rc.unique_constraint_schema \
          AND ccu.constraint_name = rc.unique_constraint_name \
          AND ccu.ordinal_position = kcu.position_in_unique_constraint \
         WHERE kcu.table_schema NOT IN ('information_schema', 'pg_catalog', 'pg_toast')",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut foreign_keys: HashMap<(String, String, String), ForeignKeyRef> = HashMap::new();
    for row in &fk_rows {
        let schema_name: String = row.try_get("table_schema").map_err(|e| e.to_string())?;
        let table_name: String = row.try_get("table_name").map_err(|e| e.to_string())?;
        let column_name: String = row.try_get("column_name").map_err(|e| e.to_string())?;
        let foreign_schema: String = row.try_get("foreign_schema").map_err(|e| e.to_string())?;
        let foreign_table: String = row.try_get("foreign_table").map_err(|e| e.to_string())?;
        let foreign_column: String = row.try_get("foreign_column").map_err(|e| e.to_string())?;
        foreign_keys.insert(
            (schema_name, table_name, column_name),
            ForeignKeyRef {
                schema: foreign_schema,
                table: foreign_table,
                column: foreign_column,
            },
        );
    }

    let mut schemas: Vec<SchemaInfo> = Vec::new();
    for row in &rows {
        let schema_name: String = row.try_get("table_schema").map_err(|e| e.to_string())?;
        let table_name: String = row.try_get("table_name").map_err(|e| e.to_string())?;
        let column_name: String = row.try_get("column_name").map_err(|e| e.to_string())?;
        let data_type: String = row.try_get("data_type").map_err(|e| e.to_string())?;
        let nullable: String = row.try_get("is_nullable").map_err(|e| e.to_string())?;
        let column_default: Option<String> =
            row.try_get("column_default").map_err(|e| e.to_string())?;
        let character_maximum_length: Option<i32> = row
            .try_get("character_maximum_length")
            .map_err(|e| e.to_string())?;

        let is_primary_key = primary_keys
            .get(&(schema_name.clone(), table_name.clone()))
            .is_some_and(|cols| cols.contains(&column_name));
        let references = foreign_keys
            .get(&(schema_name.clone(), table_name.clone(), column_name.clone()))
            .cloned();

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
            column_default,
            character_maximum_length,
            is_primary_key,
            is_foreign_key: references.is_some(),
            references,
        });
    }

    Ok(schemas)
}
