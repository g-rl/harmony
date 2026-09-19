use std::sync::atomic::Ordering;

use eframe::egui;

use crate::hm::app::State;
use crate::hm::export::queue::JobState;
use crate::hm::ui::{theme, widgets};

pub fn window(ctx: &egui::Context, state: &mut State) {
    let mut open = state.show_queue;
    egui::Window::new("queue")
        .open(&mut open)
        .default_width(420.0)
        .default_height(320.0)
        .frame(
            egui::Frame::new()
                .fill(theme::PANEL)
                .stroke(egui::Stroke::new(1.0, theme::HAIRLINE))
                .inner_margin(egui::Margin::same(8)),
        )
        .show(ctx, |ui| body(ui, state));
    state.show_queue = open;
    // A run that is moving needs the window redrawn whether or not anything is
    // being clicked, or the numbers sit still while the work goes on.
    if state.queue.progress.lock().map(|p| p.running).unwrap_or(false) {
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
}

/// What the window needs off the queue this frame.
///
/// Everything here is read under one lock and let go of straight away: the
/// workers are writing into the same struct, and a window that held the lock
/// while it drew would stop them. The rows are the one thing that is never
/// copied wholesale — a bulk run has none, and a listed run copies only the
/// lines that are actually on screen.
struct Look {
    done: usize,
    failed: usize,
    total: usize,
    running: bool,
    output: Option<std::path::PathBuf>,
    bulk: bool,
    bytes: u64,
    workers: usize,
    elapsed: f32,
    recent: Vec<String>,
    failures: Vec<(String, String)>,
    rows: usize,
}

fn look(state: &State) -> Look {
    let progress = state.queue.progress.lock().unwrap();
    Look {
        done: progress.done,
        failed: progress.failed,
        total: progress.total,
        running: progress.running,
        output: progress.output.clone(),
        bulk: progress.bulk,
        bytes: progress.bytes,
        workers: progress.workers,
        elapsed: progress
            .started
            .map(|at| at.elapsed().as_secs_f32())
            .unwrap_or(0.0),
        recent: progress.recent.iter().cloned().collect(),
        failures: progress.failures.clone(),
        rows: progress.jobs.len(),
    }
}

fn body(ui: &mut egui::Ui, state: &mut State) {
    let it = look(state);
    let fraction = if it.total == 0 {
        0.0
    } else {
        (it.done + it.failed) as f32 / it.total as f32
    };

    ui.horizontal(|ui| {
        let paused = state.queue.paused.load(Ordering::Relaxed);
        if it.running {
            if ui.button(if paused { "resume" } else { "pause" }).clicked() {
                state.queue.toggle_pause();
            }
            if ui.button("cancel").clicked() {
                state.queue.stop();
            }
            if paused {
                ui.colored_label(theme::UNLIT, "paused");
            }
        } else if it.total > 0 {
            ui.colored_label(theme::GOOD, "finished");
        } else {
            ui.colored_label(theme::DIM, "nothing queued");
        }
        if let Some(root) = it.output.clone()
            && ui.button("open output").clicked()
        {
            let _ = open_folder(&root);
        }
        if it.failed > 0 && !it.running && !it.bulk && ui.button("retry failed").clicked() {
            retry(state);
        }
    });

    ui.add_space(4.0);
    widgets::meter(
        ui,
        ui.available_width(),
        fraction,
        if it.failed > 0 { theme::UNLIT } else { theme::WAVE },
    );
    ui.add_space(4.0);
    ui.colored_label(
        theme::DIM,
        format!(
            "{} done  {} failed  {} total  ({:.0}%)",
            widgets::tally(it.done),
            widgets::tally(it.failed),
            widgets::tally(it.total),
            fraction * 100.0
        ),
    );
    if let Some(root) = it.output.clone() {
        ui.colored_label(
            theme::DIM,
            crate::hm::discord::clip(&root.to_string_lossy().to_ascii_lowercase(), 60),
        );
    }

    match it.bulk {
        true => bulk(ui, &it),
        false => rows(ui, state, it.rows),
    }
}

/// The card a library run is watched by.
///
/// No row per sound: a hundred and twenty thousand of them is a list nobody
/// reads and a megabyte of strings copied every frame. What is here instead is
/// what somebody waiting on a long run actually wants — how fast it is going,
/// how much has landed, how long is left, and the last few names, so the thing
/// is visibly moving through the library.
fn bulk(ui: &mut egui::Ui, it: &Look) {
    let through = match it.elapsed > 0.5 {
        true => (it.done + it.failed) as f32 / it.elapsed,
        false => 0.0,
    };
    let left = match through > 0.01 {
        true => Some((it.total.saturating_sub(it.done + it.failed)) as f32 / through),
        false => None,
    };

    ui.add_space(4.0);
    widgets::hairline(ui);
    ui.add_space(4.0);
    ui.colored_label(
        theme::DIM,
        format!(
            "{} written  ·  {:.0} a second  ·  {} threads",
            widgets::bytes(it.bytes),
            through,
            it.workers.max(1)
        ),
    );
    ui.colored_label(
        theme::DIM,
        match (it.running, left) {
            (true, Some(seconds)) => format!(
                "{} gone  ·  about {} left",
                widgets::seconds(it.elapsed),
                widgets::seconds(seconds)
            ),
            (true, None) => format!("{} gone", widgets::seconds(it.elapsed)),
            (false, _) => format!("took {}", widgets::seconds(it.elapsed)),
        },
    );

    if !it.recent.is_empty() {
        ui.add_space(4.0);
        widgets::hairline(ui);
        ui.add_space(4.0);
        for name in it.recent.iter().rev() {
            ui.add(
                egui::Label::new(egui::RichText::new(name.as_str()).color(theme::DIM))
                    .truncate()
                    .selectable(false),
            );
        }
    }

    if !it.failures.is_empty() {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(format!("{} failed", widgets::tally(it.failed)))
            .id_salt("queue-failures")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("queue-failed-rows")
                    .max_height(140.0)
                    .show_rows(ui, theme::ROW_HEIGHT, it.failures.len(), |ui, range| {
                        for index in range {
                            let (name, why) = &it.failures[index];
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!("{name}  {why}"))
                                        .color(theme::UNLIT),
                                )
                                .truncate()
                                .selectable(false),
                            );
                        }
                    });
            });
    }
}

