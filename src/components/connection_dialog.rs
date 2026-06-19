use iced::widget::{Column, Row, TextInput, button, column, container, row, text, text_input};
use iced::{Color, Element, Length, Task, Theme};

use crate::core::connection_config::ConnectionConfig;
use crate::theme;
use crate::ui::input_field::{self, InputField, InputFieldMessage};

#[derive(Debug, Clone)]
pub struct ConnectionDialog {
    pub visible: bool,
    pub editing_id: Option<String>,
    pub cfg: ConnectionConfig,
    pub form: Form,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DialogMessage {
    DialogNameField(InputFieldMessage),
    DialogHostField(InputFieldMessage),
    DialogPortField(InputFieldMessage),
    DialogUserField(InputFieldMessage),
    DialogPasswordField(InputFieldMessage),
    DialogDatabaseField(InputFieldMessage),
    DialogSave,
    DialogCancel,
}

#[derive(Debug, Clone)]
pub struct Form {
    name: InputField,
    host: InputField,
    port: InputField,
    user: InputField,
    password: InputField,
    database: InputField,
}

impl Form {
    fn new(cfg: ConnectionConfig) -> Self {
        Self {
            name: InputField::default()
                .placeholder(String::from("Connection Name"))
                .label(String::from("Connection Name"))
                .value(cfg.name.clone())
                .secure(false),

            host: InputField::default()
                .placeholder(String::from("localhost"))
                .label(String::from("Host"))
                .value(cfg.host.clone()),

            port: InputField::default()
                .placeholder(String::from("5432"))
                .label(String::from("Port"))
                .value(cfg.port.to_string().clone())
                .width(Length::Fixed(90.0)),

            user: InputField::default()
                .placeholder(String::from("postgres"))
                .label(String::from("User"))
                .value(cfg.user.clone()),

            password: InputField::default()
                .label(String::from("Password"))
                .value(cfg.password.clone())
                .secure(true),

            database: InputField::default()
                .placeholder(String::from("postgres_db"))
                .label(String::from("Database"))
                .value(cfg.database.clone()),
        }
    }
}

impl Default for ConnectionDialog {
    fn default() -> Self {
        let cfg = ConnectionConfig::default();
        Self {
            visible: false,
            editing_id: None,
            error: None,
            form: Form::new(cfg.clone()),
            cfg,
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
        self.error = None;
        self.cfg = cfg.clone();
        self.form = Form::new(self.cfg.clone())
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.editing_id = None;
        self.error = None;
    }

    /// Validate and build a ConnectionConfig; returns Err with message on failure.
    pub fn build_config(&self) -> Result<ConnectionConfig, String> {
        if self.form.name.value.trim().is_empty() {
            return Err("Connection name cannot be empty.".to_string());
        }
        if self.form.host.value.trim().is_empty() {
            return Err("Host cannot be empty.".to_string());
        }
        let port: u16 = self
            .form
            .port
            .value
            .trim()
            .parse()
            .map_err(|_| "Port must be a number.".to_string())?;
        if self.form.user.value.trim().is_empty() {
            return Err("User cannot be empty.".to_string());
        }
        if self.form.database.value.trim().is_empty() {
            return Err("Database cannot be empty.".to_string());
        }

        let mut cfg = ConnectionConfig::new(
            self.cfg.name.trim().to_string(),
            self.cfg.host.trim().to_string(),
            port,
            self.cfg.user.trim().to_string(),
            self.cfg.password.clone(),
            self.cfg.database.trim().to_string(),
        );

        if let Some(ref id) = self.editing_id {
            cfg.id = id.clone();
        }

        Ok(cfg)
    }

    pub fn view(&self) -> Option<Element<'_, DialogMessage>> {
        if !self.visible {
            return None;
        }

        let title_str = if self.editing_id.is_some() {
            "Edit Connection"
        } else {
            "New Connection"
        };

        let mut form = column![
            text(title_str).size(18),
            iced::widget::rule::horizontal(1),
            self.form.name.view().map(DialogMessage::DialogNameField),
            row![
                self.form.host.view().map(DialogMessage::DialogHostField),
                self.form.port.view().map(DialogMessage::DialogPortField),
            ]
            .spacing(12),
            self.form
                .database
                .view()
                .map(DialogMessage::DialogDatabaseField),
            self.form.user.view().map(DialogMessage::DialogUserField),
            self.form
                .password
                .view()
                .map(DialogMessage::DialogPasswordField),
        ]
        .spacing(14)
        .padding(24)
        .width(Length::Fixed(440.0));

        if let Some(ref err) = self.error {
            form = form.push(text(err.as_str()).size(13).color(theme::DANGER));
        }

        let actions: Row<DialogMessage> = row![
            button(text("Cancel").size(14))
                .on_press(DialogMessage::DialogCancel)
                .padding([8, 18])
                .style(iced::widget::button::secondary),
            button(text("Save").size(14))
                .on_press(DialogMessage::DialogSave)
                .padding([8, 18]),
        ]
        .spacing(10);

        form = form.push(actions);
        Some(
            container(form)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    container::Style {
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
                .into(),
        )
    }

    pub fn update(&mut self, message: DialogMessage) -> Task<DialogMessage> {
        match message {
            DialogMessage::DialogNameField(msg) => self
                .form
                .name
                .update(msg)
                .map(DialogMessage::DialogNameField),
            DialogMessage::DialogHostField(msg) => self
                .form
                .host
                .update(msg)
                .map(DialogMessage::DialogHostField),
            DialogMessage::DialogPortField(msg) => self
                .form
                .port
                .update(msg)
                .map(DialogMessage::DialogPortField),
            DialogMessage::DialogUserField(msg) => self
                .form
                .user
                .update(msg)
                .map(DialogMessage::DialogUserField),
            DialogMessage::DialogPasswordField(msg) => self
                .form
                .password
                .update(msg)
                .map(DialogMessage::DialogPasswordField),
            DialogMessage::DialogDatabaseField(msg) => self
                .form
                .database
                .update(msg)
                .map(DialogMessage::DialogDatabaseField),
            DialogMessage::DialogSave => todo!(),
            DialogMessage::DialogCancel => {
                self.close();
                Task::none()
            }
        }
    }
}
