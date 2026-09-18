//! What the window says when there is not enough room to write.
//!
//! Two shapes of the same problem: work that has not started because the disk
//! is already too full for it, and a queue that filled the disk while it ran
//! and stopped itself partway. Both say where harmony is writing, what it
//! needs and what is there, and neither throws the work away.

use eframe::egui;

use crate::hm::app::{Again, State};
use crate::hm::ui::{theme, widgets};

pub fn window(ctx: &egui::Context, state: &mut State) {
    blocked(ctx, state);
    stalled(ctx, state);
}

/// Work that never started.
fn blocked(ctx: &egui::Context, state: &mut State) {
    let Some(blocked) = state.blocked.clone() else {
        return;
    };
    let mut open = true;
    egui::Window::new("not enough room")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            ui.colored_label(
                theme::UNLIT,
                format!("harmony cannot {} yet", blocked.what),
            );
            ui.add_space(6.0);
            widgets::field(ui, "writing to", &shown(&blocked.room.path));
            widgets::field(ui, "needs", &widgets::bytes(blocked.room.needed()));
            widgets::field(ui, "free", &widgets::bytes(blocked.room.free));
            widgets::field(ui, "short by", &widgets::bytes(blocked.room.missing()));
            ui.add_space(4.0);
            ui.colored_label(
                theme::DIM,
                "nothing was written. make room and try again, or point it somewhere else.",
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("try again").clicked() {
                    state.retry_blocked();
                }
                if ui.button(folder_word(blocked.again)).clicked() {
                    state.repoint_blocked();
                }
                if ui.button("leave it").clicked() {
                    state.blocked = None;
                }
            });
        });
    if !open {
        state.blocked = None;
    }
}

/// A queue that stopped partway because the room ran out under it.
fn stalled(ctx: &egui::Context, state: &mut State) {
    let stall = {
        let progress = state.queue.progress.lock().unwrap();
        progress.stalled.clone()
    };
    let Some(stall) = stall else {
        return;
    };
    egui::Window::new("the disk filled up")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 40.0))
        .show(ctx, |ui| {
            ui.set_min_width(360.0);
            let (done, total) = {
                let progress = state.queue.progress.lock().unwrap();
                (progress.done + progress.failed, progress.total)
            };
            ui.colored_label(
                theme::UNLIT,
                format!(
                    "the queue is waiting at {} of {}",
                    widgets::tally(done),
                    widgets::tally(total)
                ),
            );
            ui.add_space(6.0);
            widgets::field(ui, "writing to", &shown(&stall.path));
            widgets::field(ui, "needs", &widgets::bytes(stall.want));
            widgets::field(ui, "free", &widgets::bytes(stall.free));
            ui.add_space(4.0);
            ui.colored_label(
                theme::DIM,
                "the files already written are fine. free some room, then carry on.",
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("carry on").clicked() {
                    let mut progress = state.queue.progress.lock().unwrap();
                    progress.stalled = None;
                    drop(progress);
                    state
                        .queue
                        .paused
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                }
                if ui.button("cancel the rest").clicked() {
                    state.queue.stop();
                    let mut progress = state.queue.progress.lock().unwrap();
                    progress.stalled = None;
                    drop(progress);
                    state
                        .queue
                        .paused
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                }
            });
        });
}

fn folder_word(again: Again) -> &'static str {
    match again {
        Again::Cache => "cache somewhere else",
        Again::Extract { .. } => "extract somewhere else",
        Again::DragOut => "scratch somewhere else",
    }
}

fn shown(path: &std::path::Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}
