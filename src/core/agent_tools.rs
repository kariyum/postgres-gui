use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Column, PgPool, Row, TypeInfo};

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

#[derive(Debug, Clone)]
pub struct ToolError(pub String);

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ToolError {}

impl From<serde_json::Error> for ToolError {
    fn from(e: serde_json::Error) -> Self {
        ToolError(e.to_string())
    }
}

impl From<sqlx::Error> for ToolError {
    fn from(e: sqlx::Error) -> Self {
        ToolError(e.to_string())
    }
}

pub fn is_destructive(sql: &str) -> bool {
    let trimmed = sql.trim().to_uppercase();
    trimmed.starts_with("INSERT")
        || trimmed.starts_with("UPDATE")
        || trimmed.starts_with("DELETE")
        || trimmed.starts_with("DROP")
        || trimmed.starts_with("TRUNCATE")
        || trimmed.starts_with("ALTER")
        || trimmed.starts_with("CREATE")
        || trimmed.starts_with("REINDEX")
        || trimmed.starts_with("VACUUM")
        || trimmed.starts_with("CLUSTER")
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSchemasArgs;

pub struct ListSchemas {
    pool: PgPool,
}

impl ListSchemas {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ListSchemas {
    const NAME: &'static str = "list_schemas";

    type Error = ToolError;
    type Args = ListSchemasArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "List all non-system schemas in the connected PostgreSQL database. \
                          Returns a JSON array of schema names."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let schemas: Vec<String> = sqlx::query_scalar(
            "SELECT schema_name FROM information_schema.schemata \
             WHERE schema_name NOT IN ('information_schema', 'pg_catalog', 'pg_toast') \
             ORDER BY schema_name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(json!({
            "schemas": schemas
        })
        .to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTablesArgs {
    pub schema: String,
}

pub struct ListTables {
    pool: PgPool,
}

impl ListTables {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ListTables {
    const NAME: &'static str = "list_tables";

    type Error = ToolError;
    type Args = ListTablesArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description: "List all base tables in a given schema. \
                          Returns a JSON object with schema name and an array of table names."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schema": {
                        "type": "string",
                        "description": "The schema name to list tables from"
                    }
                },
                "required": ["schema"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = $1 AND table_type = 'BASE TABLE' \
             ORDER BY table_name",
        )
        .bind(&args.schema)
        .fetch_all(&self.pool)
        .await?;

        Ok(json!({
            "schema": args.schema,
            "tables": tables
        })
        .to_string())
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainQueryArgs {
    pub sql: String,
}

pub struct ExplainQuery {
    pool: PgPool,
}

impl ExplainQuery {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl Tool for ExplainQuery {
    const NAME: &'static str = "explain_query";

    type Error = ToolError;
    type Args = ExplainQueryArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: self.name(),
            description:
                "Get the query execution plan for a SQL statement using EXPLAIN (FORMAT JSON). \
                          Returns the plan as a JSON object. \
                          Note: This does not execute the query (no ANALYZE)."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "sql": {
                        "type": "string",
                        "description": "The SQL query to explain"
                    }
                },
                "required": ["sql"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let explain_sql = format!("EXPLAIN (FORMAT JSON) {}", args.sql);
        let rows = sqlx::query(&explain_sql).fetch_all(&self.pool).await?;

        let plan_lines: Vec<String> = rows.iter().map(|row| row.get::<String, _>(0)).collect();

        let plan_json: Value = plan_lines
            .join("")
            .parse()
            .unwrap_or(Value::String(plan_lines.join("\n")));

        Ok(json!({
            "query": args.sql,
            "plan": plan_json
        })
        .to_string())
    }
}

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

pub struct ToolManager {
    toolset: rig_core::tool::ToolSet,
}

impl ToolManager {
    pub fn new(pool: PgPool) -> Self {
        let mut toolset = rig_core::tool::ToolSet::default();
        toolset.add_tool(ExecuteSql::new(pool.clone()));
        toolset.add_tool(ListSchemas::new(pool.clone()));
        toolset.add_tool(ListTables::new(pool.clone()));
        toolset.add_tool(DescribeTable::new(pool.clone()));
        toolset.add_tool(ExplainQuery::new(pool.clone()));
        toolset.add_tool(ShowTableStats::new(pool));

        Self { toolset }
    }

    pub async fn definitions(&self) -> Result<Vec<ToolDefinition>, ToolError> {
        self.toolset
            .get_tool_definitions()
            .await
            .map_err(|e| ToolError(e.to_string()))
    }

    pub async fn execute(&self, tool_name: &str, args_json: &str) -> Result<String, ToolError> {
        self.toolset
            .call(tool_name, args_json.to_string())
            .await
            .map_err(|e| ToolError(e.to_string()))
    }
}
