mod app;
mod discovery;
mod message;
mod net;
mod node;
mod theme;
mod view;

use app::App;

fn main() -> iced::Result {
    // SAFETY: Called at program start before any threads are spawned.
    unsafe {
        if std::env::var("RUST_BACKTRACE").is_err() {
            std::env::set_var("RUST_BACKTRACE", "1");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("tephra_client=info".parse().unwrap()),
        )
        .init();

    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size((1200.0, 800.0))
        .antialiasing(true)
        .run()
}
