/// A column in a query result.
#[derive(Debug, Clone)]
pub struct ResultColumn {
    pub name: String,
}

impl crate::widgets::table::TableColumn for ResultColumn {
    fn name(&self) -> &str {
        &self.name
    }
}

/// A single row of query results (each cell is a string for display).
#[derive(Debug, Clone)]
pub struct ResultRow {
    pub cells: Vec<String>,
}

impl crate::widgets::table::TableRow for ResultRow {
    fn cells(&self) -> &[String] {
        &self.cells
    }
}

/// The full result of executing a query.
#[derive(Debug)]
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

/// A foreign key reference: which schema.table.column a column points to.
#[derive(Debug, Clone)]
pub struct ForeignKeyRef {
    pub schema: String,
    pub table: String,
    pub column: String,
}

/// A column in a table's schema definition.
#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub column_default: Option<String>,
    pub character_maximum_length: Option<i32>,
    pub is_primary_key: bool,
    pub is_foreign_key: bool,
    pub references: Option<ForeignKeyRef>,
}

/// A table and its columns within a database schema.
#[derive(Debug, Clone)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
}

/// Whether a routine is a function or a procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutineKind {
    Function,
    Procedure,
}

/// A function or procedure defined in a schema.
#[derive(Debug, Clone)]
pub struct RoutineInfo {
    pub name: String,
    pub kind: RoutineKind,
    pub arguments: String,
    /// `None` for procedures, which have no return type.
    pub return_type: Option<String>,
    pub definition: String,
}

/// An index defined on a table in a schema.
#[derive(Debug, Clone)]
pub struct IndexInfo {
    pub name: String,
    pub table: String,
    pub definition: String,
    pub is_unique: bool,
}

/// A named PostgreSQL schema and its tables, routines, and indexes.
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub name: String,
    pub tables: Vec<TableInfo>,
    pub routines: Vec<RoutineInfo>,
    pub indexes: Vec<IndexInfo>,
}

/// The full schema catalog for a single connection/database.
#[derive(Debug, Clone)]
pub struct Schema {
    pub connection: String,
    pub schemas: Vec<SchemaInfo>,
}

impl Schema {
    pub fn new(connection: String) -> Schema {
        Self {
            connection,
            schemas: Vec::new(),
        }
    }
}
