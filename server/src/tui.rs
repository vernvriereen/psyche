use psyche_network::{NetworkTUI, NetworkTUIState};
use psyche_tui::{
    ratatui::{
        layout::{Constraint, Direction, Layout},
        widgets::{Block, Borders, Paragraph, Widget},
    },
    CustomWidget
};

#[derive(Default, Debug)]
pub struct TUIState {
    pub network: NetworkTUIState,
    pub current_step: u64,
}

#[derive(Default)]
pub struct TUI {
    network: NetworkTUI,
}

impl CustomWidget for TUI {
    type Data = TUIState;

    fn render(
        &mut self,
        area: psyche_tui::ratatui::prelude::Rect,
        buf: &mut psyche_tui::ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // current step
                    Constraint::Max(3),
                    // network info
                    Constraint::Fill(1),
                ]
                .as_ref(),
            )
            .split(area);
        Paragraph::new(format!("Current step: {}", state.current_step))
            .block(Block::new().borders(Borders::ALL))
            .render(chunks[0], buf);
        self.network.render(chunks[1], buf, &state.network);
    }
}