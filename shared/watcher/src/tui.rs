use psyche_coordinator::{Coordinator, RunState};
use psyche_core::NodeIdentity;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Paragraph, Widget},
};

#[derive(Default, Debug)]
pub struct CoordinatorTui;

impl psyche_tui::CustomWidget for CoordinatorTui {
    type Data = CoordinatorTuiState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(2, 5),
                    Constraint::Ratio(1, 5),
                    Constraint::Ratio(1, 5),
                    Constraint::Ratio(1, 5),
                ]
                .as_ref(),
            )
            .split(area);

        Paragraph::new(format!("Run state: {:?}", state.run_state)).render(chunks[0], buf);
        Paragraph::new(format!("Clients: {:?}", state.clients)).render(chunks[1], buf);
        Paragraph::new(format!("Height: {:?}", state.height)).render(chunks[2], buf);
        Paragraph::new(format!("Tick: {:?}", state.tick)).render(chunks[3], buf);
    }
}

#[derive(Default, Debug)]
pub struct CoordinatorTuiState {
    pub run_state: RunState,
    pub height: u32,
    pub clients: u32,
    pub tick: u64,
}

impl<T: NodeIdentity> From<&Coordinator<T>> for CoordinatorTuiState {
    fn from(value: &Coordinator<T>) -> Self {
        Self {
            run_state: value.run_state,
            height: value.rounds[value.rounds_head as usize].height,
            clients: value.rounds[value.rounds_head as usize].clients_len,
            tick: value.tick,
        }
    }
}
