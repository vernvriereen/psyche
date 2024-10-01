use psyche_coordinator::{Committee, RunState};
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{Paragraph, Widget},
};

#[derive(Default, Debug)]
pub struct ClientTUI;

impl psyche_tui::CustomWidget for ClientTUI {
    type Data = ClientTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let coord_split =
            Layout::vertical(vec![Constraint::Fill(1), Constraint::Length(2)]).split(area);
        {}
        {
            let hsplit =
                Layout::horizontal(Constraint::from_fills([1, 1, 1, 1])).split(coord_split[1]);
            Paragraph::new(format!("Step: {}", state.step)).render(hsplit[0], buf);
            Paragraph::new(format!("Height: {}", state.height)).render(hsplit[1], buf);
            Paragraph::new(format!("Run state: {:?}", state.run_state)).render(hsplit[2], buf);
            Paragraph::new(format!(
                "Committee: {}",
                state.committee.map(|x| x.to_string()).unwrap_or_default()
            ))
            .render(hsplit[3], buf);
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ClientTUIState {
    pub step: u32,
    pub height: u32,
    pub committee: Option<Committee>,
    pub run_state: RunState,
    pub loss: Vec<f32>,
}
