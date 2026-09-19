use eframe::egui;

use crate::hm::analysis::{db, transient};
use crate::hm::catalog::Source;
use crate::hm::export::{FORMATS, layout};
use crate::hm::ui::{theme, widgets};

use crate::hm::app::State;

pub fn panel(ui: &mut egui::Ui, state: &mut State) {
    egui::ScrollArea::vertical()
        .id_salt("detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            match state.cursor {
                Some(index) => sound(ui, state, index),
                None => {
                    ui.colored_label(theme::DIM, "no sound selected");
                }
            }
            ui.add_space(8.0);
            widgets::hairline(ui);
            ui.add_space(6.0);
            output(ui, state);
        });
}

fn sound(ui: &mut egui::Ui, state: &mut State, index: usize) {
    let Some(entry) = state.catalog.entries.get(index).cloned() else {
        return;
    };
    let package = state
        .mounted
        .packages
        .get(entry.package.0 as usize)
        .cloned()
        .unwrap_or_else(|| "unknown".into());

    ui.horizontal_wrapped(|ui| {
        ui.colored_label(theme::TEXT, entry.display());
    });
    ui.add_space(4.0);
    widgets::field(ui, "category", entry.category.label());
    widgets::field(ui, "package", &package);
    widgets::field(
        ui,
        "language",
        entry.language.as_deref().unwrap_or("shared"),
    );
    match entry.source {
        Source::Stream { key } => widgets::field(ui, "key", &format!("{key:016x}")),
        Source::Bank {
            package: id, index, ..
        } => widgets::field(ui, "bank", &format!("{}:{index}", id.0)),
    }
    widgets::field(ui, "codec", entry.codec.label());
    widgets::field(ui, "rate", &format!("{} hz", entry.rate));
    widgets::field(ui, "channels", &entry.channels.to_string());
    widgets::field(ui, "length", &widgets::seconds(entry.seconds()));
    widgets::field(ui, "packed", &widgets::bytes(entry.bytes));

    // A sound being read still gets the block, saying so. Taking it away and
    // putting it back is the panel jumping a frame every time the selection
    // moves, which is what it looked like.
    let reading = state
        .sounding
        .as_ref()
        .filter(|sounding| !sounding.against && sounding.index == index)
        .map(|sounding| sounding.area.clone());
    if let Some(area) = reading.filter(|_| state.loaded.is_none()) {
        ui.add_space(6.0);
        widgets::hairline(ui);
        ui.add_space(6.0);
        ui.colored_label(theme::DIM, format!("still loading {area}.."));
    }

    if let Some(loaded) = state.loaded.as_ref() {
        ui.add_space(6.0);
        widgets::hairline(ui);
        ui.add_space(6.0);
        let stats = &loaded.stats;
        widgets::field(ui, "peak", &format!("{:.1} db", db(stats.peak)));
        widgets::field(ui, "rms", &format!("{:.1} db", db(stats.rms)));
        widgets::field(ui, "centroid", &format!("{:.0} hz", stats.centroid));
        widgets::field(
            ui,
            "crossings",
            &widgets::tally(stats.zero_crossings),
        );
        widgets::field(ui, "silent", if stats.silence { "yes" } else { "no" });
        widgets::field(
            ui,
            "decoded",
            &format!(
                "{} frames {}",
                widgets::tally(loaded.samples.frames()),
                widgets::seconds(loaded.samples.seconds())
            ),
        );
        if let Some(marks) = transient::detect(&loaded.samples) {
            let rate = loaded.samples.rate.max(1) as f32;
            widgets::field(
                ui,
                "attack",
                &format!("{:.3}s", marks.attack as f32 / rate),
            );
            widgets::field(
                ui,
                "head/tail",
                &format!(
                    "{:.3}s / {:.3}s",
                    marks.silence_head as f32 / rate,
                    marks.silence_tail as f32 / rate
                ),
            );
        }
    }

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        let favorite = entry.favorite;
        if widgets::chip(ui, "favorite", favorite).clicked() {
            state.toggle_favorite(index);
        }
        if ui.button("extract").clicked() {
            state.selection.clear();
            state.selection.insert(index);
            state.extract(false);
        }
        if ui.button("copy name").clicked() {
            ui.ctx().copy_text(entry.display());
        }
        if ui.button("select all shown").clicked() {
            state.selection = state.filtered.iter().copied().collect();
        }
    });

    if !entry.tags.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for tag in &entry.tags {
                ui.colored_label(theme::ACCENT, tag);
            }
        });
    }

    similar(ui, state, index);
}


