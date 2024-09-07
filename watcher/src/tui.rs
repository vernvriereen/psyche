use psyche_coordinator::coordinator::{Coordinator, RunState};
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};

#[derive(Default, Debug)]
pub struct CoordinatorTUI;

impl psyche_tui::CustomWidget for CoordinatorTUI {
    type Data = CoordinatorTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        Paragraph::new(format!("{:?}", state.run_state)).render(area, buf);
    }
}

#[derive(Default, Debug)]
pub struct CoordinatorTUIState {
    run_state: RunState,
}

impl<T> From<&Coordinator<T>> for CoordinatorTUIState {
    fn from(value: &Coordinator<T>) -> Self {
        Self {
            run_state: value.run_state,
        }
    }
}
