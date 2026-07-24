mod app;
mod components;
mod connection_manager;
mod core;
mod db;
mod theme;
mod types;
mod ui;

use iced::Size;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::core::config_loader;

fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("pgeru")
        .join("logs");

    let file_appender = tracing_appender::rolling::daily(log_dir, "app.log");

    let (non_blocking_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_writer)
        .with_ansi(false);

    let stdout_layer = tracing_subscriber::fmt::layer().pretty();
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn,pgeru=debug"));
    tracing_subscriber::registry()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer)
        .init();

    guard
}

fn app_theme(_state: &app::App) -> iced::Theme {
    theme::create()
}

fn app_init() -> (app::App, iced::Task<app::Message>) {
    let app = app::App::default();
    let task = iced::Task::perform(
        async {
            tokio::task::spawn_blocking(|| config_loader::load_config())
                .await
                .unwrap()
                .unwrap_or_default()
        },
        |config| app::Message::ConfigLoaded(config),
    );
    (app, task)
}

fn main() -> iced::Result {
    let _guard = init_logging();
    iced::application(app_init, app::App::update, app::App::view)
        .title("Pgeru")
        .theme(app_theme)
        .window(iced::window::Settings {
            size: Size::new(1920.0, 1080.0),
            min_size: Some(Size::new(800.0, 500.0)),
            decorations: false,
            resizable: true,
            position: iced::window::Position::Centered,
            ..Default::default()
        })
        .centered()
        .scale_factor(|state| 1.0 + (state.zoom_multiplier as f32) * 0.125)
        .antialiasing(true)
        .subscription(|app| {
            iced::Subscription::batch([
                app.key_press_handler(),
                app.save_subscription(),
                app.window_event_subscription(),
            ])
        })
        .run()
}
