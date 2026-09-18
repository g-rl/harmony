use eframe::egui;

use crate::hm::app::{State, support_of, title_label};
use crate::hm::catalog::group::{self, GroupBy};
use crate::hm::game::TitleId;
use crate::hm::scan::DEPTHS;
use crate::hm::ui::{VIEWS, theme, widgets};
use crate::hm::window::chrome;

/// The tab strip, newest first, then the older engines.
const KNOWN: &[TitleId] = &[
    TitleId::T11,
    TitleId::T10,
    TitleId::Jup,
    TitleId::Iw9,
    TitleId::Iw8,
    TitleId::Iw7,
    TitleId::T7,
    TitleId::S1,
    TitleId::H1,
    TitleId::Iw5,
    TitleId::H2,
    TitleId::T6,
    TitleId::T5,
    TitleId::Iw6,
    TitleId::Iw4,
    TitleId::Iw3,
    TitleId::T4,
];

pub fn strip(ui: &mut egui::Ui, state: &mut State) {
    title_bar(ui, state);

    ui.horizontal_wrapped(|ui| {
        let mut wanted: Option<TitleId> = None;
        let mut forget: Option<TitleId> = None;
        for id in KNOWN {
            let found = state.detected.iter().any(|print| print.title == *id);
            let known = state.root_for(*id);
            let on = state.title == Some(*id);
            // Three states, and they are all clickable: this game, a game whose
            // folder harmony has been shown, and one it has not seen yet.
            let colour = if on {
                theme::WAVE
            } else if found || known.is_some() {
                theme::TEXT
            } else {
                theme::DIM
            };
            let button = egui::Button::new(egui::RichText::new(id.abbr()).color(colour))
                .fill(if on { theme::PRESSED } else { theme::PANEL })
                .stroke(egui::Stroke::new(
                    1.0,
                    if on {
                        theme::WAVE
                    } else if known.is_some() {
                        theme::HAIRLINE
                    } else {
                        theme::PANEL
                    },
                ));
            let note = match (&known, found) {
                (_, true) => "in this folder".to_string(),
                (Some(path), _) => path.to_string_lossy().to_ascii_lowercase(),
                (None, _) => "click to point harmony at it".to_string(),
            };
            let mut hit = ui.add(button);
            // A game being scanned says so on its own tab, whether or not it is
            // the one on screen: three of them can be running at once.
            let working = state.work.get(id).cloned();
            if let Some(work) = &working {
                let fraction = match work.total {
                    0 => 0.05,
                    total => (work.done as f32 / total as f32).clamp(0.05, 1.0),
                };
                // Inside the tab, not across it: the same inset as the
                // button's own border, in the colour the rest of the progress
                // uses rather than the accent.
                let rect = hit.rect.shrink2(egui::vec2(2.0, 0.0));
                let track = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), rect.bottom() - 3.0),
                    egui::vec2(rect.width(), 2.0),
                );
                ui.painter()
                    .rect_filled(track, 1.0, theme::HAIRLINE);
                let line = egui::Rect::from_min_size(
                    track.min,
                    egui::vec2(track.width() * fraction, 2.0),
                );
                ui.painter()
                    .rect_filled(line, 1.0, theme::WAVE.gamma_multiply(0.8));
                hit = hit.on_hover_text(format!(
                    "{} \u{b7} {} scan \u{b7} {} of {}\n{}",
                    title_label(*id),
                    work.depth.label(),
                    widgets::tally(work.done),
                    widgets::tally(work.total),
                    work.what
                ));
            } else {
                hit = hit.on_hover_text(format!(
                    "{}  \u{b7}  {}\n{note}",
                    title_label(*id),
                    support_of(*id).label()
                ));
            }
            if hit.clicked() {
                wanted = Some(*id);
            }
            // Double-clicking the tab that is already open is the way to move
            // a game: it asks for the folder rather than doing nothing.
            if hit.double_clicked() {
                wanted = None;
                forget = Some(*id);
            }
            hit.context_menu(|ui| {
                if ui.button("point at another folder").clicked() {
                    wanted = None;
                    forget = Some(*id);
                    ui.close();
                }
            });
        }
        if let Some(id) = forget {
            state.pick_folder_for(id);
        } else if let Some(id) = wanted {
            state.switch(id);
        }
        if state.looking.is_some() {
            ui.colored_label(theme::DIM, "looking...");
        } else if state.detected.is_empty() && state.root.is_some() {
            ui.colored_label(theme::UNLIT, "unknown game");
        }
        if ui.button("folder").clicked() {
            state.pick_folder();
        }
        if ui.button("names").clicked() {
            state.pick_names();
        }
        if state.names.is_empty() {
            ui.colored_label(theme::DIM, "no name list loaded");
        } else {
            ui.colored_label(
                theme::GOOD,
                format!("{} names", widgets::tally(state.names.len())),
            );
        }
    });

    // Wrapped, so a narrow window puts the controls on a second line rather
    // than running them off the edge. The search field takes what is going and
    // no more.
    ui.horizontal_wrapped(|ui| {
        let room = (ui.available_width() - 340.0).clamp(120.0, 320.0);
        let field = ui.add_sized(
            egui::vec2(room, theme::INTERACT_HEIGHT),
            egui::TextEdit::singleline(&mut state.query_text)
                .hint_text("search")
                .desired_width(room),
        );
        if field.changed() {
            state.refilter();
        }
        egui::ComboBox::from_id_salt("group")
            .selected_text(state.group_by.label())
            .width(110.0)
            .show_ui(ui, |ui| {
                for by in group::ALL {
                    if ui
                        .selectable_label(state.group_by == *by, by.label())
                        .clicked()
                    {
                        state.group_by = *by;
                        state.tree_pick = None;
                        state.refilter();
                    }
                }
            });
        egui::ComboBox::from_id_salt("view")
            .selected_text(view_text(ui, state.view))
            .width(110.0)
            .show_ui(ui, |ui| {
                for view in VIEWS {
                    if ui
                        .selectable_label(state.view == *view, view_text(ui, *view))
                        .clicked()
                    {
                        state.view = *view;
                        state.refilter();
                    }
                }
            });
        egui::ComboBox::from_id_salt("depth")
            .selected_text(state.depth.label())
            .width(80.0)
            .show_ui(ui, |ui| {
                for depth in DEPTHS {
                    if ui
                        .selectable_label(
                            state.depth == *depth,
                            format!("{}  {}", depth.label(), depth.note()),
                        )
                        .clicked()
                    {
                        state.depth = *depth;
                    }
                }
            });
        if state.busy_here() {
            if ui.button("stop").clicked() {
                state.stop_scan();
            }
        } else if ui.button("scan").clicked() {
            state.start_scan();
        }
        if !state.busy_here()
            && ui
                .button("refresh")
                .on_hover_text("check the files against the cached scan, and rescan if they moved")
                .clicked()
        {
            state.refresh();
        }
        if ui.button("extract").clicked() {
            state.extract(false);
        }
        if ui.button("queue").clicked() {
            state.show_queue = !state.show_queue;
        }
    });
}

