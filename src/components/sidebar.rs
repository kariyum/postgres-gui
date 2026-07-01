use iced::widget::{Column, button, column, container, row, scrollable, svg, text};
use iced::{Element, Length, Padding};

use crate::components::connection_item::{self, ConnectionItem};
use crate::theme;

#[derive(Debug, Clone)]
pub enum SidebarMessage {
    SelectConnection(String),
    ItemMessage(String, connection_item::ItemMessage),
}

pub fn view<'a>(items: &'a [ConnectionItem]) -> Element<'a, SidebarMessage> {
    let mut conn_list = Column::new().spacing(4).padding(Padding::from([0, 0]));

    for item in items {
        let id = item.cfg.id.clone();
        let view: Element<'_, connection_item::ItemMessage> = item.view().into();
        let view = view.map(move |msg| SidebarMessage::ItemMessage(id.clone(), msg));
        conn_list = conn_list.push(view);
    }

    if items.is_empty() {
        conn_list = conn_list.push(
            container(
                text("No connections yet.")
                    .size(13)
                    .color(theme::TEXT_MUTED),
            )
            .padding([20, 12])
            .width(Length::Fill),
        );
    }

    container(column![scrollable(conn_list).height(Length::Fill),])
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .into()
}
