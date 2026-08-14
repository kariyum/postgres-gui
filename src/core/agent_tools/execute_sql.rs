use iced::futures::channel::mpsc::Sender;
use rig_core::tool::PortableTool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Column, Row, TypeInfo};

use super::{DatabaseKeeperMessage, ToolError, cell_to_value, get_pool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteSqlArgs {
    pub database_name: String,
    pub sql: String,
}

pub struct ExecuteSql {
    db_actor: Sender<DatabaseKeeperMessage>,
}

impl Clone for ExecuteSql {
    fn clone(&self) -> Self {
        Self {
            db_actor: self.db_actor.clone(),
        }
    }
}

impl ExecuteSql {
    pub fn new(db_actor: Sender<DatabaseKeeperMessage>) -> Self {
        Self { db_actor }
    }
}

impl PortableTool for ExecuteSql {
    const NAME: &'static str = "execute_sql";

    type Error = ToolError;
    type Args = ExecuteSqlArgs;
    type Output = String;

    fn description(&self) -> String {
        "Execute a SQL query against a PostgreSQL database. \
                          The database_name must match one of the available connected databases. \
                          Returns results as a JSON object with 'columns' (array of column names), \
                          'rows' (array of arrays), 'rows_affected' count, and a 'truncated' flag. \
                          Results are capped at 50 rows. \
                          Use for SELECT, INSERT, UPDATE, DELETE, DDL, or any arbitrary SQL."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "database_name": {
                    "type": "string",
                    "description": "The name of the database to execute the query on"
                },
                "sql": {
                    "type": "string",
                    "description": "The SQL query to execute"
                }
            },
            "required": ["database_name", "sql"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let pool = get_pool(&mut self.db_actor.clone(), &args.database_name).await?;
        let trimmed = args.sql.trim().to_uppercase();
        let is_select = trimmed.starts_with("SELECT")
            || trimmed.starts_with("WITH")
            || trimmed.starts_with("SHOW")
            || trimmed.starts_with("EXPLAIN")
            || trimmed.starts_with("TABLE");

        if is_select {
            let rows = sqlx::query(&args.sql).fetch_all(&pool).await?;

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
            let result = sqlx::query(&args.sql).execute(&pool).await?;

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
