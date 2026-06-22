use iced::widget::{Column, button, column, container, row, scrollable, svg, text};
use iced::{Element, Length, Padding};

use crate::components::connection_item::{self, ConnectionItem};
use crate::theme;

#[derive(Debug, Clone)]
pub enum SidebarMessage {
    AddConnection,
    SelectConnection(String),
    ItemMessage(String, connection_item::ItemMessage),
}

pub fn view<'a>(items: &'a [ConnectionItem]) -> Element<'a, SidebarMessage> {
    let header = container(
        row![
            text("Connections").size(13),
            iced::widget::Space::new().width(Length::Fill),
            button(row![
                svg(svg::Handle::from_memory(include_bytes!(
                    "../resources/plus.svg"
                )))
                .width(16)
                .height(16)
                .style(|_theme, _status| svg::Style {
                    color: Some(iced::color!(255, 255, 255))
                }),
            ])
            .on_press(SidebarMessage::AddConnection)
            .style(iced::widget::button::primary),
        ]
        .align_y(iced::Alignment::Center),
    )
    .padding([10, 12]);

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
                column![
                    text("No connections yet.")
                        .size(13)
                        .color(theme::TEXT_MUTED),
                    text("Click + to add one.")
                        .size(12)
                        .color(theme::TEXT_MUTED),
                ]
                .spacing(4)
                .align_x(iced::Alignment::Center),
            )
            .padding([20, 12])
            .width(Length::Fill),
        );
    }

    container(column![
        header,
        iced::widget::rule::horizontal(1),
        scrollable(conn_list).height(Length::Fill),
    ])
    .width(Length::Fixed(260.0))
    .height(Length::Fill)
    .into()
}
