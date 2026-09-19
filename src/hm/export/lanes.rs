//! Export lanes: more than one queue, running side by side.
//!
//! One line of exports is the right shape for one disk and one library — the
//! runs go in order, the machine is not fought over, and what is going is
//! obvious. It is the wrong shape for two games on two drives, or for a small
//! job that should not have to sit behind an afternoon of writing.
//!
//! So harmony keeps lanes. Lane one is the queue that has always been there
//! and everything lands in it by default; a split opens another, with its own
//! window, its own line and its own threads, and as many can be opened as
//! there is work for. The worker count is divided between the lanes that are
//! open, so two lanes are two halves of the machine rather than two machines.
//!
//! Anything written against the old single queue still works: `Lanes` derefs
//! to lane one, which is what `state.queue.progress` has always meant.

use std::ops::Deref;
use std::sync::atomic::Ordering;

use crate::hm::console::{self, Channel, Level};
use crate::hm::export::queue::{LANES, Queue, Run};
use crate::hm::storage::Resume;

/// One queue, and what the window needs to know about it.
pub struct Lane {
    /// 1 for the first, counting up. It is what the window is titled and what
    /// the log calls the lane, so it never changes once handed out.
    pub id: usize,
    pub queue: Queue,
    /// Whether the window for this lane is on screen.
    pub open: bool,
}

impl Lane {
    fn new(id: usize) -> Lane {
        Lane {
            id,
            queue: Queue::in_lane(id),
            open: true,
        }
    }

    /// Nothing going and nothing waiting.
    pub fn idle(&self) -> bool {
        self.queue
            .progress
            .lock()
            .map(|lock| !lock.running && lock.waiting.is_empty())
            .unwrap_or(true)
    }

    /// What this lane is doing, in one line: for the crash report and the
    /// console's `queue` command.
    pub fn line(&self) -> String {
        let Ok(lock) = self.queue.progress.lock() else {
            return format!("lane {}: unreadable", self.id);
        };
        if !lock.running && lock.total == 0 {
            return format!("lane {}: idle", self.id);
        }
        // A lane that has been handed work but has not started it yet has no
        // label of its own: say what it is about to do rather than nothing.
        if lock.total == 0 && !lock.waiting.is_empty() {
            return format!(
                "lane {}: about to start {} ({} waiting)",
                self.id,
                lock.waiting[0].label,
                lock.waiting.len()
            );
        }
        format!(
            "lane {}: {} - {} of {} written, {} failed, {} waiting{}",
            self.id,
            match lock.label.is_empty() {
                true => "an export".to_string(),
                false => lock.label.clone(),
            },
            lock.done,
            lock.total,
            lock.failed,
            lock.waiting.len(),
            match lock.running {
                true => "",
                false => " (finished)",
            }
        )
    }
}

/// Where a run went.
pub struct Sent {
    pub lane: usize,
    /// True when it started straight away rather than joining a line.
    pub started: bool,
}

pub struct Lanes {
    list: Vec<Lane>,
    next: usize,
}

impl Default for Lanes {
    fn default() -> Lanes {
        Lanes {
            list: vec![Lane::new(1)],
            next: 2,
        }
    }
}

/// Lane one is what harmony has always meant by "the queue", so everything
/// written against it keeps working.
impl Deref for Lanes {
    type Target = Queue;

    fn deref(&self) -> &Queue {
        &self.list[0].queue
    }
}

impl Lanes {
    pub fn list(&self) -> &[Lane] {
        &self.list
    }

    pub fn find(&self, id: usize) -> Option<&Lane> {
        self.list.iter().find(|lane| lane.id == id)
    }

    pub fn find_mut(&mut self, id: usize) -> Option<&mut Lane> {
        self.list.iter_mut().find(|lane| lane.id == id)
    }

    pub fn count(&self) -> usize {
        self.list.len()
    }

    /// Open another lane.
    ///
    /// The threads are shared out again as it opens: a second lane does not
    /// double the work the machine is asked to do, it halves what each lane
    /// gets. The run already going keeps the pool it started with — changing
    /// it under a hundred thousand sounds would be worse than uneven.
    pub fn split(&mut self) -> usize {
        let id = self.next;
        self.next += 1;
        self.list.push(Lane::new(id));
        LANES.store(self.list.len(), Ordering::Relaxed);
        console::note(
            Channel::Export,
            Level::Info,
            format!("opened lane {id}"),
            format!("{} lanes, {} threads each", self.list.len(), Queue::workers()),
        );
        id
    }

    /// Close a lane that has nothing to do. Lane one never closes: it is where
    /// everything lands by default.
    pub fn close(&mut self, id: usize) -> bool {
        if id == 1 {
            return false;
        }
        let Some(at) = self.list.iter().position(|lane| lane.id == id) else {
            return false;
        };
        if !self.list[at].idle() {
            return false;
        }
        self.list.remove(at);
        LANES.store(self.list.len().max(1), Ordering::Relaxed);
        console::info(Channel::Export, format!("closed lane {id}"));
        true
    }

