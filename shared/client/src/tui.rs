use psyche_coordinator::Committee;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    symbols,
    widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Widget},
};
use psyche_watcher::TuiRunState;

#[derive(Default, Debug)]
pub struct ClientTUI;

impl psyche_tui::CustomWidget for ClientTUI {
    type Data = ClientTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let coord_split =
            Layout::vertical(vec![Constraint::Fill(1), Constraint::Length(2)]).split(area);
        {
            let x_max = (state.step + 1) as f64;
            let x_min = x_max - (state.loss.len() as f64);
            let data = state
                .loss
                .iter()
                .enumerate()
                .map(|(i, val)| (i as f64 + x_min, *val as f64))
                .collect::<Vec<_>>();
            let y_min = 0f64.max(
                data.iter()
                    .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap())
                    .unwrap_or(&(0., 0.))
                    .1
                    - 0.1f64,
            );
            let y_max = data
                .iter()
                .max_by(|x, y| x.1.partial_cmp(&y.1).unwrap())
                .unwrap_or(&(0., 0.))
                .1
                + 0.1f64;
            let dataset = Dataset::default()
                .name("Loss")
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().cyan())
                .data(&data);
            let x_axis = Axis::default()
                .bounds([x_min, x_max])
                .style(Style::default().white());
            let y_axis = Axis::default()
                .bounds([y_min, y_max])
                .labels([format!("{y_min:.1}"), format!("{y_max:.1}")])
                .style(Style::default().white());
            Chart::new(vec![dataset])
                .x_axis(x_axis)
                .y_axis(y_axis)
                .render(coord_split[0], buf);
        }
        {
            let hsplit =
                Layout::horizontal(Constraint::from_fills([1, 1, 1, 1, 1])).split(coord_split[1]);
            Paragraph::new(format!("Step: {}", state.step)).render(hsplit[0], buf);
            Paragraph::new(format!(
                "Committee: {}",
                state.committee.map(|x| x.to_string()).unwrap_or_default()
            ))
            .render(hsplit[1], buf);
            Paragraph::new(format!("State: {}", state.run_state)).render(hsplit[2], buf);
            Paragraph::new(format!("Batches Left: {}", state.batches_left)).render(hsplit[3], buf);
            Paragraph::new(format!("Loss: {:.3}", state.loss.last().unwrap_or(&0.0)))
                .render(hsplit[4], buf);
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct ClientTUIState {
    pub step: u32,
    pub committee: Option<Committee>,
    pub run_state: TuiRunState,
    pub batches_left: usize,
    pub loss: Vec<f32>,
}
