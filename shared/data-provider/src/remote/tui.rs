use psyche_core::NodeIdentity;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};
use psyche_watcher::Backend;

use crate::TokenizedDataProvider;

use super::DataProviderTcpServer;

#[derive(Default, Debug)]
pub struct DataServerTui;

impl psyche_tui::CustomWidget for DataServerTui {
    type Data = DataServerTuiState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let coord_split = Layout::horizontal(Constraint::from_fills([1, 1])).split(area);
        {
            Paragraph::new(
                state
                    .clients
                    .iter()
                    .map(|c| {
                        let status = if c.2 { "⏳" } else { "✅" };
                        Line::from(format!("{status} [{}]: {}", c.0, c.1))
                    })
                    .collect::<Vec<Line>>(),
            )
            .block(Block::bordered().title("Clients"))
            .render(coord_split[0], buf);
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
            .render(coord_split[1], buf);
        }
    }
}

#[derive(Default, Debug)]
pub struct DataServerTuiState {
    pub height: u32,
    pub clients: Vec<(String, usize, bool)>,
    pub tick: u64,
}

impl<T, D, W> From<&DataProviderTcpServer<T, D, W>> for DataServerTuiState
where
    T: NodeIdentity,
    D: TokenizedDataProvider,
    W: Backend<T>,
{
    fn from(v: &DataProviderTcpServer<T, D, W>) -> Self {
        Self {
            height: v.state.rounds[v.state.rounds_head as usize].height,
            clients: v
                .state
                .clients
                .iter()
                .map(|c| {
                    let id = format!("{}", c.id);
                    let data_id = v.state.data_id(&c.id).unwrap_or(0);
                    let has_fetched = *v.provided_sequences.get(&data_id).unwrap_or(&false);
                    (id, data_id, has_fetched)
                })
                .collect(),
            tick: v.state.tick,
        }
    }
}
