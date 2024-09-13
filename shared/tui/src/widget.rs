use crossterm::event::Event;
use ratatui::{buffer::Buffer, layout::Rect};

pub trait CustomWidget: Send + 'static {
    type Data: Default + Send + 'static;
    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data);
    fn on_ui_event(&mut self, event: &Event) {
        let _ = event;
    }
}
