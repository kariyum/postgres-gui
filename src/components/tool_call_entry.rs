use anyhow::Context;
use iced::border::Radius;
use iced::widget::space::horizontal;
use iced::widget::{button, column, container, row, space, text};
use iced::{Background, Border, Color, Element, Length, Theme};
use serde::{Deserialize, Serialize};

use crate::core::agent_tools::connect_to_database::ConnectToDatabaseArgs;
use crate::core::agent_tools::describe_table::DescribeTableArgs;
use crate::core::agent_tools::execute_sql::ExecuteSqlArgs;
use crate::core::agent_tools::explain_query::ExplainQueryArgs;
use crate::core::agent_tools::list_connections::ListConnectionsArgs;
use crate::core::agent_tools::list_schemas::ListSchemasArgs;
use crate::core::agent_tools::list_tables::ListTablesArgs;
use crate::core::agent_tools::show_table_stats::ShowTableStatsArgs;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCallEntry {
    pub call_id: String,
    pub tool_name: String,
    pub args: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub status: ToolCallStatus,
    pub tool_details: Result<ToolDetails, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToolArgs {
    ConnectToDatabase(ConnectToDatabaseArgs),
    DescribeTable(DescribeTableArgs),
    ExecuteSQL(ExecuteSqlArgs),
    ExplainQuery(ExplainQueryArgs),
    ListConnections(ListConnectionsArgs),
    ListSchemas(ListSchemasArgs),
    ListTables(ListTablesArgs),
    ShowTableStats(ShowTableStatsArgs),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDetails {
    pub args: ToolArgs,
}

impl ToolDetails {
    pub fn new(tool_name: &str, args: String) -> anyhow::Result<ToolDetails> {
        let args = ToolArgs::new(tool_name, args);
        if let Err(ref err) = args {
            tracing::warn!("ToolArgs construction failed with {err}")
        }
        Ok(ToolDetails { args: args? })
    }

    fn view(&self) -> Element<'_, Message> {
        match self.args {
            ToolArgs::ConnectToDatabase(ref args) => {
                text(format!("Database name: {}", args.database_name)).into()
            }
            ToolArgs::DescribeTable(ref args) => {
                text(format!("Describe Table {}.{}", args.schema, args.table)).into()
            }
            ToolArgs::ExecuteSQL(ref args) => column![text(args.sql.as_str())].into(),
            ToolArgs::ExplainQuery(ref args) => text(format!("{}", args.sql)).into(),
            ToolArgs::ListConnections(_) => space().into(),
            ToolArgs::ListSchemas(ref args) => text(format!("{}", args.database_name)).into(),
            ToolArgs::ListTables(ref args) => text(format!("{}", args.schema)).into(),
            ToolArgs::ShowTableStats(ref args) => text(format!("{}", args.table)).into(),
        }
    }
}

impl ToolArgs {
    fn new(tool_name: &str, args: String) -> anyhow::Result<ToolArgs> {
        let result = match tool_name {
            "connect_to_database" => {
                serde_json::from_str(args.as_str()).map(ToolArgs::ConnectToDatabase)
            }
            "describe_table" => serde_json::from_str(args.as_str()).map(ToolArgs::DescribeTable),
            "execute_sql" => serde_json::from_str(args.as_str()).map(ToolArgs::ExecuteSQL),
            "explain_query" => serde_json::from_str(args.as_str()).map(ToolArgs::ExplainQuery),
            "list_connections" => {
                serde_json::from_str(args.as_str()).map(ToolArgs::ListConnections)
            }
            "list_schemas" => serde_json::from_str(args.as_str()).map(ToolArgs::ListSchemas),
            "list_tables" => serde_json::from_str(args.as_str()).map(ToolArgs::ListTables),
            "show_table_stats" => serde_json::from_str(args.as_str()).map(ToolArgs::ShowTableStats),
            other => anyhow::bail!("Unknown tool: {other}"),
        };
        result.context(format!("Failed to deserialize {tool_name} {args}"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ToolCallStatus {
    PendingApproval,
    Running,
    Completed,
    Failed,
    Rejected,
}

#[derive(Debug, Clone)]
pub enum Message {
    ApproveToolCall(String),
    RejectToolCall(String),
}

impl ToolCallEntry {
    pub fn approve(&mut self) {
        self.status = ToolCallStatus::Running;
    }

    pub fn reject(&mut self) {
        self.status = ToolCallStatus::Rejected;
    }

    pub fn needs_approval(&self) -> bool {
        matches!(self.status, ToolCallStatus::PendingApproval)
    }

    pub fn status_label(&self) -> &'static str {
        match &self.status {
            ToolCallStatus::PendingApproval => "Needs approval",
            ToolCallStatus::Running => "Running...",
            ToolCallStatus::Completed => "Done",
            ToolCallStatus::Failed => "Failed",
            ToolCallStatus::Rejected => "Rejected",
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = column![
            row![
                text(self.status_label()),
                text(format!(
                    "{}",
                    self.tool_name.replace("_", " ").to_uppercase()
                ))
            ]
            .spacing(8),
            container(self.view_args()).padding([4, 6]),
            self.view_error(),
            row![horizontal(), self.view_actions()],
        ]
        .spacing(8)
        .into();

        container(body)
            .padding(8)
            .width(Length::Fixed(500.0))
            .style(container::primary)
            .into()
    }

    fn view_args(&self) -> Element<'_, Message> {
        match self.tool_details {
            Ok(ref tool_details) => tool_details.view().into(),
            Err(_) => text(self.args.as_str()).size(11).into(),
        }
    }

    fn view_error(&self) -> Element<'_, Message> {
        match self.error {
            Some(ref err) => text(format!("Error: {err}"))
                .size(11)
                .style(text::danger)
                .into(),
            None => space().into(),
        }
    }

    fn view_actions(&self) -> Element<'_, Message> {
        if let ToolCallStatus::PendingApproval = &self.status {
            row![
                button(text("Approve").size(12))
                    .on_press(Message::ApproveToolCall(self.call_id.clone())),
                button(text("Reject").size(12))
                    .on_press(Message::RejectToolCall(self.call_id.clone()))
            ]
            .spacing(8)
            .into()
        } else {
            space().into()
        }
    }
}
