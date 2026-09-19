use std::sync::atomic::Ordering;

use eframe::egui;

use crate::hm::app::State;
use crate::hm::export::queue::JobState;
use crate::hm::ui::{theme, widgets};

/// Every lane that has a window open.
///
/// Lane one is the queue harmony has always had, and the export screen's
/// button opens it. A split view opens another beside it — its own line, its
/// own threads, its own window — and as many can be opened as there is work
/// for. They are drawn stepped down and across so that two of them are two
/// windows rather than one hiding the other.
pub fn window(ctx: &egui::Context, state: &mut State) {
    let lanes: Vec<(usize, bool)> = state
        .queue
        .list()
        .iter()
        .map(|lane| (lane.id, lane.open))
        .collect();

    for (at, (id, open)) in lanes.into_iter().enumerate() {
        let showing = match id {
            1 => state.show_queue,
            _ => open,
        };
        if !showing {
            continue;
        }
        let mut still = true;
        let title = match id {
            1 => "queue".to_string(),
            other => format!("queue \u{b7} lane {other}"),
        };
        egui::Window::new(title)
            .id(egui::Id::new(("queue-lane", id)))
            .open(&mut still)
            .default_width(430.0)
            .default_height(340.0)
            .default_pos([40.0 + at as f32 * 34.0, 70.0 + at as f32 * 34.0])
            .frame(
                egui::Frame::new()
                    .fill(theme::PANEL)
                    .stroke(egui::Stroke::new(
                        1.0,
                        match id == state.lane_focus {
                            true => theme::WAVE.gamma_multiply(0.5),
                            false => theme::HAIRLINE,
                        },
                    ))
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                if ui.rect_contains_pointer(ui.max_rect()) {
                    state.lane_focus = id;
                }
                body(ui, state, id)
            });
        if !still {
            match id {
                1 => state.show_queue = false,
                other => {
                    // A lane with nothing left to do goes away with its
                    // window; one still writing keeps both, because closing
                    // the window is not the same as calling the work off.
                    if !state.queue.close(other)
                        && let Some(lane) = state.queue.find_mut(other)
                    {
                        lane.open = false;
                    }
                }
            }
        }
    }

    if state.queue.any_running() {
        // Work that is moving needs the window redrawn whether or not anything
        // is being clicked, or the numbers sit still while the writing goes on.
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
}

/// What the window needs off one lane this frame.
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
    /// True while the clock is still moving, false once it stopped.
    going: bool,
    recent: Vec<String>,
    failures: Vec<(String, String)>,
    rows: usize,
    label: String,
    waiting: Vec<crate::hm::export::queue::Waiting>,
}

fn look(state: &State, lane: usize) -> Look {
    let Some(lane) = state.queue.find(lane) else {
        return empty();
    };
    let Ok(progress) = lane.queue.progress.lock() else {
        return empty();
    };
    Look {
        done: progress.done,
        failed: progress.failed,
        total: progress.total,
        running: progress.running,
        output: progress.output.clone(),
        bulk: progress.bulk,
        bytes: progress.bytes,
        workers: progress.workers,
        // The run that has ended says how long it took; the run that is going
        // is read off the moment it started. Without the first of those the
        // clock carries on counting after the writing has stopped.
        elapsed: progress.spent.unwrap_or_else(|| {
            progress
                .started
                .map(|at| at.elapsed().as_secs_f32())
                .unwrap_or(0.0)
        }),
        going: progress.spent.is_none(),
        recent: progress.recent.iter().cloned().collect(),
        failures: progress.failures.clone(),
        rows: progress.jobs.len(),
        label: progress.label.clone(),
        waiting: progress.waiting.clone(),
    }
}

fn empty() -> Look {
    Look {
        done: 0,
        failed: 0,
        total: 0,
        running: false,
        output: None,
        bulk: false,
        bytes: 0,
        workers: 0,
        elapsed: 0.0,
        going: false,
        recent: Vec::new(),
        failures: Vec::new(),
        rows: 0,
        label: String::new(),
        waiting: Vec::new(),
    }
}

