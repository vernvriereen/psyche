use psyche_coordinator::{Coordinator, NodeIdentity, RunState};
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Paragraph, Widget},
};

#[derive(Default, Debug)]
pub struct CoordinatorTUI;

impl psyche_tui::CustomWidget for CoordinatorTUI {
    type Data = CoordinatorTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Ratio(1, 4),
                    Constraint::Ratio(1, 4),
                    Constraint::Ratio(1, 4),
                    Constraint::Ratio(1, 4),
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
pub struct CoordinatorTUIState {
    run_state: RunState,
    height: u32,
    clients: u32,
    tick: u64,
}

impl<T: NodeIdentity> From<&Coordinator<T>> for CoordinatorTUIState {
    fn from(value: &Coordinator<T>) -> Self {
        Self {
            run_state: value.run_state,
            height: value.rounds[value.rounds_head as usize].height,
            clients: value.rounds[value.rounds_head as usize].clients_len,
            tick: value.tick,
        }
    }
}