fn similar(ui: &mut egui::Ui, state: &mut State, _index: usize) {
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(4.0);
    ui.colored_label(theme::DIM, "similar");

    let ranked = state.similar.clone();
    if ranked.is_empty() {
        ui.colored_label(theme::DIM, "nothing to compare with");
        return;
    }
    let mut clicked = None;
    for (score, other) in ranked {
        let Some(entry) = state.catalog.entries.get(other) else {
            continue;
        };
        let package = state
            .mounted
            .packages
            .get(entry.package.0 as usize)
            .cloned()
            .unwrap_or_default();
        let text = format!(
            "{}  {}  {:.2}",
            crate::hm::discord::clip(&entry.display(), 18),
            crate::hm::discord::clip(&package, 16),
            score
        );
        if ui
            .selectable_label(false, egui::RichText::new(text).color(theme::DIM))
            .clicked()
        {
            clicked = Some(other);
        }
    }
    if let Some(other) = clicked {
        state.select(other, true);
    }
}

/// How deep a folder tree an extraction builds, and what that looks like.
///
/// One ladder rather than a menu and a checkbox beside it: the rungs run from
/// the file on its own up to the language above the category above the
/// container, and the line underneath is the path the sound on screen would
/// actually be written to, so the choice is read off an example rather than
/// worked out from the names of the modes.
fn tree(ui: &mut egui::Ui, state: &mut State) {
    ui.add_space(6.0);
    ui.colored_label(theme::DIM, "folder tree");
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        for choice in layout::ALL {
            let on = state.options.layout == *choice;
            if widgets::chip(ui, choice.label(), on)
                .on_hover_text(choice.about())
                .clicked()
            {
                state.options.layout = *choice;
                state.save_options();
            }
        }
    });

    // What the sound on screen would be written as. With nothing selected
    // there is no example to show, so the rung says what it does instead.
    let shown = state.cursor.and_then(|index| {
        let entry = state.catalog.entries.get(index)?;
        let package = state
            .mounted
            .packages
            .get(entry.package.0 as usize)
            .cloned()
            .unwrap_or_else(|| "unknown".into());
        Some(layout::preview(
            entry,
            &package,
            state.options.layout,
            state.options.normalise_names,
            state.options.format.extension(),
        ))
    });
    let line = match &shown {
        Some(path) => path.as_str(),
        None => state.options.layout.about(),
    };
    ui.add(
        egui::Label::new(egui::RichText::new(line).color(theme::DIM))
            .truncate()
            .selectable(false),
    )
    .on_hover_text(line);
    ui.add_space(4.0);
}

/// Buckets a library run leaves on the shelf.
///
/// Ticked off rather than searched for: a dump of everything that skips the
/// voice folder is still a dump, and typing `-category:voice` into the search
/// box to get one is a thing nobody finds. What is left out here is left out
/// of `extract library` only — `extract shown` already writes exactly what is
/// on screen — and it is remembered between runs.
fn leaving_out(ui: &mut egui::Ui, state: &mut State) {
    if state.buckets.is_empty() {
        return;
    }
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(6.0);
    let gone: usize = state
        .buckets
        .iter()
        .filter(|(name, _)| state.settings.skip.iter().any(|skip| skip == name))
        .map(|(_, count)| count)
        .sum();
    ui.colored_label(
        theme::DIM,
        match gone {
            0 => "leave out of a library run".to_string(),
            _ => format!("leave out of a library run  ({} skipped)", widgets::tally(gone)),
        },
    );
    ui.add_space(2.0);
    let buckets = state.buckets.clone();
    let mut toggled: Option<&str> = None;
    ui.horizontal_wrapped(|ui| {
        for (name, count) in &buckets {
            let off = state.settings.skip.iter().any(|skip| skip == name);
            if widgets::chip(ui, &format!("{name} {}", widgets::tally(*count)), off)
                .on_hover_text(match off {
                    true => format!("{name} is left out"),
                    false => format!("leave {name} out"),
                })
                .clicked()
            {
                toggled = Some(name);
            }
        }
    });
    if let Some(name) = toggled {
        state.toggle_left_out(name);
    }
}