fn body(ui: &mut egui::Ui, state: &mut State, id: usize) {
    let it = look(state, id);
    let fraction = if it.total == 0 {
        0.0
    } else {
        (it.done + it.failed) as f32 / it.total as f32
    };

    ui.horizontal(|ui| {
        let paused = state
            .queue
            .find(id)
            .map(|lane| lane.queue.paused.load(Ordering::Relaxed))
            .unwrap_or(false);
        if it.running {
            if ui.button(if paused { "resume" } else { "pause" }).clicked()
                && let Some(lane) = state.queue.find(id)
            {
                lane.queue.toggle_pause();
            }
            if ui
                .button("cancel")
                .on_hover_text("stop this run; the ones behind it carry on")
                .clicked()
                && let Some(lane) = state.queue.find(id)
            {
                lane.queue.stop();
            }
            if !it.waiting.is_empty()
                && ui
                    .button("cancel all")
                    .on_hover_text("stop this run and empty the line behind it")
                    .clicked()
                && let Some(lane) = state.queue.find(id)
            {
                lane.queue.stop_all();
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

    if !it.label.is_empty() {
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(egui::RichText::new(it.label.as_str()).color(theme::TEXT))
                .truncate()
                .selectable(false),
        );
    }

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

    lanes(ui, state, id);
    line(ui, state, &it, id);

    match it.bulk {
        true => bulk(ui, &it),
        false => rows(ui, state, id, it.rows),
    }
}

/// The lane strip: which lanes there are, which one this window is, and
/// whether the next export opens one of its own.
///
/// One line of exports is the right shape for one disk and one library. It is
/// the wrong shape for two games on two drives, or for a short job that should
/// not sit behind an afternoon of writing — so a split runs them side by side,
/// sharing the threads out rather than each lane taking the machine.
fn lanes(ui: &mut egui::Ui, state: &mut State, id: usize) {
    ui.add_space(4.0);
    widgets::hairline(ui);
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(theme::DIM, "lanes");
        let open: Vec<(usize, bool)> = state
            .queue
            .list()
            .iter()
            .map(|lane| (lane.id, !lane.idle()))
            .collect();
        let mut show: Option<usize> = None;
        for (other, busy) in open {
            let label = match busy {
                true => format!("{other} \u{25cf}"),
                false => other.to_string(),
            };
            if widgets::chip(ui, &label, other == id)
                .on_hover_text(match busy {
                    true => "writing",
                    false => "idle",
                })
                .clicked()
            {
                show = Some(other);
            }
        }
        if let Some(other) = show {
            state.lane_focus = other;
            if let Some(lane) = state.queue.find_mut(other) {
                lane.open = true;
            }
            if other == 1 {
                state.show_queue = true;
            }
        }
        if ui
            .small_button("split view")
            .on_hover_text("open another lane and its window, running beside this one")
            .clicked()
        {
            let new = state.queue.split();
            state.lane_focus = new;
        }
        let split = state.split_next;
        if widgets::chip(ui, "next in its own lane", split)
            .on_hover_text("the next export starts beside this one instead of behind it")
            .clicked()
        {
            state.split_next = !split;
        }
    });
}

/// The runs behind this one.
///
/// Asking for a second export while one is going does not throw the first one
/// away and does not make the second one wait for a person to come back: it
/// joins the line and starts when the one in front of it finishes. From here
/// any of them can be moved to the front, taken out, or lifted into a lane of
/// its own to be got on with now — and the whole line can be held so nothing
/// new starts when the run going ends.
fn line(ui: &mut egui::Ui, state: &mut State, it: &Look, id: usize) {
    if it.waiting.is_empty() {
        return;
    }
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.colored_label(
            theme::DIM,
            format!("{} waiting", widgets::tally(it.waiting.len())),
        );
        let holding = state
            .queue
            .find(id)
            .map(|lane| lane.queue.holding())
            .unwrap_or(false);
        if widgets::chip(ui, if holding { "held" } else { "hold" }, holding)
            .on_hover_text("finish this run and stop, rather than starting the next")
            .clicked()
            && let Some(lane) = state.queue.find(id)
        {
            lane.queue.toggle_hold();
        }
        if ui
            .small_button("clear waiting")
            .on_hover_text("take them all out; the run that is going keeps going")
            .clicked()
            && let Some(lane) = state.queue.find(id)
        {
            lane.queue.clear_waiting();
        }
    });
    ui.add_space(2.0);

    let mut promote: Option<u64> = None;
    let mut drop_it: Option<u64> = None;
    let mut split: Option<u64> = None;
    for (at, run) in it.waiting.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.colored_label(theme::DIM, format!("{}.", at + 1));
            ui.add(
                egui::Label::new(egui::RichText::new(run.label.as_str()).color(theme::DIM))
                    .truncate()
                    .selectable(false),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("drop").clicked() {
                    drop_it = Some(run.id);
                }
                if ui
                    .small_button("split")
                    .on_hover_text("start it now, in a lane of its own")
                    .clicked()
                {
                    split = Some(run.id);
                }
                if at > 0
                    && ui
                        .small_button("next")
                        .on_hover_text("start this one next")
                        .clicked()
                {
                    promote = Some(run.id);
                }
            });
        });
    }
    if let Some(run) = promote
        && let Some(lane) = state.queue.find(id)
    {
        lane.queue.promote(run);
    }
    if let Some(run) = drop_it
        && let Some(lane) = state.queue.find(id)
    {
        lane.queue.drop_waiting(run);
    }
    if let Some(run) = split
        && let Some(into) = state.queue.split_off(id, run)
    {
        state.lane_focus = into;
        state.status = format!("started in lane {into}");
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
            "{} written  \u{b7}  {:.0} a second  \u{b7}  {} threads",
            widgets::bytes(it.bytes),
            through,
            it.workers.max(1)
        ),
    );
    ui.colored_label(
        theme::DIM,
        match (it.going, left) {
            (true, Some(seconds)) => format!(
                "{} gone  \u{b7}  about {} left",
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
fn rows(ui: &mut egui::Ui, state: &mut State, id: usize, count: usize) {
    ui.add_space(4.0);
    widgets::hairline(ui);
    egui::ScrollArea::vertical()
        .id_salt(("queue-rows", id))
        .auto_shrink([false, false])
        .show_rows(ui, theme::ROW_HEIGHT, count, |ui, range| {
            let shown: Vec<(String, JobState)> = {
                let Some(lane) = state.queue.find(id) else {
                    return;
                };
                let Ok(progress) = lane.queue.progress.lock() else {
                    return;
                };
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

pub fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
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
