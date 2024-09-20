use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Paragraph, Widget},
};

use crate::CustomWidget;

pub struct MaybeTui<T: CustomWidget> {
    empty_string: String,
    t: T,
}

impl<T: CustomWidget + Default> Default for MaybeTui<T> {
    fn default() -> Self {
        Self {
            empty_string: "no data :(".to_owned(),
            t: Default::default(),
        }
    }
}

impl<T: CustomWidget> CustomWidget for MaybeTui<T> {
    type Data = Option<T::Data>;

    fn render(
        &mut self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        if let Some(state) = state {
            self.t.render(area, buf, state);
        } else {
            render_not_found(&self.empty_string, area, buf)
        }
    }
}

fn render_not_found(empty: &str, area: Rect, buf: &mut Buffer) {
    Paragraph::new(empty)
        .centered()
        .block(Block::bordered())
        .render(area, buf);
}
