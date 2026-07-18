use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Column, PgPool, Row, TypeInfo};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

use super::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteSqlArgs {
    pub sql: String,
}

pub struct ExecuteSql {
    pool: PgPool,
}

impl ExecuteSql {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ExecuteSql {
    const NAME: &'static str = "execute_sql";

    type Error = ToolError;
    type Args = ExecuteSqlArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Execute a SQL query against the connected PostgreSQL database. \
                          Returns results as a JSON object with 'columns' (array of column names), \
                          'rows' (array of arrays), 'rows_affected' count, and a 'truncated' flag. \
                          Results are capped at 50 rows. \
                          Use for SELECT, INSERT, UPDATE, DELETE, DDL, or any arbitrary SQL."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "sql": {
                        "type": "string",
                        "description": "The SQL query to execute"
                    }
                },
                "required": ["sql"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let trimmed = args.sql.trim().to_uppercase();
        let is_select = trimmed.starts_with("SELECT")
            || trimmed.starts_with("WITH")
            || trimmed.starts_with("SHOW")
            || trimmed.starts_with("EXPLAIN")
            || trimmed.starts_with("TABLE");

        if is_select {
            let rows = sqlx::query(&args.sql).fetch_all(&self.pool).await?;

            if rows.is_empty() {
                return Ok(json!({
                    "columns": [],
                    "rows": [],
                    "rows_affected": 0,
                    "truncated": false,
                    "message": "Query returned 0 rows."
                })
                .to_string());
            }

            let columns: Vec<String> = rows[0]
                .columns()
                .iter()
                .map(|c| c.name().to_string())
                .collect();

            let total = rows.len();
            let display_rows: Vec<Vec<Value>> = rows
                .iter()
                .take(50)
                .map(|row| {
                    row.columns()
                        .iter()
                        .map(|col| {
                            let idx = col.ordinal();
                            let type_name = col.type_info().name().to_string();
                            cell_to_value(row, idx, &type_name)
                        })
                        .collect()
                })
                .collect();

            let truncated = total > 50;

            Ok(json!({
                "columns": columns,
                "rows": display_rows,
                "rows_affected": total as u64,
                "truncated": truncated,
                "message": if truncated {
                    format!("{} row(s) returned (showing first 50).", total)
                } else {
                    format!("{} row(s) returned.", total)
                }
            })
            .to_string())
        } else {
            let result = sqlx::query(&args.sql).execute(&self.pool).await?;

            let affected = result.rows_affected();
            Ok(json!({
                "columns": [],
                "rows": [],
                "rows_affected": affected,
                "truncated": false,
                "message": format!("Query OK. {} row(s) affected.", affected)
            })
            .to_string())
        }
    }
}

fn cell_to_value(row: &sqlx::postgres::PgRow, idx: usize, type_name: &str) -> Value {
    let string_val = match type_name {
        "INT2" => row.try_get::<i16, _>(idx).ok().map(|v| v.to_string()),
        "INT4" => row.try_get::<i32, _>(idx).ok().map(|v| v.to_string()),
        "INT8" => row.try_get::<i64, _>(idx).ok().map(|v| v.to_string()),
        "FLOAT4" => row.try_get::<f32, _>(idx).ok().map(|v| v.to_string()),
        "FLOAT8" => row.try_get::<f64, _>(idx).ok().map(|v| v.to_string()),
        "BOOL" => row.try_get::<bool, _>(idx).ok().map(|v| v.to_string()),
        _ => None,
    };

    if let Some(s) = string_val {
        return Value::String(s);
    }

    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return v.map_or(Value::Null, Value::String);
    }
    if let Ok(v) = row.try_get::<Option<&str>, _>(idx) {
        return v.map_or(Value::Null, |s| Value::String(s.to_string()));
    }

    Value::Null
}
