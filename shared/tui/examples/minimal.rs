use std::time::Duration;

use psyche_tui::{init_logging, start_render_loop, CustomWidget};
use rand::RngCore;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Paragraph, Widget},
};
use tracing::{error, info, warn};

#[derive(Default)]
pub struct MinimalWidget {
    persistant_state: u64,
}

impl CustomWidget for MinimalWidget {
    type Data = u64;

    fn render(
        &mut self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        self.persistant_state += 1;
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        Paragraph::new(format!("persistant state is {}", self.persistant_state))
            .centered()
            .render(chunks[0], buf);
        Paragraph::new(format!("state passed from main thread is {}", state))
            .centered()
            .render(chunks[1], buf);
    }
}

#[allow(dead_code)]
fn main() -> anyhow::Result<()> {
    init_logging(psyche_tui::LogOutput::TUI);

    info!("foo");
    warn!("bar");
    error!("baz");

    let tx = start_render_loop(MinimalWidget::default())?;
    loop {
        let prng_num = rand::thread_rng().next_u64();
        tx.send(prng_num).expect("sending works!");
        std::thread::sleep(Duration::from_secs(2));
    }
}
