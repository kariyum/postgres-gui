use iced::widget::{
    button, column, container, row, text, text_input, Column, Row,
};
use iced::{Color, Element, Length, Theme};

use crate::app::Message;
use crate::theme;
use crate::types::ConnectionConfig;

/// State for the "Add / Edit Connection" modal panel.
#[derive(Debug, Clone)]
pub struct ConnectionDialog {
    pub visible: bool,
    pub editing_id: Option<String>,
    // form fields
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
    pub error: Option<String>,
}

impl Default for ConnectionDialog {
    fn default() -> Self {
        let cfg = ConnectionConfig::default();
        Self {
            visible: false,
            editing_id: None,
            name: cfg.name,
            host: cfg.host,
            port: cfg.port.to_string(),
            user: cfg.user,
            password: cfg.password,
            database: cfg.database,
            error: None,
        }
    }
}

impl ConnectionDialog {
    pub fn open_new(&mut self) {
        *self = Self::default();
        self.visible = true;
    }

    pub fn open_edit(&mut self, cfg: &ConnectionConfig) {
        self.visible = true;
        self.editing_id = Some(cfg.id.clone());
        self.name = cfg.name.clone();
        self.host = cfg.host.clone();
        self.port = cfg.port.to_string();
        self.user = cfg.user.clone();
        self.password = cfg.password.clone();
        self.database = cfg.database.clone();
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.editing_id = None;
        self.error = None;
    }

    /// Validate and build a ConnectionConfig; returns Err with message on failure.
    pub fn build_config(&self) -> Result<ConnectionConfig, String> {
        if self.name.trim().is_empty() {
            return Err("Connection name cannot be empty.".to_string());
        }
        if self.host.trim().is_empty() {
            return Err("Host cannot be empty.".to_string());
        }
        let port: u16 = self
            .port
            .trim()
            .parse()
            .map_err(|_| "Port must be a valid number (1-65535).".to_string())?;
        if self.user.trim().is_empty() {
            return Err("User cannot be empty.".to_string());
        }
        if self.database.trim().is_empty() {
            return Err("Database cannot be empty.".to_string());
        }

        let mut cfg = ConnectionConfig::new(
            self.name.trim().to_string(),
            self.host.trim().to_string(),
            port,
            self.user.trim().to_string(),
            self.password.clone(),
            self.database.trim().to_string(),
        );

        if let Some(ref id) = self.editing_id {
            cfg.id = id.clone();
        }

        Ok(cfg)
    }

    pub fn view(&self) -> Element<'_, Message> {
        if !self.visible {
            return container(column![]).into();
        }

        let title_str = if self.editing_id.is_some() {
            "Edit Connection"
        } else {
            "New Connection"
        };

        let mk_field = |label: String,
                        placeholder: String,
                        value: String,
                        msg: fn(String) -> Message|
         -> Column<Message> {
            column![
                text(label).size(12).color(theme::TEXT_MUTED),
                text_input(placeholder.as_str(), value.as_str())
                    .on_input(msg)
                    .padding(8)
                    .size(14),
            ]
            .spacing(4)
        };

        let password_field: Column<Message> = column![
            text("Password").size(12).color(theme::TEXT_MUTED),
            text_input("password", &self.password)
                .on_input(Message::DialogPasswordChanged)
                .secure(true)
                .padding(8)
                .size(14),
        ]
        .spacing(4);

        let mut form: Column<Message> = column![
            text(title_str).size(18),
            iced::widget::rule::horizontal(1),
            mk_field("Connection Name".into(), "My DB".into(), self.name.clone(), Message::DialogNameChanged),
            row![
                mk_field("Host".into(), "localhost".into(), self.host.clone(), Message::DialogHostChanged)
                    .width(Length::Fill),
                mk_field("Port".into(), "5432".into(), self.port.clone(), Message::DialogPortChanged)
                    .width(Length::Fixed(90.0)),
            ]
            .spacing(12),
            mk_field(
                "Database".into(),
                "postgres".into(),
                self.database.clone(),
                Message::DialogDatabaseChanged
            ),
            mk_field("User".into(), "postgres".into(), self.user.clone(), Message::DialogUserChanged),
            password_field,
        ]
        .spacing(14)
        .padding(24)
        .width(Length::Fixed(440.0));

        if let Some(ref err) = self.error {
            form = form.push(
                text(err.as_str())
                    .size(13)
                    .color(theme::DANGER),
            );
        }

        let actions: Row<Message> = row![
            button(text("Cancel").size(14))
                .on_press(Message::DialogCancel)
                .padding([8, 18])
                .style(iced::widget::button::secondary),
            button(text("Save").size(14))
                .on_press(Message::DialogSave)
                .padding([8, 18]),
        ]
        .spacing(10);

        form = form.push(actions);

        container(form)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                iced::widget::container::Style {
                    background: Some(palette.background.base.color.into()),
                    border: iced::Border {
                        color: palette.background.strong.color,
                        width: 1.0,
                        radius: 10.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                        offset: iced::Vector::new(0.0, 8.0),
                        blur_radius: 24.0,
                    },
                    ..Default::default()
                }
            })
            .into()
    }
}
