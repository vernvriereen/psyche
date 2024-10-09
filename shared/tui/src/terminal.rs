use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::{Backend, CrosstermBackend},
    Terminal,
};
use tracing::{error, trace};

pub struct TerminalWrapper<T: Backend>(pub Terminal<T>);

pub fn init_terminal() -> io::Result<TerminalWrapper<impl Backend>> {
    trace!(target:"crossterm", "Initializing terminal");
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    terminal.hide_cursor()?;
    Ok(TerminalWrapper(terminal))
}

impl<T: Backend> Drop for TerminalWrapper<T> {
    fn drop(&mut self) {
        trace!(target:"crossterm", "Restoring terminal");
        if let Err(e) = disable_raw_mode() {
            error!("failed to disable terminal raw mode: {e}");
        }
        if let Err(e) = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture) {
            error!("failed to leave alternate screen & disable mouse capture: {e}");
        }
    }
}
