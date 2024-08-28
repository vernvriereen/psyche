use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use iroh::net::key::PublicKey;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Stylize},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Padding, Paragraph,
        Widget, Wrap,
    },
    Terminal,
};
use std::{
    collections::{HashMap, VecDeque},
    io,
    ops::Sub,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};
use tracing::{debug, trace};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetEvent, TuiWidgetState};

use crate::{peer_list::PeerList, state::State, util::convert_bytes};
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AppMode {
    #[default]
    Run,
    Quit,
}

#[derive(Default, Debug)]
pub struct UIDownloadProgress {
    downloaded: u64,
    total: u64,
}

#[derive(Default, Debug)]
pub struct UIState {
    pub join_ticket: PeerList,
    pub last_seen: HashMap<PublicKey, Instant>,
    // pub data_per_sec_per_client: HashMap<PublicKey, f64>,
    pub total_data_per_sec: f64,
    pub download_bandwidth_history: VecDeque<f64>,

    pub downloads: HashMap<String, UIDownloadProgress>,
}

impl From<&State> for UIState {
    fn from(s: &State) -> Self {
        Self {
            join_ticket: s.join_ticket.clone(),
            last_seen: s.last_seen.clone(),
            total_data_per_sec: s.bandwidth_tracker.get_bandwidth(),
            download_bandwidth_history: s.bandwidth_history.clone(),
            downloads: s
                .download_progesses
                .iter()
                .map(|(key, dl)| {
                    (
                        key.clone(),
                        UIDownloadProgress {
                            downloaded: dl.downloaded_size,
                            total: dl.total_size,
                        },
                    )
                })
                .collect(),
        }
    }
}

struct App {
    mode: AppMode,
    logger_state: TuiWidgetState,
    psyche_state: UIState,
}

#[derive(Debug)]
enum AppEvent {
    UiEvent(Event),
    StateUpdated(UIState),
    Frame,
}

impl App {
    pub fn new() -> App {
        let logger_state = TuiWidgetState::new();
        App {
            mode: AppMode::Run,
            logger_state,
            psyche_state: Default::default(),
        }
    }

    pub fn start(
        mut self,
        terminal: &mut Terminal<impl Backend>,
        state_rx: Receiver<UIState>,
    ) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel();

        thread::spawn({
            let tx = tx.clone();
            move || loop {
                let _ = tx.send(AppEvent::Frame);
                std::thread::sleep(Duration::from_millis(150));
            }
        });
        thread::spawn({
            let tx = tx.clone();
            move || {
                while let Ok(event) = event::read() {
                    trace!(target:"crossterm", "Stdin event received {:?}", event);
                    let _ = tx.send(AppEvent::UiEvent(event));
                }
                panic!("crossterm input thread exited")
            }
        });

        thread::spawn({
            let tx = tx.clone();
            move || {
                for state in state_rx {
                    let _ = tx.send(AppEvent::StateUpdated(state));
                }
            }
        });

        self.run(terminal, rx)?;
        Ok(())
    }

    fn run(
        &mut self,
        terminal: &mut Terminal<impl Backend>,
        rx: mpsc::Receiver<AppEvent>,
    ) -> anyhow::Result<()> {
        for event in rx {
            match event {
                AppEvent::UiEvent(event) => self.handle_ui_event(event),
                AppEvent::StateUpdated(s) => self.psyche_state = s,
                AppEvent::Frame => {
                    // just render!
                }
            }
            if self.mode == AppMode::Quit {
                break;
            }
            self.draw(terminal)?;
        }
        Ok(())
    }

    fn handle_ui_event(&mut self, event: Event) {
        debug!(target: "App", "Handling UI event: {:?}",event);

        if let Event::Key(key) = event {
            match key.code {
                KeyCode::Char('q') => self.mode = AppMode::Quit,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.mode = AppMode::Quit
                }
                KeyCode::Esc => self.logger_state.transition(TuiWidgetEvent::EscapeKey),
                _ => {}
            }
        } else if let Event::Mouse(mouse) = event {
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    self.logger_state.transition(TuiWidgetEvent::PrevPageKey);
                }
                MouseEventKind::ScrollDown => {
                    self.logger_state.transition(TuiWidgetEvent::NextPageKey);
                }
                _ => {}
            }
        }
    }

    fn draw(&mut self, terminal: &mut Terminal<impl Backend>) -> anyhow::Result<()> {
        terminal.draw(|frame| {
            frame.render_widget(self, frame.area());
        })?;
        Ok(())
    }
}

