use iced::widget::{Column, button, row, svg, text};
use iced::{Length, Task, Theme};

use crate::types::{TreeNode, TreeNodeKind};

#[derive(Debug, Clone)]
pub enum TreeMessage {
    ToggleSchema(String),
    ToggleTableGroup(String),
    SelectTable(String, String),
}

#[derive(Debug)]
pub struct SchemaTree {
    nodes: Vec<TreeNode>,
}

impl SchemaTree {
    pub fn new(nodes: Vec<TreeNode>) -> Self {
        Self { nodes }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn update(&mut self, message: TreeMessage) -> Task<TreeMessage> {
        match message {
            TreeMessage::ToggleSchema(schema_name) => {
                for node in &mut self.nodes {
                    if node.label == schema_name && node.kind == TreeNodeKind::Schema {
                        node.expanded = !node.expanded;
                    }
                }
                Task::none()
            }
            TreeMessage::ToggleTableGroup(schema_name) => {
                for schema in &mut self.nodes {
                    if schema.label == schema_name {
                        for child in &mut schema.children {
                            if child.kind == TreeNodeKind::TableGroup {
                                child.expanded = !child.expanded;
                            }
                        }
                    }
                }
                Task::none()
            }
            TreeMessage::SelectTable(_, _) => Task::none(),
        }
    }

    pub fn view(&self) -> Column<'_, TreeMessage> {
        render_tree(&self.nodes, 0)
    }
}

fn render_tree<'a>(nodes: &'a [TreeNode], depth: usize) -> Column<'a, TreeMessage> {
    let indent = depth as f32 * 16.0;
    let mut col = Column::new().spacing(2);

    for node in nodes {
        let icon = node_icon(&node.kind);
        let label_row = row![
            iced::widget::Space::new().width(Length::Fixed(indent)),
            button(
                row![
                    icon.width(12).height(12),
                    text(node.label.as_str()).size(13),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center)
            )
            .on_press_maybe(tree_node_message(&node.kind, &node.label, &node.schema))
            .padding([4, 8])
            .width(Length::Fill)
            .style(tree_button_style),
        ]
        .spacing(0);

        col = col.push(label_row);

        if node.expanded && !node.children.is_empty() {
            col = col.push(render_tree(&node.children, depth + 1));
        }
    }

    col
}

fn node_icon(kind: &TreeNodeKind) -> svg::Svg<'_> {
    let handle = match kind {
        TreeNodeKind::Connection => {
            svg::Handle::from_memory(include_bytes!("../resources/plug.svg"))
        }
        TreeNodeKind::SchemaGroup => {
            svg::Handle::from_memory(include_bytes!("../resources/folder-open.svg"))
        }
        TreeNodeKind::Schema => svg::Handle::from_memory(include_bytes!("../resources/box.svg")),
        TreeNodeKind::TableGroup => {
            svg::Handle::from_memory(include_bytes!("../resources/table.svg"))
        }
        TreeNodeKind::Table => svg::Handle::from_memory(include_bytes!("../resources/table.svg")),
    };

    svg(handle)
}

fn tree_node_message(
    kind: &TreeNodeKind,
    label: &str,
    schema: &Option<String>,
) -> Option<TreeMessage> {
    match kind {
        TreeNodeKind::Schema => Some(TreeMessage::ToggleSchema(label.to_string())),
        TreeNodeKind::TableGroup => Some(TreeMessage::ToggleTableGroup(
            schema.clone().unwrap_or_default(),
        )),
        TreeNodeKind::Table => Some(TreeMessage::SelectTable(
            schema.clone().unwrap_or_default(),
            label.to_string(),
        )),
        _ => None,
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
