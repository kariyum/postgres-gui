use iced::widget::{Row, button, container, row, text};
use iced::{Element, Length, Theme};

use crate::components::connection_item::{self, ConnectionItem};
use crate::theme;

#[derive(Debug, Clone)]
pub enum TabBarMessage {
    SelectTab(String),
    CloseTab(String),
    ItemMessage(String, connection_item::ItemMessage),
}

pub fn view<'a>(
    items: &'a [ConnectionItem],
    active_connection: &Option<String>,
) -> Element<'a, TabBarMessage> {
    let mut tabs_row = Row::new().align_y(iced::Alignment::Center);

    for item in items {
        if item.pool.is_none() {
            continue;
        }

        let is_active = active_connection.as_deref() == Some(&item.cfg.id);
        let id = item.cfg.id.clone();
        let name = item.cfg.name.clone();

        let tab_btn = button(
            row![
                text("🔌").size(12),
                text(name).size(13),
                iced::widget::Space::new().width(Length::Fixed(4.0)),
                button(text("✕").size(10))
                    .on_press(TabBarMessage::CloseTab(id.clone()))
                    .padding([1, 4])
                    .style(iced::widget::button::text),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center),
        )
        .on_press(TabBarMessage::SelectTab(id.clone()))
        .padding([4, 8])
        .style(move |theme: &Theme, _status| {
            let palette = theme.extended_palette();
            if is_active {
                button::Style {
                    background: Some(palette.background.base.color.into()),
                    text_color: palette.background.base.text,
                    border: iced::Border {
                        width: 1.0,
                        color: palette.primary.base.color,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            } else {
                button::Style {
                    background: Some(palette.background.weak.color.into()),
                    text_color: palette.background.weak.text,
                    border: iced::Border {
                        width: 1.0,
                        color: palette.background.strong.color,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }
            }
        });

        tabs_row = tabs_row.push(tab_btn);
    }

    if items.iter().all(|i| i.pool.is_none()) {
        tabs_row = tabs_row.push(
            text("  Connect to a database to get started")
                .size(12)
                .color(theme::TEXT_MUTED),
        );
    }

    container(tabs_row)
        .height(Length::Fixed(38.0))
        .width(Length::Fill)
        .padding([4, 8])
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            iced::widget::container::Style {
                background: Some(palette.background.weak.color.into()),
                ..Default::default()
            }
        })
        .into()
}