    /// Send a run off.
    ///
    /// Without a split it goes to lane one and waits its turn there, which is
    /// what a line is for. With one it goes to whichever lane is free, or to a
    /// lane opened for it, and starts at once beside whatever else is running.
    pub fn send(&mut self, run: Run, split: bool) -> Sent {
        if !split {
            let started = self.list[0].queue.submit(run);
            return Sent { lane: 1, started };
        }
        let free = self
            .list
            .iter()
            .find(|lane| lane.idle())
            .map(|lane| lane.id);
        let id = match free {
            Some(id) => id,
            None => self.split(),
        };
        let started = match self.find_mut(id) {
            Some(lane) => {
                lane.open = true;
                lane.queue.submit(run)
            }
            None => self.list[0].queue.submit(run),
        };
        Sent { lane: id, started }
    }

    /// Take a run out of one lane's line and start it in another, so something
    /// that was waiting behind an afternoon of writing can be got on with now.
    pub fn split_off(&mut self, from: usize, run: u64) -> Option<usize> {
        let taken = self.find(from)?.queue.take(run)?;
        let sent = self.send(taken, true);
        console::info(
            Channel::Export,
            format!("moved a waiting export from lane {from} to lane {}", sent.lane),
        );
        Some(sent.lane)
    }

    /// Is anything at all going on?
    pub fn any_running(&self) -> bool {
        self.list.iter().any(|lane| {
            lane.queue
                .progress
                .lock()
                .map(|lock| lock.running)
                .unwrap_or(false)
        })
    }

    /// Written, failed, asked for, and how many runs are still waiting, added
    /// up over every lane.
    pub fn totals(&self) -> (usize, usize, usize, usize) {
        let mut sums = (0, 0, 0, 0);
        for lane in &self.list {
            let Ok(lock) = lane.queue.progress.lock() else {
                continue;
            };
            if lock.running {
                sums.0 += lock.done;
                sums.1 += lock.failed;
                sums.2 += lock.total;
            }
            sums.3 += lock.waiting.len();
        }
        sums
    }

    /// The lane worth talking about: the one that is running with the most
    /// left to do, which is what rich presence and the status line show.
    pub fn loudest(&self) -> Option<&Lane> {
        self.list
            .iter()
            .filter_map(|lane| {
                let lock = lane.queue.progress.lock().ok()?;
                match lock.running && lock.total > 0 {
                    true => Some((lane, lock.total.saturating_sub(lock.done + lock.failed))),
                    false => None,
                }
            })
            .max_by_key(|(_, left)| *left)
            .map(|(lane, _)| lane)
    }

    /// Can anything being done right now be put down and picked up later?
    pub fn resumable(&self) -> bool {
        self.list.iter().any(|lane| {
            lane.queue
                .progress
                .lock()
                .map(|lock| lock.resume.is_some())
                .unwrap_or(false)
        })
    }

    /// Every run in every lane, written down: the one going in each lane with
    /// how far it got, and everything behind them as it was sent.
    pub fn all_notes(&self) -> Vec<Resume> {
        let mut found: Vec<Resume> = Vec::new();
        for lane in &self.list {
            if let Ok(lock) = lane.queue.progress.lock()
                && let Some(note) = lock.resume.clone()
                && lock.running
            {
                found.push(Resume {
                    done: lock.done,
                    failed: lock.failed,
                    total: lock.total,
                    at: crate::hm::storage::stamp(),
                    ..note
                });
            }
            found.extend(lane.queue.notes());
        }
        found
    }

    /// Stop everything, everywhere. Shadows the single queue's `stop_all` on
    /// purpose: a window closing on four lanes means all four.
    pub fn stop_all(&self) {
        for lane in &self.list {
            lane.queue.stop_all();
        }
    }

    /// Pause or unpause every lane at once.
    pub fn pause_all(&self, paused: bool) {
        for lane in &self.list {
            lane.queue.paused.store(paused, Ordering::Relaxed);
        }
    }

    /// One line per lane, for the crash report.
    pub fn lines(&self) -> Vec<String> {
        self.list
            .iter()
            .filter(|lane| !lane.idle())
            .map(|lane| lane.line())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_always_a_lane_one() {
        let lanes = Lanes::default();
        assert_eq!(lanes.count(), 1);
        assert_eq!(lanes.list()[0].id, 1);
        assert!(lanes.list()[0].idle());
        assert!(!lanes.any_running());
    }

    #[test]
    fn a_split_opens_a_lane_and_it_can_be_closed_again() {
        let mut lanes = Lanes::default();
        let id = lanes.split();
        assert_eq!(id, 2);
        assert_eq!(lanes.count(), 2);
        // Lane one stays whatever is asked of it.
        assert!(!lanes.close(1));
        assert!(lanes.close(2));
        assert_eq!(lanes.count(), 1);
    }

    #[test]
    fn lanes_share_the_machine_rather_than_each_taking_it() {
        let mut lanes = Lanes::default();
        LANES.store(1, Ordering::Relaxed);
        let alone = Queue::workers();
        lanes.split();
        let shared = Queue::workers();
        assert!(shared <= alone);
        assert!(shared >= 1);
        LANES.store(1, Ordering::Relaxed);
    }
}
