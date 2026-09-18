//! The veil over the window while harmony is setting itself up.
//!
//! The window goes up before the work does: the last folder still has to be
//! fingerprinted, its cached catalogue read and any name lists loaded, and all
//! of that happens on threads. Rather than a bare frame with nothing in it,
//! the panels are drawn dimmed behind a veil that says what is going on.

use eframe::egui;

use crate::hm::app::State;
use crate::hm::ui::theme;

pub fn veil(ctx: &egui::Context, state: &State) {
    if !state.booting {
        return;
    }
    let screen = ctx.viewport_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("setting up"),
    ));

    // egui has no blur, so the window behind is softened the way a blur reads:
    // a heavy wash of the background colour, then a few wide, barely-there
    // panels over the middle to lift it where the words go.
    painter.rect_filled(screen, 0.0, theme::BG.gamma_multiply(0.92));
    let middle = egui::Rect::from_center_size(
        screen.center(),
        egui::vec2(screen.width().min(560.0), 150.0),
    );
    for step in 0..6 {
        let grown = middle.expand(step as f32 * 7.0);
        painter.rect_filled(grown, 10.0, theme::PANEL.gamma_multiply(0.10));
    }

    painter.text(
        screen.center() - egui::vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        "setting up harmony..",
        egui::FontId::monospace(18.0),
        theme::TEXT,
    );
    painter.text(
        screen.center() + egui::vec2(0.0, 16.0),
        egui::Align2::CENTER_CENTER,
        state
            .activity()
            .unwrap_or_else(|| state.status.clone())
            .to_ascii_lowercase(),
        egui::FontId::monospace(11.0),
        theme::DIM,
    );

    // A slow sweep under the words, so a long wait still looks like progress.
    let time = ctx.input(|i| i.time) as f32;
    let width = 220.0;
    let track = egui::Rect::from_center_size(
        screen.center() + egui::vec2(0.0, 38.0),
        egui::vec2(width, 2.0),
    );
    painter.rect_filled(track, 1.0, theme::HAIRLINE);
    let sweep = (time * 0.6).sin() * 0.5 + 0.5;
    let lit = egui::Rect::from_min_size(
        egui::pos2(track.left() + (width - 60.0) * sweep, track.top()),
        egui::vec2(60.0, 2.0),
    );
    painter.rect_filled(lit, 1.0, theme::WAVE);
    ctx.request_repaint();
}
