use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

pub const APP_ID: &str = "1550300883965841560";
pub const LARGE_IMAGE: &str = "chinchou";
pub const LARGE_TEXT: &str = "harmony";
pub const IDLE_DETAILS: &str = "idle";
const MIN_FIELD: usize = 2;
const TICK: Duration = Duration::from_millis(500);
const RETRY_GAP: Duration = Duration::from_secs(15);
const SEND_GAP: Duration = Duration::from_secs(4);

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Presence {
    pub details: String,
    pub state: String,
    pub large_text: String,
    pub since: Option<i64>,
}

pub fn usable(text: &str) -> bool {
    text.chars().count() >= MIN_FIELD
}

/// A count short enough for a presence line: `847`, `18.5k`, `118k`, `1.4m`.
///
/// Discord gives a line about forty characters before it truncates, so the
/// exact figure the window shows would eat it. This is the only place harmony
/// rounds a count.
pub fn compact(value: usize) -> String {
    match value {
        0..=999 => value.to_string(),
        1_000..=999_999 => {
            let thousands = value as f32 / 1_000.0;
            match thousands < 100.0 {
                true => format!("{thousands:.1}k"),
                false => format!("{}k", thousands.round() as usize),
            }
        }
        _ => {
            let millions = value as f32 / 1_000_000.0;
            match millions < 100.0 {
                true => format!("{millions:.1}m"),
                false => format!("{}m", millions.round() as usize),
            }
        }
    }
}

pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max.saturating_sub(1)).collect::<String>() + "\u{2026}"
}

impl Presence {
    fn activity(&self) -> Activity<'_> {
        let details = if usable(&self.details) {
            self.details.as_str()
        } else {
            IDLE_DETAILS
        };
        let large_text = if usable(&self.large_text) {
            self.large_text.as_str()
        } else {
            LARGE_TEXT
        };
        let mut activity = Activity::new()
            .details(details)
            .assets(Assets::new().large_image(LARGE_IMAGE).large_text(large_text));
        if usable(&self.state) {
            activity = activity.state(self.state.as_str());
        }
        if let Some(since) = self.since {
            activity = activity.timestamps(Timestamps::new().start(since));
        }
        activity
    }
}

pub struct Rpc {
    tx: Option<Sender<Option<Presence>>>,
    linked: Arc<AtomicBool>,
    last: Option<Option<Presence>>,
}

impl Rpc {
    pub fn new(app_id: &str) -> Rpc {
        if app_id.trim().is_empty() {
            return Rpc::disabled();
        }
        let (tx, rx) = channel();
        let linked = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&linked);
        let id = app_id.trim().to_string();
        let spawned = thread::Builder::new()
            .name("harmony-discord".to_string())
            .spawn(move || run(&id, rx, flag));
        Rpc {
            tx: spawned.ok().map(|_| tx),
            linked,
            last: None,
        }
    }

    pub fn disabled() -> Rpc {
        Rpc {
            tx: None,
            linked: Arc::new(AtomicBool::new(false)),
            last: None,
        }
    }

    pub fn is_live(&self) -> bool {
        self.tx.is_some()
    }

    pub fn is_linked(&self) -> bool {
        self.tx.is_some() && self.linked.load(Ordering::Relaxed)
    }

    pub fn set(&mut self, presence: Presence) {
        self.send(Some(presence));
    }

    pub fn clear(&mut self) {
        self.send(None);
    }

    fn send(&mut self, wanted: Option<Presence>) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        if self.last.as_ref() == Some(&wanted) {
            return;
        }
        if tx.send(wanted.clone()).is_err() {
            self.tx = None;
            return;
        }
        self.last = Some(wanted);
    }
}

impl Default for Rpc {
    fn default() -> Rpc {
        Rpc::disabled()
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

fn run(app_id: &str, rx: Receiver<Option<Presence>>, linked: Arc<AtomicBool>) {
    let mut client = DiscordIpcClient::new(app_id);
    let mut connected = false;
    let mut next_try = Instant::now();
    let mut last_send: Option<Instant> = None;
    let mut wanted: Option<Presence> = None;
    let mut sent: Option<Option<Presence>> = None;

    loop {
        let mut closing = match rx.recv_timeout(TICK) {
            Ok(next) => {
                wanted = next;
                false
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => true,
        };
        while !closing {
            match rx.try_recv() {
                Ok(next) => wanted = next,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => closing = true,
            }
        }

        if !connected && !closing {
            if Instant::now() < next_try {
                continue;
            }
            match client.connect() {
                Ok(()) => {
                    connected = true;
                    sent = None;
                    linked.store(true, Ordering::Relaxed);
                }
                Err(_) => {
                    next_try = Instant::now() + RETRY_GAP;
                    continue;
                }
            }
        }

        let stale = sent.as_ref() != Some(&wanted);
        let spaced = closing || last_send.is_none_or(|at| at.elapsed() >= SEND_GAP);
        if connected && stale && spaced {
            let written = match wanted.as_ref() {
                Some(presence) => client.set_activity(presence.activity()),
                None => client.clear_activity(),
            };
            let answered = written.and_then(|()| client.recv().map(|_| ()));
            match answered {
                Ok(()) => {
                    sent = Some(wanted.clone());
                    last_send = Some(Instant::now());
                }
                Err(_) => {
                    let _ = client.close();
                    connected = false;
                    sent = None;
                    linked.store(false, Ordering::Relaxed);
                    next_try = Instant::now() + RETRY_GAP;
                }
            }
        }

        if closing {
            if connected {
                let _ = client.clear_activity();
                let _ = client.close();
            }
            linked.store(false, Ordering::Relaxed);
            return;
        }
    }
}
