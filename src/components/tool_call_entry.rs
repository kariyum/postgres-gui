use anyhow::Context;
use iced::border::Radius;
use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Color, Element, Length, Theme};
use serde::Serialize;

use crate::components::agent_chat::AgentChatMessage;
use crate::core::agent_tools::connect_to_database::ConnectToDatabaseArgs;
use crate::core::agent_tools::describe_table::DescribeTableArgs;
use crate::core::agent_tools::execute_sql::ExecuteSqlArgs;
use crate::core::agent_tools::explain_query::ExplainQueryArgs;
use crate::core::agent_tools::list_connections::ListConnectionsArgs;
use crate::core::agent_tools::list_schemas::ListSchemasArgs;
use crate::core::agent_tools::list_tables::ListTablesArgs;
use crate::core::agent_tools::show_table_stats::ShowTableStatsArgs;

#[derive(Clone, Debug)]
pub struct ToolCallEntry {
    pub call_id: String,
    pub tool_name: String,
    pub args: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub status: ToolCallStatus,
    pub tool_details: Result<ToolDetails, String>,
}

#[derive(Clone, Debug, Serialize)]
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

#[derive(Clone, Debug)]
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
}

impl ToolArgs {
    fn new(tool_name: &str, args: String) -> anyhow::Result<ToolArgs> {
        let result = match tool_name {
            "connect_to_database" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ConnectToDatabase),
            "describe_table" => serde_json::from_str(args.as_str())
                .map(ToolArgs::DescribeTable),
            "execute_sql" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ExecuteSQL),
            "explain_query" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ExplainQuery),
            "list_connections" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ListConnections),
            "list_schemas" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ListSchemas),
            "list_tables" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ListTables),
            "show_table_stats" => serde_json::from_str(args.as_str())
                .map(ToolArgs::ShowTableStats),
            other => anyhow::bail!("Unknown tool: {other}"),
        };
        result.context(format!("Failed to deserialize {tool_name} {args}"))
    }
}

#[derive(Clone, Debug)]
pub enum ToolCallStatus {
    PendingApproval,
    Running,
    Completed,
    Failed,
    Rejected,
}

impl ToolCallEntry {
    pub fn status_label(&self) -> &'static str {
        match &self.status {
            ToolCallStatus::PendingApproval => "Needs approval",
            ToolCallStatus::Running => "Running...",
            ToolCallStatus::Completed => "Done",
            ToolCallStatus::Failed => "Failed",
            ToolCallStatus::Rejected => "Rejected",
        }
    }

    pub fn view(&self) -> Element<'_, AgentChatMessage> {
        let mut children: Vec<Element<'_, AgentChatMessage>> = vec![
            row![
                text(format!(
                    "{}",
                    self.tool_name.replace("_", " ").to_uppercase()
                ))
                .size(13),
                iced::widget::space::horizontal(),
                text(self.status_label()).size(11),
            ]
            .spacing(8)
            .into(),
            container(text(&self.args).size(11))
                .padding([4, 6])
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.2))),
                    border: Border {
                        color: Color::from_rgba(0.5, 0.5, 0.8, 0.2),
                        width: 1.0,
                        radius: Radius::new(4.0),
                    },
                    ..Default::default()
                })
                .into(),
        ];

        if let Some(result) = &self.result {
            children.push(
                container(text(result).size(11))
                    .padding([4, 6])
                    .style(|_: &Theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(0.0, 0.2, 0.0, 0.15))),
                        border: Border {
                            color: Color::from_rgba(0.3, 0.8, 0.3, 0.3),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if let Some(error) = &self.error {
            children.push(
                container(text(error).size(11).color(Color::from_rgb(1.0, 0.3, 0.3)))
                    .padding([4, 6])
                    .style(|_: &Theme| container::Style {
                        background: Some(Background::Color(Color::from_rgba(0.3, 0.0, 0.0, 0.15))),
                        border: Border {
                            color: Color::from_rgba(1.0, 0.3, 0.3, 0.3),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    })
                    .into(),
            );
        }

        if let ToolCallStatus::PendingApproval = &self.status {
            children.push(
                row![
                    button(
                        text("Approve")
                            .size(12)
                            .color(Color::from_rgb(0.2, 0.8, 0.2))
                    )
                    .on_press(AgentChatMessage::ApproveToolCall(self.call_id.clone()))
                    .style(|_theme, _status| button::Style {
                        background: Some(Background::Color(Color::from_rgba(0.0, 0.3, 0.0, 0.3,))),
                        border: Border {
                            color: Color::from_rgba(0.2, 0.8, 0.2, 0.5),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    }),
                    button(
                        text("Reject")
                            .size(12)
                            .color(Color::from_rgb(1.0, 0.3, 0.3))
                    )
                    .on_press(AgentChatMessage::RejectToolCall(self.call_id.clone()))
                    .style(|_theme, _status| button::Style {
                        background: Some(Background::Color(Color::from_rgba(0.3, 0.0, 0.0, 0.3,))),
                        border: Border {
                            color: Color::from_rgba(1.0, 0.3, 0.3, 0.5),
                            width: 1.0,
                            radius: Radius::new(4.0),
                        },
                        ..Default::default()
                    }),
                ]
                .spacing(8)
                .into(),
            );
        }

        container(column(children).spacing(4))
            .padding(8)
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba(0.15, 0.15, 0.25, 0.6))),
                border: Border {
                    color: Color::from_rgba(0.5, 0.5, 0.9, 0.3),
                    width: 1.0,
                    radius: Radius::new(6.0),
                },
                ..Default::default()
            })
            .width(Length::Fixed(500.0))
            .into()
    }
}
