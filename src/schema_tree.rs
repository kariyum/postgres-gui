use iced::widget::{button, row, text, Column};
use iced::{Length, Theme};

use crate::app::Message;
use crate::theme;
use crate::types::{TreeNode, TreeNodeKind};

/// Recursively render a tree node and its children.
pub fn render_tree<'a>(
    nodes: &'a [TreeNode],
    depth: usize,
    conn_id: &'a str,
) -> Column<'a, Message> {
    let indent = depth as f32 * 16.0;
    let mut col = Column::new().spacing(2);

    for node in nodes {
        let icon = node_icon(&node.kind);
        let expand_icon = if node.children.is_empty() {
            " "
        } else if node.expanded {
            "▾"
        } else {
            "▸"
        };

        let label_row = row![
            // Indentation spacer
            iced::widget::Space::new().width(Length::Fixed(indent)),
            button(
                row![
                    text(expand_icon).size(12).color(theme::TEXT_MUTED),
                    text(icon).size(14),
                    text(node.label.as_str()).size(13),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center)
            )
            .on_press(tree_node_message(&node.kind, conn_id, &node.label, &node.schema))
            .padding([4, 8])
            .width(Length::Fill)
            .style(tree_button_style),
        ]
        .spacing(0);

        col = col.push(label_row);

        if node.expanded && !node.children.is_empty() {
            col = col.push(render_tree(&node.children, depth + 1, conn_id));
        }
    }

    col
}

fn node_icon(kind: &TreeNodeKind) -> &'static str {
    match kind {
        TreeNodeKind::Connection => "🔌",
        TreeNodeKind::SchemaGroup => "📁",
        TreeNodeKind::Schema => "🗂 ",
        TreeNodeKind::TableGroup => "📋",
        TreeNodeKind::Table => "▦ ",
    }
}

fn tree_node_message(
    kind: &TreeNodeKind,
    conn_id: &str,
    label: &str,
    schema: &Option<String>,
) -> Message {
    match kind {
        TreeNodeKind::Connection => Message::ToggleConnectionTree(conn_id.to_string()),
        TreeNodeKind::Schema => {
            Message::ToggleSchemaNode(conn_id.to_string(), label.to_string())
        }
        TreeNodeKind::TableGroup => Message::ToggleTableGroup(
            conn_id.to_string(),
            schema.clone().unwrap_or_default(),
        ),
        TreeNodeKind::Table => {
            let schema_name = schema.clone().unwrap_or_default();
            Message::SelectTable(conn_id.to_string(), schema_name, label.to_string())
        }
        _ => Message::Noop,
    }
}

fn tree_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(palette.primary.weak.color.into()),
            text_color: palette.primary.weak.text,
            border: iced::Border::default(),
            ..Default::default()
        },
        _ => button::Style {
            background: None,
            text_color: palette.background.base.text,
            border: iced::Border::default(),
            ..Default::default()
        },
    }
}
