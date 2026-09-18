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
}

fn body(ui: &mut egui::Ui, state: &mut State) {
    let (jobs, done, failed, total, running, output) = {
        let progress = state.queue.progress.lock().unwrap();
        (
            progress.jobs.clone(),
            progress.done,
            progress.failed,
            progress.total,
            progress.running,
            progress.output.clone(),
        )
    };

    let fraction = if total == 0 {
        0.0
    } else {
        (done + failed) as f32 / total as f32
    };

    ui.horizontal(|ui| {
        let paused = state.queue.paused.load(Ordering::Relaxed);
        if running {
            if ui.button(if paused { "resume" } else { "pause" }).clicked() {
                state.queue.toggle_pause();
            }
            if ui.button("cancel").clicked() {
                state.queue.stop();
            }
        } else if total > 0 {
            ui.colored_label(theme::GOOD, "finished");
        } else {
            ui.colored_label(theme::DIM, "nothing queued");
        }
        if let Some(root) = output.clone()
            && ui.button("open output").clicked()
        {
            let _ = open_folder(&root);
        }
        if failed > 0 && ui.button("retry failed").clicked() {
            retry(state, &jobs);
        }
    });

    ui.add_space(4.0);
    widgets::meter(
        ui,
        ui.available_width(),
        fraction,
        if failed > 0 { theme::UNLIT } else { theme::WAVE },
    );
    ui.add_space(4.0);
    ui.colored_label(
        theme::DIM,
        format!(
            "{} done  {} failed  {} total",
            widgets::tally(done),
            widgets::tally(failed),
            widgets::tally(total)
        ),
    );
    if let Some(root) = output {
        ui.colored_label(
            theme::DIM,
            crate::hm::discord::clip(&root.to_string_lossy().to_ascii_lowercase(), 60),
        );
    }

    ui.add_space(4.0);
    widgets::hairline(ui);
    egui::ScrollArea::vertical()
        .id_salt("queue-rows")
        .auto_shrink([false, false])
        .show_rows(ui, theme::ROW_HEIGHT, jobs.len(), |ui, range| {
            for index in range {
                let job = &jobs[index];
                let (colour, note) = match &job.state {
                    JobState::Waiting => (theme::DIM, String::from("waiting")),
                    JobState::Running => (theme::WAVE, String::from("running")),
                    JobState::Done => (theme::GOOD, String::from("done")),
                    JobState::Failed(error) => (theme::UNLIT, error.to_ascii_lowercase()),
                };
                ui.colored_label(
                    colour,
                    format!(
                        "{:<40} {}",
                        crate::hm::discord::clip(&job.label, 40),
                        note
                    ),
                );
            }
        });
}

fn retry(state: &mut State, jobs: &[crate::hm::export::queue::Job]) {
    let failed: Vec<String> = jobs
        .iter()
        .filter(|job| matches!(job.state, JobState::Failed(_)))
        .map(|job| job.label.clone())
        .collect();
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
