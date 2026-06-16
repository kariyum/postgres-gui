mod app;
mod connection_dialog;
mod db;
mod db_config;
mod schema_tree;
mod types;

use iced::{Size, Theme};

fn app_theme(_state: &app::App) -> Theme {
    Theme::TokyoNight
}

fn main() -> iced::Result {
    iced::application(app::App::default, app::App::update, app::App::view)
        .title("Pgeru")
        .theme(app_theme)
        .window(iced::window::Settings {
            size: Size::new(1280.0, 800.0),
            min_size: Some(Size::new(800.0, 500.0)),
            ..Default::default()
        })
        .antialiasing(true)
        .run()
}