impl Widget for &mut App {
    fn render(self, size: Rect, buf: &mut Buffer) {
        let block = Block::default();
        block.render(size, buf);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Max(7),
                    Constraint::Percentage(50),
                    Constraint::Fill(1),
                ]
                .as_ref(),
            )
            .split(size);

        let ticket = Paragraph::new(format!(
            "{}\n{:?}",
            self.psyche_state.join_ticket, self.psyche_state.join_ticket.0
        ))
        .wrap(Wrap { trim: true })
        .block(
            Block::new()
                .title("Join Ticket")
                .padding(Padding::symmetric(1, 0))
                .borders(Borders::ALL),
        );
        ticket.render(chunks[0], buf);

        let middle_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(chunks[1]);

        let client_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(middle_chunks[0]);

        {
            let clients = List::new(self.psyche_state.last_seen.iter().map(
                |(peer_id, last_seen_instant)| {
                    let last_seen_time = Instant::now().sub(*last_seen_instant).as_secs_f64();
                    let li =
                        ListItem::new(format!("{}: {:.2} seconds ago", peer_id, last_seen_time));
                    if last_seen_time < 1.0 {
                        li.bg(Color::LightYellow).fg(Color::Black)
                    } else {
                        li
                    }
                },
            ))
            .block(Block::default().title("Clients").borders(Borders::ALL));
            clients.render(client_chunks[0], buf);
        }

        {
            let downloads =
                List::new(self.psyche_state.downloads.iter().map(|(hash, download)| {
                    let percent = download.downloaded as f64 / download.total as f64;
                    ListItem::new(format!(
                        "[{:02}%]{hash}: {}/{}",
                        percent,
                        convert_bytes(download.downloaded as f64),
                        convert_bytes(download.total as f64)
                    ))
                }))
                .block(Block::default().title("Downloads").borders(Borders::ALL));
            downloads.render(client_chunks[1], buf);
        }

        {
            let bw_history = self
                .psyche_state
                .download_bandwidth_history
                .iter()
                .enumerate()
                .map(|(x, y)| (x as f64, *y))
                .collect::<Vec<_>>();

            let bandwidth_graph = Chart::new(vec![Dataset::default()
                .graph_type(GraphType::Line)
                .data(&bw_history)])
            .block(
                Block::default()
                    .title(format!(
                        "Download Bandwidth {}/s",
                        convert_bytes(self.psyche_state.total_data_per_sec)
                    ))
                    .borders(Borders::ALL),
            )
            .x_axis(Axis::default().title("Time").labels(vec!["0", "30", "60"]))
            .y_axis(Axis::default().title("Bytes/s)").labels(vec![
                convert_bytes(0.0),
                convert_bytes(5.0 * 1024.0 * 1024.0),
            ]));
            bandwidth_graph.render(middle_chunks[1], buf);
        }

        {
            TuiLoggerWidget::default()
                .block(Block::bordered().title("Logs"))
                .output_separator('|')
                .output_timestamp(Some("%H:%M:%S%.3f".to_string()))
                .output_level(Some(TuiLoggerLevelOutput::Long))
                .output_target(false)
                .output_file(false)
                .output_line(false)
                .render(chunks[2], buf);
        }
    }
}

fn init_terminal() -> io::Result<Terminal<impl Backend>> {
    trace!(target:"crossterm", "Initializing terminal");
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(io::stdout());
    Terminal::new(backend)
}

fn restore_terminal() -> io::Result<()> {
    trace!(target:"crossterm", "Restoring terminal");
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
}
pub fn start_render_loop(state: Receiver<UIState>) -> Result<()> {
    debug!(target:"App", "Logging initialized");

    let mut terminal = init_terminal()?;
    terminal.clear()?;
    terminal.hide_cursor()?;

    App::new().start(&mut terminal, state)?;

    restore_terminal()?;

    Ok(())
}
