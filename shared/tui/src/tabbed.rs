use crate::CustomWidget;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Tabs, Widget},
};
use std::marker::PhantomData;

pub struct TabbedWidget<T: CustomWidgetTuple> {
    widgets: T,
    current_tab: usize,
    tab_titles: Vec<String>,
    _phantom: PhantomData<T::Data>,
}

pub trait CustomWidgetTuple: Send + 'static {
    type Data: Default + Send + 'static;
    fn len(&self) -> usize;
    fn render_at(&mut self, index: usize, area: Rect, buf: &mut Buffer, state: &Self::Data);
    fn on_ui_event_at(&mut self, index: usize, event: &Event);
}

impl<T: CustomWidgetTuple> TabbedWidget<T> {
    pub fn new<S: ToString>(widgets: T, tab_titles: &[S]) -> Self {
        Self {
            widgets,
            current_tab: 0,
            tab_titles: tab_titles.iter().map(|x| x.to_string()).collect(),
            _phantom: PhantomData,
        }
    }

    fn get_tab_from_key(&self, code: &KeyCode) -> Option<usize> {
        match code {
            KeyCode::Char(c) => c.to_digit(10).map(|d| d as usize - 1),
            _ => None,
        }
    }

    fn render_tab_bar(&self, area: Rect, buf: &mut Buffer) {
        let tabs = Tabs::new(
            self.tab_titles
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    Span::styled(
                        format!("[{}] {t}", i + 1),
                        Style::default().fg(Color::White),
                    )
                })
                .collect::<Vec<_>>(),
        )
        .select(self.current_tab)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|");

        let block = Block::default().borders(Borders::BOTTOM);
        tabs.block(block).render(area, buf);
    }
}

impl<T: CustomWidgetTuple> CustomWidget for TabbedWidget<T> {
    type Data = T::Data;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(area);

        self.render_tab_bar(chunks[0], buf);
        self.widgets
            .render_at(self.current_tab, chunks[1], buf, state);
    }

    fn on_ui_event(&mut self, event: &Event) {
        if let Event::Key(KeyEvent { code, .. }) = event {
            if let Some(new_tab) = self.get_tab_from_key(code) {
                if new_tab < self.widgets.len() {
                    self.current_tab = new_tab;
                    return;
                }
            }
        }
        self.widgets.on_ui_event_at(self.current_tab, event);
    }
}

// NOTE:
// I cannot, for the life of me, figure out how to write this as a macro that knows which index an item is in the tuple.
// I'll just copy-paste for now. lol.

impl<T1> CustomWidgetTuple for (T1,)
where
    T1: CustomWidget,
{
    type Data = (T1::Data,);

    fn len(&self) -> usize {
        1
    }

    fn render_at(&mut self, index: usize, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let (t1,) = self;
        if index == 0 {
            t1.render(area, buf, &state.0);
        }
    }

    fn on_ui_event_at(&mut self, index: usize, event: &Event) {
        let (t1,) = self;
        if index == 0 {
            t1.on_ui_event(event)
        }
    }
}

impl<T1, T2> CustomWidgetTuple for (T1, T2)
where
    T1: CustomWidget,
    T2: CustomWidget,
{
    type Data = (T1::Data, T2::Data);

    fn len(&self) -> usize {
        2
    }

    fn render_at(&mut self, index: usize, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let (t1, t2) = self;
        match index {
            0 => {
                t1.render(area, buf, &state.0);
            }
            1 => {
                t2.render(area, buf, &state.1);
            }
            _ => {}
        }
    }

    fn on_ui_event_at(&mut self, index: usize, event: &Event) {
        let (t1, t2) = self;
        match index {
            0 => t1.on_ui_event(event),
            1 => t2.on_ui_event(event),
            _ => {}
        }
    }
}

impl<T1, T2, T3> CustomWidgetTuple for (T1, T2, T3)
where
    T1: CustomWidget,
    T2: CustomWidget,
    T3: CustomWidget,
{
    type Data = (T1::Data, T2::Data, T3::Data);

    fn len(&self) -> usize {
        3
    }

    fn render_at(&mut self, index: usize, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let (t1, t2, t3) = self;
        match index {
            0 => {
                t1.render(area, buf, &state.0);
            }
            1 => {
                t2.render(area, buf, &state.1);
            }
            2 => {
                t3.render(area, buf, &state.2);
            }
            _ => {}
        }
    }

    fn on_ui_event_at(&mut self, index: usize, event: &Event) {
        let (t1, t2, t3) = self;
        match index {
            0 => t1.on_ui_event(event),
            1 => t2.on_ui_event(event),
            2 => t3.on_ui_event(event),
            _ => {}
        }
    }
}

impl<T1, T2, T3, T4> CustomWidgetTuple for (T1, T2, T3, T4)
where
    T1: CustomWidget,
    T2: CustomWidget,
    T3: CustomWidget,
    T4: CustomWidget,
{
    type Data = (T1::Data, T2::Data, T3::Data, T4::Data);

    fn len(&self) -> usize {
        4
    }

    fn render_at(&mut self, index: usize, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let (t1, t2, t3, t4) = self;
        match index {
            0 => t1.render(area, buf, &state.0),
            1 => t2.render(area, buf, &state.1),
            2 => t3.render(area, buf, &state.2),
            3 => t4.render(area, buf, &state.3),
            _ => {}
        }
    }

    fn on_ui_event_at(&mut self, index: usize, event: &Event) {
        let (t1, t2, t3, t4) = self;
        match index {
            0 => t1.on_ui_event(event),
            1 => t2.on_ui_event(event),
            2 => t3.on_ui_event(event),
            3 => t4.on_ui_event(event),
            _ => {}
        }
    }
}

impl<T1, T2, T3, T4, T5> CustomWidgetTuple for (T1, T2, T3, T4, T5)
where
    T1: CustomWidget,
    T2: CustomWidget,
    T3: CustomWidget,
    T4: CustomWidget,
    T5: CustomWidget,
{
    type Data = (T1::Data, T2::Data, T3::Data, T4::Data, T5::Data);

    fn len(&self) -> usize {
        5
    }

    fn render_at(&mut self, index: usize, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let (t1, t2, t3, t4, t5) = self;
        match index {
            0 => t1.render(area, buf, &state.0),
            1 => t2.render(area, buf, &state.1),
            2 => t3.render(area, buf, &state.2),
            3 => t4.render(area, buf, &state.3),
            4 => t5.render(area, buf, &state.4),
            _ => {}
        }
    }

    fn on_ui_event_at(&mut self, index: usize, event: &Event) {
        let (t1, t2, t3, t4, t5) = self;
        match index {
            0 => t1.on_ui_event(event),
            1 => t2.on_ui_event(event),
            2 => t3.on_ui_event(event),
            3 => t4.on_ui_event(event),
            4 => t5.on_ui_event(event),
            _ => {}
        }
    }
}
