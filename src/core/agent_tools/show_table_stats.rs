use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

use super::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShowTableStatsArgs {
    pub schema: String,
    pub table: String,
}

pub struct ShowTableStats {
    pool: PgPool,
}

impl ShowTableStats {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ShowTableStats {
    const NAME: &'static str = "show_table_stats";

    type Error = ToolError;
    type Args = ShowTableStatsArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description:
                "Get table statistics including estimated row count, total size, table size, \
                          and index size. Returns a JSON object with byte sizes and row estimate. \
                          Sizes are returned in bytes and as human-readable strings."
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
                        "description": "The table name to get stats for"
                    }
                },
                "required": ["schema", "table"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query(
            "SELECT \
               (SELECT pg_total_relation_size($1)) AS total_size_bytes, \
               (SELECT pg_relation_size($1)) AS table_size_bytes, \
               (SELECT pg_indexes_size($1)) AS index_size_bytes, \
               COALESCE((SELECT n_live_tup FROM pg_stat_user_tables \
                         WHERE schemaname = $2 AND relname = $3), 0) AS row_estimate",
        )
        .bind(format!("{}.{}", args.schema, args.table))
        .bind(&args.schema)
        .bind(&args.table)
        .fetch_one(&self.pool)
        .await?;

        let total: i64 = row.get("total_size_bytes");
        let table: i64 = row.get("table_size_bytes");
        let index: i64 = row.get("index_size_bytes");
        let row_est: i64 = row.get("row_estimate");

        fn fmt_bytes(bytes: i64) -> String {
            if bytes >= 1_073_741_824 {
                format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
            } else if bytes >= 1_048_576 {
                format!("{:.2} MB", bytes as f64 / 1_048_576.0)
            } else if bytes >= 1024 {
                format!("{:.2} kB", bytes as f64 / 1024.0)
            } else {
                format!("{} B", bytes)
            }
        }

        Ok(json!({
            "schema": args.schema,
            "table": args.table,
            "row_estimate": row_est,
            "total_size_bytes": total,
            "total_size_human": fmt_bytes(total),
            "table_size_bytes": table,
            "table_size_human": fmt_bytes(table),
            "index_size_bytes": index,
            "index_size_human": fmt_bytes(index),
        })
        .to_string())
    }
}
