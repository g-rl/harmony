use eframe::egui;

use crate::hm::app::State;
use crate::hm::ui::{theme, widgets};

/// The question harmony asks before it closes on top of work that is running.
///
/// Three answers, and they are the three real ones: put the export down in a
/// way that can be picked up, throw it away, or go back to what you were
/// doing. A scan is named here too but has no button of its own — it writes
/// its own cache and can simply be run again, and saying so is more use than
/// a choice that changes nothing.
pub fn window(ctx: &egui::Context, state: &mut State) {
    if !state.closing {
        return;
    }
    let busy = state.busy();
    // Whatever was running has stopped while the card was up: nothing left to
    // ask about.
    if !busy.anything() {
        state.closing = false;
        state.quitting = true;
        crate::hm::window::chrome::close(ctx);
        return;
    }

    let mut answered: Option<Answer> = None;
    egui::Modal::new(egui::Id::new("closing")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.colored_label(theme::TEXT, "harmony is still working");
        ui.add_space(6.0);

        if let Some((through, total)) = busy.exporting {
            let left = total.saturating_sub(through);
            ui.colored_label(
                theme::DIM,
                format!(
                    "an export is running: {} of {} done, {} to go",
                    widgets::tally(through),
                    widgets::tally(total),
                    widgets::tally(left)
                ),
            );
        }
        if busy.scanning {
            ui.colored_label(
                theme::DIM,
                "a scan is running; it writes its own cache and can be run again",
            );
        }
        if busy.naming {
            ui.colored_label(theme::DIM, "a name list is still being matched");
        }

        ui.add_space(8.0);
        widgets::hairline(ui);
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if busy.exporting.is_some()
                && state.library_at.is_some()
                && ui
                    .button("pause and quit")
                    .on_hover_text(
                        "stop where it is and remember the run, so it can be picked up later",
                    )
                    .clicked()
            {
                answered = Some(Answer::Save);
            }
            if ui
                .button("quit anyway")
                .on_hover_text("stop everything and close; what is written stays written")
                .clicked()
            {
                answered = Some(Answer::Quit);
            }
            if ui.button("keep working").clicked() {
                answered = Some(Answer::Stay);
            }
        });
    });

    match answered {
        Some(Answer::Save) => {
            state.pause_and_save();
            state.closing = false;
            state.quitting = true;
            crate::hm::window::chrome::close(ctx);
        }
        Some(Answer::Quit) => {
            state.queue.stop();
            state.closing = false;
            state.quitting = true;
            crate::hm::window::chrome::close(ctx);
        }
        Some(Answer::Stay) => state.closing = false,
        None => {}
    }
}

enum Answer {
    Save,
    Quit,
    Stay,
}
