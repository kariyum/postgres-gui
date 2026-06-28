mod app;
mod components;
mod connection_manager;
mod core;
mod db;
mod db_config;
mod theme;
mod types;
mod ui;

use iced::Size;

fn app_theme(_state: &app::App) -> iced::Theme {
    theme::create()
}

fn app_init() -> (app::App, iced::Task<app::Message>) {
    let app = app::App::default();
    let task = iced::Task::perform(
        async {
            tokio::task::spawn_blocking(|| crate::db_config::load_connections())
                .await
                .unwrap_or_default()
        },
        |configs| {
            app::Message::ConnManager(
                crate::connection_manager::ConnManagerMessage::ConnectionsLoaded(configs),
            )
        },
    );
    (app, task)
}

fn main() -> iced::Result {
    iced::application(app_init, app::App::update, app::App::view)
        .title("Pgeru")
        .theme(app_theme)
        .window(iced::window::Settings {
            size: Size::new(1280.0, 800.0),
            min_size: Some(Size::new(800.0, 500.0)),
            decorations: false,
            resizable: true,
            ..Default::default()
        })
        .centered()
        .scale_factor(|state| 1.0 + (state.zoom_multiplier as f32) * 0.125)
        .antialiasing(true)
        .subscription(|app| app.key_press_handler())
        .run()
}
