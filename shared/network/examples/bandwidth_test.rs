use anyhow::{bail, Result};
use chrono::{Local, Timelike};
use clap::{ArgAction, Parser};
use iroh::{
    base::ticket::BlobTicket,
    net::relay::{RelayMap, RelayMode, RelayUrl},
};
use psyche_network::{NetworkConnection, NetworkEvent, NetworkTUI, NetworkTUIState, PeerList};
use psyche_tui::{
    ratatui::{
        layout::{Constraint, Direction, Layout},
        widgets::{Block, Borders, Paragraph, Widget},
    },
    CustomWidget, LogOutput,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::{
    str::FromStr,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    select,
    time::{interval, interval_at, Interval},
};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
struct Args {
    #[clap(long)]
    secret_key: Option<String>,
    #[clap(short, long)]
    relay: Option<RelayUrl>,
    #[clap(long)]
    no_relay: bool,

    #[clap(short, long)]
    bind_port: Option<u16>,

    #[clap(
        long,
        action = ArgAction::Set,
        default_value_t = true,
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = false
    )]
    tui: bool,

    peer_list: Option<String>,
}

type NC = NetworkConnection<Message, DistroResultBlob>;

#[derive(Default, Debug)]
struct TUIState {
    network: NetworkTUIState,
    current_step: u64,
}

#[derive(Default)]
struct TUI {
    network: NetworkTUI,
}

impl CustomWidget for TUI {
    type Data = TUIState;

    fn render(
        &mut self,
        area: psyche_tui::ratatui::prelude::Rect,
        buf: &mut psyche_tui::ratatui::prelude::Buffer,
        state: &Self::Data,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // current step
                    Constraint::Max(3),
                    // network info
                    Constraint::Fill(1),
                ]
                .as_ref(),
            )
            .split(area);
        Paragraph::new(format!("Current step: {}", state.current_step))
            .block(Block::new().borders(Borders::ALL))
            .render(chunks[0], buf);
        self.network.render(chunks[1], buf, &state.network);
    }
}

#[derive(Debug)]
struct App {
    current_step: u64,
    network: NC,
    tx_tui_state: Sender<TUIState>,
    send_data_interval: Interval,
    update_tui_interval: Interval,
}

impl App {
    async fn run(&mut self) {
        loop {
            select! {
                Ok(Some(event)) = self.network.poll_next() => {
                    self.on_network_event(event);
                }
                _ = self.send_data_interval.tick() => {
                    self.on_tick().await;
                }
                _ = self.update_tui_interval.tick() => {
                    self.update_tui();
                }
                else => break,
            }
        }
    }

    fn update_tui(&mut self) {
        let tui_state = TUIState {
            current_step: self.current_step,
            network: (&self.network).into(),
        };
        self.tx_tui_state.send(tui_state).unwrap();
    }

    fn on_network_event(&mut self, event: NetworkEvent<Message, DistroResultBlob>) {
        match event {
            NetworkEvent::MessageReceived((from, Message::Message { text })) => {
                info!("[{from}]: {text}")
            }
            NetworkEvent::MessageReceived((from, Message::DistroResult { step, blob_ticket })) => {
                info!("[{from}]: step {step} blob ticket {blob_ticket}")
            }
            NetworkEvent::DownloadComplete(file) => {
                info!(
                    "Download complete! step {}: {} bytes downloaded.",
                    file.step,
                    file.data.len()
                )
            }
        }
    }
    async fn on_tick(&mut self) {
        let unix_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went forwads :)");
        let step = (unix_time.as_secs() + 2) / 15;
        info!("new step {step}");
        if step != self.current_step + 1 {
            warn!(
                "new step {step} is not 1 greater than prev step {}",
                self.current_step + 1
            );
        }

        self.current_step = step;

        const DATA_SIZE_MB: usize = 10;
        let mut data = vec![0u8; DATA_SIZE_MB * 1024 * 1024];
        rand::thread_rng().fill(&mut data[..]);

        let blob_ticket = match self
            .network
            .add_downloadable(DistroResultBlob { step, data })
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("Couldn't add downloadable for step {step}. {}", e);
                return;
            }
        };

        let message = Message::DistroResult { step, blob_ticket };

        if let Err(e) = self.network.broadcast(&message).await {
            error!("Error sending message: {}", e);
        } else {
            info!("broadcasted message for step {step}: {:?}", message);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    psyche_tui::init_logging(if args.tui {
        LogOutput::TUI
    } else {
        LogOutput::Console
    });

    let PeerList(peers) = args
        .peer_list
        .map(|p| PeerList::from_str(&p).unwrap())
        .unwrap_or_default();

    info!("joining gossip room");

    let secret_key = args.secret_key.map(|k| k.parse().unwrap());

    let relay_mode = match (args.no_relay, args.relay) {
        (false, None) => RelayMode::Default,
        (false, Some(url)) => RelayMode::Custom(RelayMap::from_url(url)),
        (true, None) => RelayMode::Disabled,
        (true, Some(_)) => bail!("You cannot set --no-relay and --relay at the same time"),
    };
    info!("using relay servers: {:?}", &relay_mode);

    let network = NC::init("123", args.bind_port, relay_mode, peers, secret_key).await?;

    let tui = args.tui;

    let tx_state = if tui {
        psyche_tui::start_render_loop::<TUI>().unwrap()
    } else {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for item in rx {
                info!("{:?}", item);
            }
        });
        tx
    };

    // fire at wall-clock 15-second intervals.
    let send_data_interval = {
        let now = Local::now();
        let seconds_until_next: u64 = 15 - (now.second() as u64 % 15);
        let start = Instant::now() + Duration::from_secs(seconds_until_next);
        interval_at(start.into(), Duration::from_secs(15))
    };

    App {
        current_step: 0,
        network,
        tx_tui_state: tx_state,
        send_data_interval,
        update_tui_interval: interval(Duration::from_millis(150)),
    }
    .run()
    .await;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
enum Message {
    Message { text: String },
    DistroResult { blob_ticket: BlobTicket, step: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
struct DistroResultBlob {
    step: u64,
    data: Vec<u8>,
}
