use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A column in a query result.
#[derive(Debug, Clone)]
pub struct ResultColumn {
    pub name: String,
}

/// A single row of query results (each cell is a string for display).
#[derive(Debug, Clone)]
pub struct ResultRow {
    pub cells: Vec<String>,
}

/// The full result of executing a query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<ResultColumn>,
    pub rows: Vec<ResultRow>,
    pub rows_affected: u64,
    pub message: String,
}

/// Schema browser tree node kinds.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum TreeNodeKind {
    Connection,
    SchemaGroup,
    Schema,
    TableGroup,
    Table,
}

/// A node in the schema browser tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub kind: TreeNodeKind,
    pub label: String,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
    /// For Schema/Table nodes, the qualified parent path (e.g. schema name)
    pub schema: Option<String>,
}
