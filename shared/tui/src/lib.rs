mod app;
pub mod logging;
mod maybe;
mod tabbed;
mod terminal;
mod widget;

use anyhow::Result;
use std::{
    sync::mpsc::{self, Sender},
    thread,
};
use terminal::{init_terminal, restore_terminal};

pub use app::App;
pub use logging::{init_logging, LogOutput};
pub use maybe::MaybeTui;
pub use tabbed::TabbedWidget;
pub use widget::CustomWidget;

pub fn start_render_loop<T: CustomWidget>(widget: T) -> Result<Sender<T::Data>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(|| {
        let mut terminal = init_terminal().unwrap();
        terminal.clear().unwrap();
        terminal.hide_cursor().unwrap();

        let start_result = App::new(widget).start(&mut terminal, rx);
        let restore_result = restore_terminal();
        start_result.unwrap();
        restore_result.unwrap();
    });
    Ok(tx)
}

pub use crossterm;
pub use ratatui;
