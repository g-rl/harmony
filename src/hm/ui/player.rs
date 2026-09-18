use eframe::egui;

use crate::hm::app::State;
use crate::hm::ui::{PANES, Pane, theme, widgets};

pub fn bar(ui: &mut egui::Ui, state: &mut State) {
    controls(ui, state);
    ui.add_space(3.0);
    match state.pane {
        Pane::Waveform => wave(ui, state),
        Pane::Spectrogram => spectrum(ui, state),
        Pane::Meta => meta(ui, state),
        Pane::Compare => compare(ui, state),
    }
}

fn controls(ui: &mut egui::Ui, state: &mut State) {
    let position = state
        .transport
        .as_ref()
        .map(|transport| transport.position())
        .unwrap_or(0.0);
    let length = state
        .loaded
        .as_ref()
        .map(|loaded| loaded.samples.seconds())
        .unwrap_or(0.0);

    ui.horizontal(|ui| {
        let playing = state.transport.as_ref().is_some_and(|t| t.playing);
        if ui.button(if playing { "pause" } else { "play" }).clicked() {
            let samples = state.loaded.as_ref().map(|loaded| &loaded.samples);
            if let (Some(transport), Some(samples)) = (state.transport.as_mut(), samples) {
                if transport.finished() {
                    transport.play(samples);
                } else {
                    transport.toggle();
                }
            }
        }
        if ui.button("stop").clicked()
            && let Some(transport) = state.transport.as_mut()
        {
            transport.stop();
        }
        let looping = state.transport.as_ref().is_some_and(|t| t.looping);
        if widgets::chip(ui, "loop", looping).clicked()
            && let Some(transport) = state.transport.as_mut()
        {
            transport.looping = !looping;
        }

        ui.colored_label(
            theme::DIM,
            format!("{} / {}", widgets::seconds(position), widgets::seconds(length)),
        );

        let mut volume = state
            .transport
            .as_ref()
            .map(|t| t.volume)
            .unwrap_or(state.settings.volume);
        if ui
            .add_sized(
                egui::vec2(90.0, theme::INTERACT_HEIGHT),
                egui::Slider::new(&mut volume, 0.0..=1.5).show_value(false),
            )
            .changed()
        {
            if let Some(transport) = state.transport.as_mut() {
                transport.set_volume(volume);
            }
            state.settings.volume = volume;
            crate::hm::storage::save(&state.settings);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for pane in PANES.iter().rev() {
                if widgets::chip(ui, pane.label(), state.pane == *pane).clicked() {
                    state.pane = *pane;
                }
            }
            if state.transport.is_none() {
                ui.colored_label(theme::UNLIT, "no audio device");
            }
        });
    });
}

fn head_of(state: &State) -> Option<f32> {
    let transport = state.transport.as_ref()?;
    let length = state.loaded.as_ref()?.samples.seconds();
    if length <= 0.0 {
        return None;
    }
    Some((transport.position() / length).clamp(0.0, 1.0))
}

fn wave(ui: &mut egui::Ui, state: &mut State) {
    let bins = state
        .loaded
        .as_ref()
        .map(|loaded| loaded.peaks.bins.clone())
        .unwrap_or_default();
    let head = head_of(state);
    let width = ui.available_width();
    // A sound being read says so where its waveform will be, rather than
    // leaving the pane looking like the sound has no audio in it. The widget
    // paints it, so it cannot end up on top of the widget's own empty text.
    // A sound that is waiting on a container still being opened says which
    // one: the wait is that container's, not the sound's, and a name the
    // person can see moving is the difference between slow and stuck.
    let reading = state.sounding.as_ref().map(|sounding| {
        match state.jobs.iter().any(|job| Some(job.title) == state.title) {
            true => format!("still loading {}..", sounding.area),
            false => format!("reading {}", sounding.name),
        }
    });
    let empty = reading.as_deref().unwrap_or("no audio");
    let response = widgets::waveform_empty(ui, egui::vec2(width, 54.0), &bins, head, empty);
    if bins.is_empty() && reading.is_some() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }
    if response.clicked()
        && let Some(point) = response.interact_pointer_pos()
    {
        let fraction =
            ((point.x - response.rect.left()) / response.rect.width().max(1.0)).clamp(0.0, 1.0);
        let length = state
            .loaded
            .as_ref()
            .map(|loaded| loaded.samples.seconds())
            .unwrap_or(0.0);
        if let Some(transport) = state.transport.as_mut() {
            transport.seek(fraction * length);
        }
    }
}

fn spectrum(ui: &mut egui::Ui, state: &mut State) {
    let columns = state
        .loaded
        .as_ref()
        .map(|loaded| loaded.columns.clone())
        .unwrap_or_default();
    let width = ui.available_width();
    widgets::spectrogram(ui, egui::vec2(width, 54.0), &columns);
}

fn meta(ui: &mut egui::Ui, state: &mut State) {
    let Some(loaded) = state.loaded.as_ref() else {
        ui.colored_label(theme::DIM, "nothing loaded");
        return;
    };
    let stats = &loaded.stats;
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            theme::DIM,
            format!(
                "{} hz  {} ch  {}  peak {:.1} db  rms {:.1} db  centroid {:.0} hz",
                loaded.samples.rate,
                loaded.samples.channels,
                widgets::seconds(loaded.samples.seconds()),
                crate::hm::analysis::db(stats.peak),
                crate::hm::analysis::db(stats.rms),
                stats.centroid,
            ),
        );
    });
    ui.horizontal(|ui| {
        ui.colored_label(theme::DIM, "level");
        widgets::meter(ui, 160.0, stats.peak, theme::WAVE);
        ui.colored_label(theme::DIM, "loudness");
        widgets::meter(ui, 160.0, stats.rms * 3.0, theme::ACCENT);
    });
}

fn compare(ui: &mut egui::Ui, state: &mut State) {
    let width = ui.available_width();
    let (a_bins, a_name) = match (state.loaded.as_ref(), state.cursor) {
        (Some(loaded), Some(index)) => (
            loaded.peaks.bins.clone(),
            state
                .catalog
                .entries
                .get(index)
                .map(|entry| entry.display())
                .unwrap_or_default(),
        ),
        _ => (Vec::new(), String::from("nothing selected")),
    };
    let b_bins = state
        .against
        .as_ref()
        .map(|held| held.peaks.bins.clone())
        .unwrap_or_default();

    ui.horizontal(|ui| {
        ui.colored_label(theme::WAVE, format!("a  {}", crate::hm::discord::clip(&a_name, 28)));
        if let (Some(a), Some(b)) = (state.loaded.as_ref(), state.against.as_ref()) {
            let gap = crate::hm::analysis::db(a.stats.rms) - crate::hm::analysis::db(b.stats.rms);
            let length = a.samples.seconds() - b.samples.seconds();
            ui.colored_label(
                theme::DIM,
                format!("{gap:+.1} db  {length:+.2}s apart"),
            );
        } else {
            ui.colored_label(theme::DIM, "hold a sound as b to compare");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("hold as b").clicked() {
                state.hold_against();
            }
            if state.against.is_some() && ui.button("clear b").clicked() {
                state.against = None;
            }
        });
    });
    widgets::waveform(ui, egui::vec2(width, 26.0), &a_bins, head_of(state));
    widgets::waveform(ui, egui::vec2(width, 26.0), &b_bins, None);
}
