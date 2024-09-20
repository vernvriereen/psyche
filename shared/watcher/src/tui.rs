use psyche_coordinator::{model::Model, Coordinator, RunState};
use psyche_core::NodeIdentity;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

#[derive(Default, Debug)]
pub struct CoordinatorTui;

impl psyche_tui::CustomWidget for CoordinatorTui {
    type Data = CoordinatorTuiState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let coord_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(area);
        {
            let vsplit = Layout::vertical(Constraint::from_fills([1, 1])).split(coord_split[0]);
            {
                Paragraph::new(format!("{:?}", state.run_state))
                    .block(Block::bordered().title("Run state"))
                    .render(vsplit[0], buf);
            }
            {
                Paragraph::new(
                    state
                        .clients
                        .iter()
                        .map(|c| format!("{:?}", c).into())
                        .collect::<Vec<Line>>(),
                )
                .block(Block::bordered().title("Clients this round"))
                .render(vsplit[1], buf);
            }
        }
        {
            let vsplit = Layout::vertical(Constraint::from_fills([1, 1])).split(coord_split[1]);
            {
                Paragraph::new(
                    [format!("Data Source: {}", state.data_source)]
                        .into_iter()
                        .map(Line::from)
                        .collect::<Vec<_>>(),
                )
                .block(Block::bordered().title("Config"))
                .render(vsplit[0], buf);
            }
            {
                Paragraph::new(
                    [
                        format!("Clients: {:?}", state.clients.len()),
                        format!("Height: {:?}", state.height),
                        format!("Tick: {:?}", state.tick),
                    ]
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
                )
                .block(Block::bordered().title("Current state"))
                .render(vsplit[1], buf);
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct CoordinatorTuiState {
    pub run_id: String,
    pub run_state: RunState,
    pub height: u32,
    pub clients: Vec<String>,
    pub tick: u64,
    pub data_source: String,
}

impl<T: NodeIdentity> From<&Coordinator<T>> for CoordinatorTuiState {
    fn from(value: &Coordinator<T>) -> Self {
        Self {
            run_id: value.run_id.clone(),
            run_state: value.run_state,
            height: value.rounds[value.rounds_head as usize].height,
            clients: value
                .clients
                .iter()
                .map(|c| format!("{:?}", c.id))
                .collect(),
            tick: value.tick,
            data_source: value
                .model
                .as_ref()
                .and_then(|m| match m {
                    Model::LLM(l) => Some(format!("{:?}", l.data_type)),
                    #[allow(unreachable_patterns)] // can happen later! remove when there's >1 lol
                    _ => None,
                })
                .unwrap_or("no llm data source...".to_owned()),
        }
    }
}
