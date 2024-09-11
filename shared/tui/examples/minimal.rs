use std::time::{Duration, SystemTime, UNIX_EPOCH};

use psyche_tui::{init_logging, start_render_loop, CustomWidget};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Paragraph, Widget},
};
use tracing::{error, info, warn};

#[derive(Default)]
struct MinimalWidget {
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

fn main() -> anyhow::Result<()> {
    init_logging(psyche_tui::LogOutput::TUI);

    info!("foo");
    warn!("bar");
    error!("baz");

    let tx = start_render_loop::<MinimalWidget>()?;
    loop {
        let prng_num = pseudo_rand();
        tx.send(prng_num as u64).expect("sending works!");
        std::thread::sleep(Duration::from_secs(2));
    }
}

fn pseudo_rand() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time goes forwards")
        .as_nanos()
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}
