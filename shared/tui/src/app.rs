use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::{
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
use tracing::{debug, trace};

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
    custom_widget: W,
    custom_widget_data_state: W::Data,
}
// TODO implement sending shutdown signal + graceful shutdown somehow..

impl<W: CustomWidget> App<W> {
    pub fn new(widget: W) -> Self {
        Self {
            mode: AppMode::Run,
            custom_widget: widget,
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

        self.custom_widget.on_ui_event(&event);

        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('q') => self.mode = AppMode::Quit,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.mode = AppMode::Quit
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        terminal.draw(|frame| {
            self.custom_widget.render(
                frame.area(),
                frame.buffer_mut(),
                &self.custom_widget_data_state,
            );
        })?;
        Ok(())
    }
}
