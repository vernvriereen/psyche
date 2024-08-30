use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Margin},
    widgets::{Block, Widget},
    Terminal,
};
use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
use tracing::{debug, trace};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetEvent, TuiWidgetState};

use crate::widget::CustomWidget;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AppMode {
    #[default]
    Run,
    Quit,
}

#[derive(Debug)]
enum AppEvent<S> {
    UiEvent(Event),
    StateUpdated(S),
    Frame,
}

pub struct App<W: CustomWidget> {
    mode: AppMode,
    logger_state: TuiWidgetState,
    custom_widget: W,
    custom_widget_data_state: W::Data,
}
// TODO implement sending shutdown signal + graceful shutdown somehow..
impl<W: CustomWidget> Default for App<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: CustomWidget> App<W> {
    pub fn new() -> Self {
        Self {
            mode: AppMode::Run,
            logger_state: TuiWidgetState::new(),
            custom_widget: Default::default(),
            custom_widget_data_state: Default::default(),
        }
    }

    pub fn start(
        mut self,
        terminal: &mut Terminal<impl Backend>,
        state_rx: Receiver<W::Data>,
    ) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel();

        // TODO these 3 threads make quitting weird, should detect Quit event somehow.
        thread::spawn({
            let tx = tx.clone();
            move || loop {
                if tx.send(AppEvent::Frame).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        });

        thread::spawn({
            let tx = tx.clone();
            move || {
                while let Ok(event) = event::read() {
                    trace!(target:"crossterm", "Stdin event received {:?}", event);
                    if tx.send(AppEvent::UiEvent(event)).is_err() {
                        return;
                    }
                }
                panic!("crossterm input thread exited")
            }
        });

        thread::spawn({
            let tx = tx.clone();
            move || {
                for state in state_rx {
                    if tx.send(AppEvent::StateUpdated(state)).is_err() {
                        return;
                    }
                }
            }
        });

        for event in rx {
            match event {
                AppEvent::UiEvent(event) => self.handle_ui_event(event),
                AppEvent::StateUpdated(s) => {
                    self.custom_widget_data_state = s;
                }
                AppEvent::Frame => {
                    // just render!
                }
            }
            if self.mode == AppMode::Quit {
                break;
            }
            self.draw(terminal)?;
        }
        Ok(())
    }

    fn handle_ui_event(&mut self, event: Event) {
        debug!(target: "App", "Handling UI event: {:?}",event);

        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('q') => self.mode = AppMode::Quit,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.mode = AppMode::Quit
                }
                KeyCode::Esc => self.logger_state.transition(TuiWidgetEvent::EscapeKey),
                _ => {}
            }
        } else if let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.logger_state.transition(TuiWidgetEvent::PrevPageKey);
                }
                MouseEventKind::ScrollDown => {
                    self.logger_state.transition(TuiWidgetEvent::NextPageKey);
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        // custom widget
                        Constraint::Percentage(80),
                        // logs
                        Constraint::Fill(1),
                    ]
                    .as_ref(),
                )
                .split(frame.area());
            self.custom_widget.render(
                chunks[0],
                frame.buffer_mut(),
                &self.custom_widget_data_state,
            );
            let log_area = chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            });
            TuiLoggerWidget::default()
                .block(Block::bordered().title("Logs"))
                .output_separator('|')
                .output_timestamp(Some("%H:%M:%S%.3f".to_string()))
                .output_level(Some(TuiLoggerLevelOutput::Long))
                .output_target(false)
                .output_file(false)
                .output_line(false)
                .render(log_area, frame.buffer_mut());
        })?;
        Ok(())
    }
}