fn output(ui: &mut egui::Ui, state: &mut State) {
    ui.colored_label(theme::DIM, "output");
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for format in FORMATS {
            if widgets::chip(ui, format.label(), state.options.format == *format)
                .on_hover_text(format.about())
                .clicked()
            {
                state.options.format = *format;
                state.save_options();
            }
        }
    });
    tree(ui, state);

    let mut changed = false;
    changed |= ui
        .checkbox(&mut state.options.normalise_names, "safe names")
        .changed();
    changed |= ui
        .checkbox(&mut state.options.skip_duplicates, "skip existing")
        .changed();
    changed |= ui
        .checkbox(&mut state.options.write_manifest, "write manifest")
        .changed();
    if changed {
        state.save_options();
    }

    ui.horizontal(|ui| {
        if ui.button("folder").clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            state.set_output_dir(folder);
        }
        let shown = state
            .output
            .as_ref()
            .map(|p| p.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "ask each time".into());
        ui.colored_label(theme::DIM, crate::hm::discord::clip(&shown, 30));
    });

    leaving_out(ui, state);

    folders(ui, state);

    presets(ui, state);

    ui.horizontal(|ui| {
        if ui.button("extract shown").clicked() {
            state.selection.clear();
            state.extract(true);
        }
        if !state.selection.is_empty()
            && ui
                .button(format!("extract {}", state.selection.len()))
                .clicked()
        {
            state.extract(false);
        }
    });

    // The whole library, filter or no filter, into a folder named after the
    // game and the build it came out of.
    if !state.catalog.is_empty() {
        // What the button says is what the run will actually write, which is
        // the catalogue less whatever is being left out.
        let gone: usize = state
            .buckets
            .iter()
            .filter(|(name, _)| state.settings.skip.iter().any(|skip| skip == name))
            .map(|(_, count)| count)
            .sum();
        let count = widgets::tally(state.catalog.len().saturating_sub(gone));
        if ui
            .button(format!("extract library ({count})"))
            .on_hover_text(format!(
                "every sound this game has, into {}",
                state.library_folder(state.mounted.build.as_deref())
            ))
            .clicked()
        {
            state.extract_library();
        }
    }

    // An export that was put down, here or in an earlier run of harmony. It
    // only shows on the game it belongs to, and picking it up steps over every
    // file that is already written.
    if let Some(resume) = state.resume.clone()
        && state.title.map(|id| id.key()) == Some(resume.game.as_str())
    {
        ui.add_space(4.0);
        ui.colored_label(
            theme::DIM,
            format!(
                "put down {}: {} of {} written",
                resume.at,
                widgets::tally(resume.done),
                widgets::tally(resume.total)
            ),
        );
        ui.horizontal(|ui| {
            if ui
                .button("resume export")
                .on_hover_text(format!("carry on into {}", resume.folder))
                .clicked()
            {
                state.resume_export();
            }
            if ui.button("forget it").clicked() {
                state.forget_resume();
            }
        });
    }

    let mut discord = state.settings.discord;
    if ui.checkbox(&mut discord, "discord presence").changed() {
        state.settings.discord = discord;
        crate::hm::storage::save(&state.settings);
        if discord {
            state.rpc = crate::hm::discord::Rpc::new(&state.settings.discord_app_id);
            state.push_presence();
        } else {
            state.rpc = crate::hm::discord::Rpc::disabled();
        }
    }
}


