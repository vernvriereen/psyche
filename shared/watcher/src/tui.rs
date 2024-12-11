use std::{
    fmt::{Display, Formatter},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

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
                Paragraph::new(format!("{}", state.run_state))
                    .block(Block::bordered().title("Run state"))
                    .render(vsplit[0], buf);
            }
            {
                Paragraph::new(
                    state
                        .clients
                        .iter()
                        .map(|c| c.to_string().into())
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
                    [
                        format!("Data Source: {}", state.data_source),
                        format!("Model Checkpoint: {}", state.model_checkpoint),
                    ]
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

#[derive(Default, Debug, Clone)]
pub enum TuiRunState {
    #[default]
    Unknown,
    WaitingForMembers {
        need: u32,
    },
    Warmup {
        end_time: Instant,
    },
    RoundTrain,
    RoundWitness,
    Cooldown {
        end_time: Option<Instant>,
    },
}

impl Display for TuiRunState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TuiRunState::Unknown => write!(f, "Unknown"),
            TuiRunState::WaitingForMembers { need } => write!(f, "Waiting for {} members", need),
            TuiRunState::Warmup { end_time } => {
                let remaining = end_time.duration_since(Instant::now());
                write!(f, "Warmup ({}s remaining)", remaining.as_secs())
            }
            TuiRunState::RoundTrain => write!(f, "Training"),
            TuiRunState::RoundWitness => write!(f, "Witnessing"),
            TuiRunState::Cooldown { end_time } => match end_time {
                Some(end_time) => {
                    let remaining = end_time.duration_since(Instant::now());
                    write!(f, "Cooldown ({}s remaining)", remaining.as_secs())
                }
                None => write!(f, "Cooldown"),
            },
        }
    }
}

impl<T: NodeIdentity> From<&Coordinator<T>> for TuiRunState {
    fn from(c: &Coordinator<T>) -> Self {
        match c.run_state {
            RunState::WaitingForMembers => TuiRunState::WaitingForMembers {
                need: c.min_clients.saturating_sub(c.clients.len() as u32),
            },
            RunState::Warmup => {
                let time_since_epoch = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO);

                TuiRunState::Warmup {
                    end_time: Instant::now()
                        + Duration::from_secs(c.warmup_time + c.run_state_start_unix_timestamp)
                        - time_since_epoch,
                }
            }
            RunState::RoundTrain => TuiRunState::RoundTrain,
            RunState::RoundWitness => TuiRunState::RoundWitness,
            RunState::Cooldown => TuiRunState::Cooldown {
                end_time: match c.cooldown_time {
                    0 => None,
                    cooldown_time => {
                        let time_since_epoch = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or(Duration::ZERO);

                        Some(
                            Instant::now()
                                + Duration::from_secs(
                                    cooldown_time + c.run_state_start_unix_timestamp,
                                )
                                - time_since_epoch,
                        )
                    }
                },
            },
        }
    }
}

#[derive(Default, Debug)]
pub struct CoordinatorTuiState {
    pub run_id: String,
    pub run_state: TuiRunState,
    pub height: u32,
    pub clients: Vec<String>,
    pub tick: u64,
    pub data_source: String,
    pub model_checkpoint: String,
    pub dropped_clients: usize,
}

impl<T: NodeIdentity> From<&Coordinator<T>> for CoordinatorTuiState {
    fn from(value: &Coordinator<T>) -> Self {
        Self {
            run_id: String::from_utf8(value.run_id.clone().to_vec()).unwrap(),
            run_state: value.into(),
            height: value.rounds[value.rounds_head as usize].height,
            clients: value
                .clients
                .iter()
                .map(|c| format!("{:?}", c.id))
                .collect(),
            tick: value.tick,
            data_source: match &value.model {
                Model::LLM(l) => format!("{:?}", l.data_type),
            },
            model_checkpoint: match &value.model {
                Model::LLM(l) => format!("{:?}", l.checkpoint),
            },
            dropped_clients: value.dropped_clients.len(),
        }
    }
}
