use iced::border::Radius;
use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Color, Element, Theme};

use crate::components::agent_chat::AgentChatMessage;

#[derive(Clone, Debug)]
pub struct ToolCallEntry {
    pub call_id: String,
    pub tool_name: String,
    pub args: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub status: ToolCallStatus,
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
    pub fn icon(&self) -> &'static str {
        match self.tool_name.as_str() {
            "execute_sql" => "\u{1F5C4}\u{FE0F}",
            "list_schemas" | "list_tables" => "\u{1F4CB}",
            "describe_table" => "\u{1F50D}",
            "explain_query" => "\u{1F4CA}",
            "show_table_stats" => "\u{1F4C8}",
            _ => "\u{1F527}",
        }
    }

    pub fn status_label(&self) -> &'static str {
        match &self.status {
            ToolCallStatus::PendingApproval => "\u{26A0}\u{FE0F} Needs approval",
            ToolCallStatus::Running => "\u{23F3} Running...",
            ToolCallStatus::Completed => "\u{2705} Done",
            ToolCallStatus::Failed => "\u{274C} Failed",
            ToolCallStatus::Rejected => "\u{1F6AB} Rejected",
        }
    }

    pub fn view(&self) -> Element<'_, AgentChatMessage> {
        let mut children: Vec<Element<'_, AgentChatMessage>> = vec![
            row![
                text(format!("{} {}", self.icon(), self.tool_name)).size(13),
                iced::widget::space::horizontal(),
                text(self.status_label())
                    .size(11)
                    .color(Color::from_rgba(0.7, 0.7, 0.9, 1.0,)),
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
            .max_width(500)
            .into()
    }
}
