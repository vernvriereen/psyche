use std::{fs::OpenOptions, path::PathBuf};

use crossterm::event::{Event, KeyCode, MouseEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Widget},
};
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Layer};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetEvent, TuiWidgetState};

use crate::CustomWidget;

pub enum LogOutput {
    TUI,
    Console,
    Json,
}

pub fn init_logging(output: LogOutput, level: Level, write_logs_file: Option<PathBuf>) {
    let subscriber = tracing_subscriber::registry().with(
        EnvFilter::builder()
            .with_default_directive(level.into())
            .from_env_lossy(),
    );

    let subscriber = match output {
        LogOutput::TUI => subscriber.with(tui_logger::tracing_subscriber_layer().boxed()),
        LogOutput::Console => subscriber.with(fmt::layer().with_writer(std::io::stdout).boxed()),
        LogOutput::Json => subscriber.with(
            fmt::layer()
                .json()
                .with_ansi(true)
                .with_writer(std::io::stdout)
                .flatten_event(true)
                .with_current_span(true)
                .boxed(),
        ),
    };

    if let Some(dir) = write_logs_file {
        let log_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(dir)
            .unwrap();
        let subscriber = subscriber.with(fmt::layer().with_ansi(false).with_writer(log_file));

        tracing::subscriber::set_global_default(subscriber)
    } else {
        tracing::subscriber::set_global_default(subscriber)
    }
    .expect("Unable to set global default subscriber");
}

#[derive(Default)]
pub struct LoggerWidget {
    state: TuiWidgetState,
    separator: Option<char>,
    timestamp_format: Option<String>,
    show_target: Option<bool>,
}

impl LoggerWidget {
    pub fn new() -> Self {
        Self {
            state: TuiWidgetState::new(),
            separator: None,
            timestamp_format: None,
            show_target: None,
        }
    }

    pub fn with_separator(mut self, separator: char) -> Self {
        self.separator = Some(separator);
        self
    }

    pub fn with_timestamp_format(mut self, format: String) -> Self {
        self.timestamp_format = Some(format);
        self
    }

    pub fn with_show_target_field(mut self, show: bool) -> Self {
        self.show_target = Some(show);
        self
    }
}

impl CustomWidget for LoggerWidget {
    type Data = ();

    fn on_ui_event(&mut self, event: &Event) {
        match event {
            Event::Key(key) => {
                if key.code == KeyCode::Esc {
                    self.state.transition(TuiWidgetEvent::EscapeKey);
                }
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.state.transition(TuiWidgetEvent::PrevPageKey);
                }
                MouseEventKind::ScrollDown => {
                    self.state.transition(TuiWidgetEvent::NextPageKey);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, _state: &Self::Data) {
        let mut widget = TuiLoggerWidget::default()
            .block(Block::bordered().title("Logs"))
            .output_level(Some(TuiLoggerLevelOutput::Long))
            .output_file(false)
            .output_line(false)
            .state(&self.state);

        if let Some(separator) = self.separator {
            widget = widget.output_separator(separator);
        }

        if let Some(timestamp_format) = &self.timestamp_format {
            widget = widget.output_timestamp(Some(timestamp_format.clone()));
        }

        if let Some(show_target) = self.show_target {
            widget = widget.output_target(show_target);
        }

        widget.render(area, buf);
    }
}