/// The window's own title bar: the name and the icon on the left, the window
/// buttons on the far right, and the whole row is what the window is carried
/// by. There is no system bar above it.
fn title_bar(ui: &mut egui::Ui, state: &mut State) {
    let ctx = ui.ctx().clone();
    let mut taken: Vec<egui::Rect> = Vec::new();

    let row = ui
        .horizontal(|ui| {
            if let Some(icon) = state.icon.clone() {
                ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(20.0, 20.0)));
            }
            ui.label(egui::RichText::new("harmony").color(theme::WAVE));
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    let close = widgets::tiny(ui, "\u{00d7}").on_hover_text("close");
                    if close.clicked() {
                        chrome::close(&ctx);
                    }
                    let fill = widgets::tiny(ui, "\u{25a1}").on_hover_text(
                        if chrome::maximized(&ctx) {
                            "give the screen back"
                        } else {
                            "fill the screen"
                        },
                    );
                    if fill.clicked() {
                        chrome::toggle_maximize(&ctx);
                    }
                    let small = widgets::tiny(ui, "\u{2013}").on_hover_text("minimise");
                    if small.clicked() {
                        chrome::minimize(&ctx);
                    }
                    taken.extend([close.rect, fill.rect, small.rect]);
                    widgets::hairline(ui);
                },
            );
        })
        .response
        .interact(egui::Sense::click_and_drag());

    // The buttons are on the bar, so the bar must not answer for them.
    let over_button = ctx
        .pointer_latest_pos()
        .is_some_and(|at| taken.iter().any(|rect| rect.contains(at)));
    if over_button {
        return;
    }
    if row.double_clicked() {
        chrome::toggle_maximize(&ctx);
    } else if row.drag_started() {
        chrome::drag(&ctx);
    }
}