/// Where harmony writes, and how much room is left there.
///
/// Three folders, and every one of them can be moved: the caches a scan
/// leaves, the scratch files a drag out of the window needs, and the folder an
/// extraction lands in. Each line says what is free on that disk, because the
/// number only matters where the writing happens.
fn folders(ui: &mut egui::Ui, state: &mut State) {
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(6.0);
    ui.colored_label(theme::DIM, "folders");

    let cache = crate::hm::storage::cache_dir();
    let disk = state.disk.clone();
    row(
        ui,
        "cache",
        &cache,
        disk.cache_free,
        Some(disk.cache_held),
        state.settings.cache_dir.is_some(),
    );
    ui.horizontal(|ui| {
        if ui.small_button("move cache").clicked()
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("where should scans be cached?")
                .pick_folder()
        {
            state.set_cache_dir(Some(folder));
        }
        if state.settings.cache_dir.is_some() && ui.small_button("default").clicked() {
            state.set_cache_dir(None);
        }
    });

    let scratch = crate::hm::window::dragout::scratch_dir();
    row(
        ui,
        "scratch",
        &scratch,
        disk.scratch_free,
        None,
        state.settings.temp_dir.is_some(),
    );
    ui.horizontal(|ui| {
        if ui.small_button("move scratch").clicked()
            && let Some(folder) = rfd::FileDialog::new()
                .set_title("where should drag-out files be written?")
                .pick_folder()
        {
            state.set_temp_dir(Some(folder));
        }
        if state.settings.temp_dir.is_some() && ui.small_button("default").clicked() {
            state.set_temp_dir(None);
        }
    });

    match state.output.clone() {
        Some(path) => row(ui, "export", &path, disk.export_free, None, true),
        None => widgets::field(ui, "export", "ask each time"),
    }
}

/// One folder line: where it is, what is free on that disk, and whether the
/// user chose it or harmony fell back to its own default.
fn row(
    ui: &mut egui::Ui,
    label: &str,
    path: &std::path::Path,
    free: Option<u64>,
    holding: Option<u64>,
    chosen: bool,
) {
    let text = path.to_string_lossy().to_ascii_lowercase();
    ui.horizontal(|ui| {
        ui.colored_label(theme::DIM, label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let room = match free {
                Some(bytes) => format!("{} free", widgets::bytes(bytes)),
                None => "room unknown".to_string(),
            };
            let colour = match free {
                Some(bytes) if bytes < crate::hm::storage::space::HEADROOM => theme::UNLIT,
                Some(_) => theme::GOOD,
                None => theme::DIM,
            };
            ui.colored_label(colour, room);
            if let Some(bytes) = holding.filter(|bytes| *bytes > 0) {
                ui.colored_label(theme::DIM, format!("{} held", widgets::bytes(bytes)));
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(
            if chosen { theme::TEXT } else { theme::DIM },
            crate::hm::discord::clip(&text, 44),
        )
        .on_hover_text(text);
    });
}

/// Extraction presets: a named format, layout and set of switches.
fn presets(ui: &mut egui::Ui, state: &mut State) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.colored_label(theme::DIM, "preset");
        let names: Vec<String> = state.settings.presets.keys().cloned().collect();
        egui::ComboBox::from_id_salt("preset")
            .selected_text(if names.is_empty() { "none saved" } else { "apply" })
            .width(110.0)
            .show_ui(ui, |ui| {
                for name in names {
                    if ui.selectable_label(false, &name).clicked() {
                        state.apply_preset(&name);
                    }
                }
            });
        ui.add_sized(
            egui::vec2(90.0, theme::INTERACT_HEIGHT),
            egui::TextEdit::singleline(&mut state.preset_name).hint_text("name"),
        );
        if ui.button("save").clicked() {
            let name = state.preset_name.clone();
            state.save_preset(&name);
        }
    });
}
