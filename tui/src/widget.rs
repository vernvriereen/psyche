use ratatui::{buffer::Buffer, layout::Rect};

pub trait CustomWidget: Default {
    type Data: Default + Send + 'static;
    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data);
}
