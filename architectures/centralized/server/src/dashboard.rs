use psyche_tui::{
    ratatui::{
        layout::{Constraint, Direction, Layout},
        text::Line,
        widgets::{Block, Paragraph, Widget},
    },
    CustomWidget,
};
use psyche_watcher::CoordinatorTuiState;

#[derive(Default)]
pub struct DashboardState {
    pub server_addr: String,
    pub coordinator_state: CoordinatorTuiState,
}
#[derive(Default)]
pub struct DashboardTui;
impl CustomWidget for DashboardTui {
    type Data = DashboardState;

    fn render(
        &mut self,
        area: psyche_tui::ratatui::prelude::Rect,
        buf: &mut psyche_tui::ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        let vertical = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).split(area);

        {
            let title_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(vertical[0]);

            Paragraph::new(state.server_addr.clone())
                .block(Block::bordered().title("Server Address"))
                .render(title_split[0], buf);
            Paragraph::new(state.coordinator_state.run_id.clone())
                .block(Block::bordered().title("Run ID"))
                .render(title_split[1], buf);
        }
        {
            let coord_split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(Constraint::from_fills([1, 1]))
                .split(vertical[1]);

            Paragraph::new(format!("{:?}", state.coordinator_state.run_state))
                .block(Block::bordered().title("Run state"))
                .render(coord_split[0], buf);

            Paragraph::new(
                [
                    format!("Clients: {:?}", state.coordinator_state.clients.len()),
                    format!("Height: {:?}", state.coordinator_state.height),
                    format!("Tick: {:?}", state.coordinator_state.tick),
                ]
                .into_iter()
                .map(Line::from)
                .collect::<Vec<_>>(),
            )
            .block(Block::bordered().title("Coordinator info"))
            .render(coord_split[1], buf);
        }
    }
}
