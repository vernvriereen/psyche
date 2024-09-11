use psyche_core::Networkable;
use psyche_tui::ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Padding, Paragraph,
        Widget, Wrap,
    },
};

use iroh::net::key::PublicKey;
use std::{
    collections::{HashMap, VecDeque},
    ops::Sub,
    time::Instant,
};

use crate::{peer_list::PeerList, util::convert_bytes, NetworkConnection};

#[derive(Default, Debug)]
pub struct NetworkTUI;

impl psyche_tui::CustomWidget for NetworkTUI {
    type Data = NetworkTUIState;

    fn render(&mut self, area: Rect, buf: &mut Buffer, state: &Self::Data) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [
                    // clients and join ticket
                    Constraint::Percentage(40),
                    // uploads & download
                    Constraint::Percentage(60),
                ]
                .as_ref(),
            )
            .split(area);

        // Clients and Join Ticket
        {
            let client_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(chunks[0]);

            Paragraph::new(state.join_ticket.to_string())
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .title("Join Ticket")
                        .padding(Padding::symmetric(1, 0))
                        .borders(Borders::ALL),
                )
                .render(client_chunks[0], buf);

            List::new(state.last_seen.iter().map(|(peer_id, last_seen_instant)| {
                let last_seen_time = Instant::now().sub(*last_seen_instant).as_secs_f64();
                let li = ListItem::new(format!("{}: {:.2} seconds ago", peer_id, last_seen_time));
                if last_seen_time < 1.0 {
                    li.bg(Color::LightYellow).fg(Color::Black)
                } else {
                    li
                }
            }))
            .block(
                Block::default()
                    .title("Recently Seen Peers")
                    .borders(Borders::ALL),
            )
            .render(client_chunks[1], buf);
        }

        // Upload & Download
        {
            let network_chunks =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);

            // Downloads and Download Bandwidth
            {
                let download_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                    .split(network_chunks[1]);

                List::new(state.downloads.iter().map(|(hash, download)| {
                    let percent = 100.0 * (download.downloaded as f64 / download.total as f64);
                    ListItem::new(format!(
                        "[{:02}%]{}: {}/{}",
                        percent,
                        hash,
                        convert_bytes(download.downloaded as f64),
                        convert_bytes(download.total as f64)
                    ))
                }))
                .block(
                    Block::default()
                        .title(format!("Downloads ({})", state.downloads.len()))
                        .borders(Borders::ALL),
                )
                .highlight_style(Style::default().add_modifier(Modifier::BOLD))
                .highlight_symbol(">>")
                .render(download_chunks[0], buf);

                let bw_history = state
                    .download_bandwidth_history
                    .iter()
                    .enumerate()
                    .map(|(x, y)| (x as f64, *y))
                    .collect::<Vec<_>>();

                Chart::new(vec![Dataset::default()
                    .graph_type(GraphType::Line)
                    .data(&bw_history)])
                .block(
                    Block::default()
                        .title(format!(
                            "Download Bandwidth {}/s",
                            convert_bytes(state.total_data_per_sec)
                        ))
                        .borders(Borders::ALL),
                )
                .x_axis(Axis::default().title("Time").labels(vec!["0", "30", "60"]))
                .y_axis(Axis::default().title("Bytes/s)").labels(vec![
                    convert_bytes(0.0),
                    convert_bytes(5.0 * 1024.0 * 1024.0),
                ]))
                .render(download_chunks[1], buf);
            }

            // Uploads and Upload Bandwidth
            {
                let upload_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                    .split(network_chunks[0]);

                let uploads = List::new(state.upload_hashes.iter().map(|hash| {
                    let item = ListItem::new(hash.as_str());
                    item
                }))
                .block(
                    Block::default()
                        .title(format!("Uploads ({})", state.upload_hashes.len()))
                        .borders(Borders::ALL),
                );

                uploads.render(upload_chunks[0], buf);

                // Placeholder for Upload Bandwidth
                let upload_bandwidth = Paragraph::new("Upload Bandwidth Graph (Placeholder)")
                    .block(
                        Block::default()
                            .title("Upload Bandwidth")
                            .borders(Borders::ALL),
                    );
                upload_bandwidth.render(upload_chunks[1], buf);
            }
        }
    }
}

#[derive(Default, Debug)]
pub struct UIDownloadProgress {
    downloaded: u64,
    total: u64,
}

#[derive(Default, Debug)]
pub struct NetworkTUIState {
    pub join_ticket: PeerList,
    pub last_seen: HashMap<PublicKey, Instant>,
    // pub data_per_sec_per_client: HashMap<PublicKey, f64>,
    pub total_data_per_sec: f64,
    pub download_bandwidth_history: VecDeque<f64>,

    pub downloads: HashMap<String, UIDownloadProgress>,

    pub upload_hashes: Vec<String>,
}

impl<M, D> From<&NetworkConnection<M, D>> for NetworkTUIState
where
    M: Networkable,
    D: Networkable,
{
    fn from(nc: &NetworkConnection<M, D>) -> Self {
        let s = &nc.state;
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
            upload_hashes: s
                .currently_sharing_blobs
                .iter()
                .rev()
                .map(|blob| blob.hash().to_string())
                .collect(),
        }
    }
}
