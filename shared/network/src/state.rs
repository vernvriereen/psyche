use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
    time::{Duration, Instant},
};

use iroh::{base::ticket::BlobTicket, blobs::Hash, net::key::PublicKey};

use crate::{download_manager::DownloadUpdate, peer_list::PeerList};

#[derive(Debug)]
pub struct State {
    pub join_ticket: PeerList,
    pub last_seen: HashMap<PublicKey, Instant>,
    pub bandwidth_tracker: BandwidthTracker,
    pub bandwidth_history: VecDeque<f64>,
    pub download_progesses: HashMap<Hash, DownloadUpdate>,

    pub currently_sharing_blobs: Vec<BlobTicket>,
}

impl State {
    pub fn new(bandwidth_average_period: u64) -> Self {
        Self {
            join_ticket: Default::default(),
            last_seen: Default::default(),
            bandwidth_tracker: BandwidthTracker::new(bandwidth_average_period),
            bandwidth_history: Default::default(),
            download_progesses: Default::default(),
            currently_sharing_blobs: Default::default(),
        }
    }
}

#[derive(Debug)]
struct DownloadEvent {
    timestamp: Instant,
    num_bytes: u64,
}

#[derive(Debug)]
pub struct BandwidthTracker {
    average_period_secs: u64,
    events: VecDeque<DownloadEvent>,
    total_bytes: u64,
}

impl BandwidthTracker {
    pub fn new(average_period_secs: u64) -> Self {
        BandwidthTracker {
            average_period_secs,
            events: VecDeque::new(),
            total_bytes: 0,
        }
    }

    pub fn add_event(&mut self, num_bytes: u64) {
        let now = Instant::now();
        self.events.push_back(DownloadEvent {
            timestamp: now,
            num_bytes,
        });
        self.total_bytes += num_bytes;

        while let Some(event) = self.events.front() {
            if now.duration_since(event.timestamp) > Duration::from_secs(self.average_period_secs) {
                if let Some(removed_event) = self.events.pop_front() {
                    self.total_bytes -= removed_event.num_bytes;
                }
            } else {
                break;
            }
        }
    }

    pub fn get_bandwidth(&self) -> f64 {
        if self.events.is_empty() {
            return 0.0;
        }

        let duration = self
            .events
            .back()
            .unwrap()
            .timestamp
            .duration_since(self.events.front().unwrap().timestamp);
        let seconds = duration.as_secs_f64();

        if seconds > 0.0 {
            self.total_bytes as f64 / seconds
        } else {
            0.0
        }
    }
}
