//! The card that stands over the window for the length of a carry.
//!
//! It exists because the drag blocks the ui thread: the window stops painting
//! for the whole gesture, so whatever is on screen when it starts is what stays
//! there. That frame is this card, and it says what is being carried and how to
//! get out of it.

use eframe::egui;

use crate::hm::app::State;
use crate::hm::ui::{theme, widgets};

const WIDTH: f32 = 260.0;

pub fn card(ctx: &egui::Context, state: &mut State) {
    let Some(carried) = state.dragging_out.as_mut() else {
        return;
    };
    let name = carried.name.clone();
    let making = carried.making.as_ref().map(|making| making.fraction());
    let note = match making {
        Some(_) => "the files are being written, the drag starts when they land",
        None => "drop them on a folder, a chat, anything outside harmony",
    };
    // This frame is the one that puts the card on screen; the next one blocks.
    carried.painted = true;
    ctx.request_repaint();

    let frame = egui::Frame::new()
        .fill(theme::PANEL)
        .stroke(egui::Stroke::new(1.0, theme::WAVE))
        .inner_margin(egui::Margin::same(14));

    egui::Area::new(egui::Id::new("harmony_dragout"))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .interactable(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(WIDTH);
                ui.vertical(|ui| {
                    let title = match making {
                        Some(_) => format!("getting {name} ready"),
                        None => format!("carrying {name}"),
                    };
                    ui.colored_label(theme::WAVE, title);
                    ui.add_space(4.0);
                    if let Some(fraction) = making {
                        widgets::meter(ui, WIDTH, fraction, theme::WAVE);
                        ui.add_space(4.0);
                    }
                    ui.colored_label(theme::DIM, note);
                    ui.colored_label(theme::DIM, "press esc to cancel");
                });
            });
        });
}
