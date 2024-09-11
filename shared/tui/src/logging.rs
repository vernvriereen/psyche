use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

pub enum LogOutput {
    TUI,
    Console,
    // todo add a file logger
}
pub fn init_logging(output: LogOutput) {
    match output {
        LogOutput::TUI => {
            let subscriber = tracing_subscriber::registry()
                .with(
                    EnvFilter::builder()
                        .with_default_directive(Level::INFO.into())
                        .from_env_lossy(),
                )
                .with(tui_logger::tracing_subscriber_layer());

            tracing::subscriber::set_global_default(subscriber)
                .expect("Unable to set global default subscriber");
        }
        LogOutput::Console => {
            let subscriber = tracing_subscriber::registry()
                .with(
                    EnvFilter::builder()
                        .with_default_directive(Level::INFO.into())
                        .from_env_lossy(),
                )
                .with(fmt::layer().with_writer(std::io::stdout));
            tracing::subscriber::set_global_default(subscriber)
                .expect("Unable to set global default subscriber");
        }
    }
}
