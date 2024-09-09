use psyche_network::{NetworkTUI, NetworkTUIState};
use psyche_tui::{
    ratatui::layout::{Constraint, Direction, Layout},
    CustomWidget,
};
use psyche_watcher::tui::{CoordinatorTUI, CoordinatorTUIState};

#[derive(Default, Debug)]
pub struct TUIState {
    pub coordinator: CoordinatorTUIState,
    pub network: NetworkTUIState,
}

#[derive(Default)]
pub struct TUI {
    coordinator: CoordinatorTUI,
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
                    // coordinator
                    Constraint::Max(1),
                    // network info
                    Constraint::Fill(1),
                ]
                .as_ref(),
            )
            .split(area);
        self.coordinator.render(chunks[0], buf, &state.coordinator);
        self.network.render(chunks[1], buf, &state.network);
    }
}