/// The row per sound a small run keeps.
///
/// Only the lines on screen are copied out from under the lock, so a run of a
/// few thousand costs the same to draw as a run of ten.
fn rows(ui: &mut egui::Ui, state: &mut State, count: usize) {
    ui.add_space(4.0);
    widgets::hairline(ui);
    egui::ScrollArea::vertical()
        .id_salt("queue-rows")
        .auto_shrink([false, false])
        .show_rows(ui, theme::ROW_HEIGHT, count, |ui, range| {
            let shown: Vec<(String, JobState)> = {
                let progress = state.queue.progress.lock().unwrap();
                range
                    .clone()
                    .filter_map(|index| {
                        progress
                            .jobs
                            .get(index)
                            .map(|job| (job.label.clone(), job.state.clone()))
                    })
                    .collect()
            };
            for (label, job) in shown {
                let (colour, note) = match &job {
                    JobState::Waiting => (theme::DIM, String::from("waiting")),
                    JobState::Running => (theme::WAVE, String::from("running")),
                    JobState::Done => (theme::GOOD, String::from("done")),
                    JobState::Failed(error) => (theme::UNLIT, error.to_ascii_lowercase()),
                };
                ui.colored_label(
                    colour,
                    format!("{:<40} {}", crate::hm::discord::clip(&label, 40), note),
                );
            }
        });
}

fn retry(state: &mut State) {
    let failed: Vec<String> = {
        let progress = state.queue.progress.lock().unwrap();
        progress
            .jobs
            .iter()
            .filter(|job| matches!(job.state, JobState::Failed(_)))
            .map(|job| job.label.clone())
            .collect()
    };
    if failed.is_empty() {
        return;
    }
    let picks: Vec<usize> = state
        .catalog
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| failed.contains(&entry.display()))
        .map(|(index, _)| index)
        .collect();
    state.selection = picks.into_iter().collect();
    state.extract(false);
}

fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
    }
}