pub fn card(ui: &mut egui::Ui, state: &mut State) {
    egui::ScrollArea::vertical()
        .id_salt("card")
        .auto_shrink([false, false])
        .show(ui, |ui| card_body(ui, state));
}

fn card_body(ui: &mut egui::Ui, state: &mut State) {
    let print = state.print().cloned();
    let id = state.title;

    match id {
        Some(id) => {
            widgets::field(ui, "game", title_label(id));
            widgets::field(ui, "id", id.key());
        }
        None => widgets::field(ui, "game", "none"),
    }
    widgets::field(
        ui,
        "path",
        &state
            .root
            .as_ref()
            .map(|p| p.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_else(|| "-".into()),
    );
    if let Some(print) = print.as_ref() {
        widgets::field(ui, "found", &print.reason);
        widgets::field(
            ui,
            "sounds in",
            &print
                .roots
                .iter()
                .filter_map(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("  "),
        );
        widgets::field(ui, "build", print.build.as_deref().unwrap_or("-"));
    }
    if let Some(id) = id {
        widgets::field(ui, "support", support_of(id).label());
        if let Some(title) = crate::hm::game::title_for(id) {
            widgets::field(ui, "containers", &title.containers().join("  "));
        }
    }
    widgets::field(ui, "formats", &formats_found(state));
    if !state.mounted.container.is_empty() {
        widgets::field(ui, "reads", &state.mounted.container);
    }
    widgets::field(
        ui,
        "packages",
        &widgets::tally(state.mounted.packages.len()),
    );
    widgets::field(ui, "keys", &widgets::tally(state.mounted.keys));
    widgets::field(
        ui,
        "discovered",
        &widgets::tally(state.catalog.len()),
    );
    ui.horizontal(|ui| {
        widgets::field(
            ui,
            "last scan",
            state.catalog.scanned_at.as_deref().unwrap_or("-"),
        );
        if state.catalog.scanned_at.is_some() && ui.small_button("forget").clicked() {
            state.forget_cache();
        }
    });
    cache_line(ui, state);

    depth_note(ui, state);
    since_last_scan(ui, state);

    section(ui, "harmony reads", false, |ui| supported(ui, state));
    section(ui, "find", true, |ui| buckets(ui, state));
    section(ui, "collections", true, |ui| collections(ui, state));

    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(6.0);

    let picked = state.tree_pick.clone();
    egui::ScrollArea::vertical()
        .id_salt("tree")
        .show(ui, |ui| {
            let all = state.tree.count;
            if ui
                .selectable_label(picked.is_none(), format!("all  {}", widgets::tally(all)))
                .clicked()
            {
                state.tree_pick = None;
                state.refilter();
            }
            let branches: Vec<(String, usize)> = state
                .tree
                .children
                .iter()
                .map(|(label, node)| (label.clone(), node.count))
                .collect();
            for (label, count) in branches {
                let on = picked.as_deref() == Some(label.as_str());
                if ui
                    .selectable_label(on, format!("{label}  {}", widgets::tally(count)))
                    .clicked()
                {
                    state.tree_pick = if on { None } else { Some(label.clone()) };
                    state.refilter();
                }
            }
        });
}

pub fn group_label(by: GroupBy) -> &'static str {
    by.label()
}


/// Where this scan is cached and what is free there. A scan of a big install
/// runs to tens of megabytes, so the card says where they are landing rather
/// than leaving it to be found out when the disk is full.
fn cache_line(ui: &mut egui::Ui, state: &State) {
    let dir = crate::hm::storage::cache_dir();
    let free = state.disk.cache_free;
    let text = dir.to_string_lossy().to_ascii_lowercase();
    ui.horizontal(|ui| {
        widgets::field(ui, "caching to", &crate::hm::discord::clip(&text, 26));
        if let Some(bytes) = free {
            let colour = if bytes < crate::hm::storage::space::HEADROOM {
                theme::UNLIT
            } else {
                theme::DIM
            };
            ui.colored_label(colour, format!("{} free", widgets::bytes(bytes)));
        }
    })
    .response
    .on_hover_text(text);
}

/// Collections and what changed since the last scan: the left card is where
/// harmony says what it knows about this install.
fn collections(ui: &mut egui::Ui, state: &mut State) {

    let names: Vec<(String, usize)> = state
        .collections
        .iter()
        .map(|(name, keys)| (name.clone(), keys.len()))
        .collect();
    if names.is_empty() {
        ui.colored_label(theme::DIM, "none yet");
    }
    let mut drop: Option<String> = None;
    for (name, count) in names {
        let on = state.collection_pick.as_deref() == Some(name.as_str());
        let response = ui.selectable_label(on, format!("{name}  {}", widgets::tally(count)));
        if response.clicked() {
            state.collection_pick = if on { None } else { Some(name.clone()) };
            state.refilter();
        }
        response.context_menu(|ui| {
            if ui.button("forget this collection").clicked() {
                drop = Some(name.clone());
                ui.close();
            }
        });
    }
    if let Some(name) = drop {
        state.drop_collection(&name);
    }

    ui.horizontal(|ui| {
        ui.add_sized(
            egui::vec2(110.0, theme::INTERACT_HEIGHT),
            egui::TextEdit::singleline(&mut state.collection_name).hint_text("new collection"),
        );
        if ui.button("add").clicked() {
            let name = state.collection_name.clone();
            let picks: Vec<usize> = if state.selection.is_empty() {
                state.cursor.into_iter().collect()
            } else {
                state.selection.iter().copied().collect()
            };
            state.collect(&name, &picks);
        }
    });
}

/// What the last scan of this game found, so a patch that moves sounds about
/// is visible rather than silent.
fn since_last_scan(ui: &mut egui::Ui, state: &State) {
    let Some(id) = state.title else {
        return;
    };
    let Some(last) = state.settings.scans.get(id.key()) else {
        return;
    };
    ui.add_space(6.0);
    widgets::hairline(ui);
    ui.add_space(4.0);
    let (note, colour) = match state.catalog.len() {
        0 => (String::new(), theme::DIM),
        now => {
            let change = now as i64 - last.sounds as i64;
            match change {
                0 => ("no change".to_string(), theme::DIM),
                n if n > 0 => (format!("+{} new", widgets::tally(n as usize)), theme::ACCENT),
                n => (
                    format!("-{} gone", widgets::tally(n.unsigned_abs() as usize)),
                    theme::ACCENT,
                ),
            }
        }
    };
    widgets::field_note(
        ui,
        "seen before",
        &widgets::tally(last.sounds),
        &note,
        colour,
    );
}

/// Quick ways into the parts of a catalogue nobody goes looking for.
fn buckets(ui: &mut egui::Ui, state: &mut State) {
    const BUCKETS: &[(&str, &str)] = &[
        ("unnamed", "named:no"),
        ("named", "named:yes"),
        ("stereo", "channels:2"),
        ("very short", "length:<0.4"),
        ("long", "length:>10"),
        ("favorites", "favorite:yes"),
    ];
    ui.horizontal_wrapped(|ui| {
        for (label, query) in BUCKETS {
            let on = state.query_text == *query;
            if widgets::chip(ui, label, on).clicked() {
                state.query_text = if on { String::new() } else { (*query).to_string() };
                state.refilter();
            }
        }
    });
}

/// What harmony can read today, said plainly, whatever is selected.
/// Which codecs the catalogue actually holds, rather than a guess made before
/// anything was read.
fn formats_found(state: &State) -> String {
    let mut seen: Vec<&'static str> = Vec::new();
    for entry in &state.catalog.entries {
        let label = entry.codec.label();
        if !seen.contains(&label) {
            seen.push(label);
        }
        if seen.len() >= 5 {
            break;
        }
    }
    if seen.is_empty() {
        "-".into()
    } else {
        seen.join("  ")
    }
}

/// Say plainly what the last scan covered. A quick scan of a kapi install reads
/// the localised packages, which hold dialogue and nothing else, and that has
/// looked like a bug often enough to be worth a line in the card.
fn depth_note(ui: &mut egui::Ui, state: &State) {
    if state.catalog.is_empty() || state.depth != crate::hm::scan::Depth::Quick {
        return;
    }
    let voice = state
        .catalog
        .entries
        .iter()
        .filter(|entry| entry.category == crate::hm::catalog::category::Category::Voice)
        .count();
    if voice * 4 < state.catalog.len() * 3 {
        return;
    }
    ui.colored_label(
        theme::UNLIT,
        "quick reads the localised packages, which are dialogue only",
    );
    ui.colored_label(theme::DIM, "scan again at deep for weapons, music, ambience");
}

/// A view's name, with whatever part of it the view wants lit.
fn view_text(ui: &egui::Ui, view: crate::hm::ui::View) -> egui::WidgetText {
    // The body font, because every other control in this row is drawn in it
    // and a picker in a smaller one reads as a mistake.
    let font = egui::TextStyle::Body.resolve(ui.style());
    let (name, mark) = view.split();
    let mut text = egui::text::LayoutJob::default();
    text.append(
        name,
        0.0,
        egui::TextFormat {
            font_id: font.clone(),
            color: theme::TEXT,
            ..Default::default()
        },
    );
    if !mark.is_empty() {
        text.append(
            mark,
            0.0,
            egui::TextFormat {
                font_id: font,
                color: theme::ACCENT,
                ..Default::default()
            },
        );
    }
    text.into()
}

/// The folders a title reads, as a line for the card.
///
/// A title that reads the install root itself says so in words: a bare `.` in
/// the card reads like a mistake.
fn folders_of(roots: &[&str]) -> String {
    roots
        .iter()
        .map(|root| match *root {
            crate::hm::game::legacy::HERE => "install folder + languages",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn supported(ui: &mut egui::Ui, state: &State) {
    // The card is a narrow column: a line too long for it is cut, never folded
    // onto a second line that reads like another game.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    for id in KNOWN {
        let support = support_of(*id);
        let folders = crate::hm::game::title_for(*id)
            .map(|title| folders_of(title.sound_roots()))
            .unwrap_or_default();
        let here = state.detected.iter().any(|print| print.title == *id);
        // The game in the folder that is open is marked at the start of its
        // line and lit, rather than labelled at the end where the label runs
        // off the edge of the card.
        let colour = if here {
            theme::WAVE
        } else {
            match support {
                crate::hm::game::Support::Verified => theme::GOOD,
                crate::hm::game::Support::Expected | crate::hm::game::Support::Partial => {
                    theme::TEXT
                }
                _ => theme::DIM,
            }
        };
        let label = title_label(*id);
        let row = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let wide = ui.available_width();
                // The label column takes what it can and no more. Padding a
                // proportional font with spaces only lines up by luck, and
                // `modern warfare 2 campaign remastered` is where the luck ran
                // out: it pushed every column after it off its own line.
                let name = (wide * 0.40).clamp(96.0, 190.0);
                cell(ui, 10.0, if here { "\u{25b8}" } else { " " }, colour);
                cell(ui, 44.0, id.abbr(), colour);
                cell(ui, name, label, colour);
                cell(ui, 62.0, support.label(), colour);
                cell(ui, ui.available_width().max(1.0), &folders, colour);
            })
            .response;
        row.on_hover_text(match here {
            true => format!("{label}: this folder"),
            false => format!("{label}: {folders}"),
        });
    }
}

/// One column of a row: a fixed width, and whatever does not fit is cut.
///
/// Every cell is allocated the same width on every row, so the columns line up
/// whatever the font does with the letters in them.
fn cell(ui: &mut egui::Ui, width: f32, text: &str, colour: egui::Color32) {
    ui.add_sized(
        egui::vec2(width, theme::ROW_HEIGHT),
        egui::Label::new(egui::RichText::new(text).color(colour))
            .truncate()
            .selectable(false),
    );
}

/// A foldable block of the card, so a long left column stays readable.
fn section(ui: &mut egui::Ui, title: &str, open: bool, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(4.0);
    egui::CollapsingHeader::new(egui::RichText::new(title).color(theme::DIM))
        .id_salt(title)
        .default_open(open)
        .show(ui, body);
}
