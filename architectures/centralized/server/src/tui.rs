use psyche_network::{NetworkTUI, NetworkTUIState};
use psyche_tui::{
    logging::LoggerWidget,
    ratatui::layout::{Constraint, Direction, Layout},
    CustomWidget,
};
use psyche_watcher::{CoordinatorTUI, CoordinatorTUIState};

#[derive(Default)]
pub struct TUIState {
    pub coordinator: CoordinatorTUIState,
    pub network: NetworkTUIState,
    pub console: (),
}

#[derive(Default)]
pub struct TUI {
    coordinator: CoordinatorTUI,
    network: NetworkTUI,
    console: LoggerWidget,
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
                    // logs
                    Constraint::Fill(1),
                ]
                .as_ref(),
            )
            .split(area);
        self.coordinator.render(chunks[0], buf, &state.coordinator);
        self.network.render(chunks[1], buf, &state.network);
        self.console.render(chunks[2], buf, &state.console);
    }
}
