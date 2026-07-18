use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

use super::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DescribeTableArgs {
    pub schema: String,
    pub table: String,
}

pub struct DescribeTable {
    pool: PgPool,
}

impl DescribeTable {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for DescribeTable {
    const NAME: &'static str = "describe_table";

    type Error = ToolError;
    type Args = DescribeTableArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "Describe a table's columns, types, nullability, defaults, primary key, and indexes. \
                          Returns a JSON object with 'columns' (array of column details), \
                          'primary_key' (array of PK column names), and 'indexes' (array of index definitions)."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schema": {
                        "type": "string",
                        "description": "The schema containing the table"
                    },
                    "table": {
                        "type": "string",
                        "description": "The table name to describe"
                    }
                },
                "required": ["schema", "table"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let columns: Vec<Value> = sqlx::query(
            "SELECT column_name, data_type, is_nullable, column_default, character_maximum_length \
             FROM information_schema.columns \
             WHERE table_schema = $1 AND table_name = $2 \
             ORDER BY ordinal_position",
        )
        .bind(&args.schema)
        .bind(&args.table)
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|row| {
            json!({
                "name": row.get::<String, _>("column_name"),
                "type": row.get::<String, _>("data_type"),
                "nullable": row.get::<String, _>("is_nullable") == "YES",
                "default": row.get::<Option<String>, _>("column_default"),
                "max_length": row.get::<Option<i32>, _>("character_maximum_length"),
            })
        })
        .collect();

        let primary_key: Vec<String> = sqlx::query_scalar(
            "SELECT kcu.column_name \
             FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu \
               ON tc.constraint_name = kcu.constraint_name \
              AND tc.table_schema = kcu.table_schema \
              AND tc.table_name = kcu.table_name \
             WHERE tc.table_schema = $1 \
               AND tc.table_name = $2 \
               AND tc.constraint_type = 'PRIMARY KEY' \
             ORDER BY kcu.ordinal_position",
        )
        .bind(&args.schema)
        .bind(&args.table)
        .fetch_all(&self.pool)
        .await?;

        let indexes: Vec<Value> = sqlx::query(
            "SELECT indexname, indexdef \
             FROM pg_indexes \
             WHERE schemaname = $1 AND tablename = $2 \
             ORDER BY indexname",
        )
        .bind(&args.schema)
        .bind(&args.table)
        .fetch_all(&self.pool)
        .await?
        .iter()
        .map(|row| {
            json!({
                "name": row.get::<String, _>("indexname"),
                "definition": row.get::<String, _>("indexdef"),
            })
        })
        .collect();

        Ok(json!({
            "schema": args.schema,
            "table": args.table,
            "columns": columns,
            "primary_key": primary_key,
            "indexes": indexes,
        })
        .to_string())
    }
}
