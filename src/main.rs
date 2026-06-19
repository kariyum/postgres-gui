mod app;
mod components;
mod core;
mod db;
mod db_config;
mod schema_tree;
mod theme;
mod types;
mod ui;

use iced::Size;

fn app_theme(_state: &app::App) -> iced::Theme {
    theme::create()
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
        .scale_factor(|state| 1.0 + (state.zoom_multiplier as f32) * 0.125)
        .antialiasing(true)
        .run()
}
