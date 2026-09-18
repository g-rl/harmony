use eframe::egui;

use crate::hm::app::State;
use crate::hm::ui::{View, theme, widgets};

pub fn show(ui: &mut egui::Ui, state: &mut State) {
    if state.catalog.is_empty() {
        empty(ui, state);
        return;
    }
    match state.view {
        View::Grid => grid(ui, state),
        View::Compact => rows(ui, state, false),
        View::Detailed => rows(ui, state, true),
        View::Waveform => waveforms(ui, state),
        View::Tree | View::TreePlus => tree(ui, state),
        View::Recent => recent(ui, state),
        View::Favorites => rows(ui, state, true),
    }
}

/// What colour a row is written in.
///
/// A favorite is lit, a sound harmony has named reads as text, and one that
/// is still a key gets its own colour: not an error, just a name harmony
/// could not recover.
fn ink(entry: &crate::hm::catalog::Entry, selected: bool) -> egui::Color32 {
    if entry.favorite {
        theme::ACCENT
    } else if selected {
        theme::WAVE
    } else if entry.name.resolved() {
        theme::TEXT
    } else {
        theme::UNNAMED
    }
}

fn empty(ui: &mut egui::Ui, state: &mut State) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);
        ui.colored_label(theme::DIM, "nothing scanned");
        ui.add_space(6.0);
        if state.root.is_none() && ui.button("folder").clicked() {
            state.pick_folder();
        } else if state.root.is_some() && !state.busy_here() && ui.button("scan").clicked() {
            state.start_scan();
        }
        if state.busy_here() {
            let (done, total) = state.here_progress().unwrap_or((0, 0));
            let fraction = if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            };
            ui.add_space(8.0);
            widgets::meter(ui, 240.0, fraction, theme::WAVE);
        }
    });
}

fn line(state: &State, index: usize, detailed: bool) -> String {
    let entry = &state.catalog.entries[index];
    if !detailed {
        return entry.display();
    }
    let package = state
        .mounted
        .packages
        .get(entry.package.0 as usize)
        .cloned()
        .unwrap_or_default();
    format!(
        "{:<28} {:<6} {:<5} {:<7} {:<8} {:<8} {}",
        crate::hm::discord::clip(&entry.display(), 28),
        entry.codec.label(),
        format!("{}ch", entry.channels),
        entry.rate,
        format!("{:.2}s", entry.seconds()),
        widgets::bytes(entry.bytes),
        package
    )
}

/// The header of the detailed view.
///
/// It is laid out in exactly the columns the rows use and pushed in by the same
/// padding a row's own widget adds, so each title sits over the first character
/// of its column. It is painted rather than laid out because it also has to
/// follow the list sideways when the list is scrolled.
fn heading(ui: &mut egui::Ui, offset: f32) {
    let text = format!(
        "{:<28} {:<6} {:<5} {:<7} {:<8} {:<8} {}",
        "name", "kind", "ch", "rate", "length", "size", "package"
    );
    let width = ui.available_width();
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, theme::ROW_HEIGHT), egui::Sense::hover());
    let at = egui::pos2(
        rect.left() + ui.style().spacing.button_padding.x - offset,
        rect.center().y,
    );
    ui.painter_at(rect).text(
        at,
        egui::Align2::LEFT_CENTER,
        text,
        egui::TextStyle::Body.resolve(ui.style()),
        theme::DIM,
    );
}

/// How far the list is scrolled sideways, as of the last frame. The header is
/// drawn before the list, so this is the only way it can know.
fn sideways(ui: &egui::Ui) -> f32 {
    ui.data(|data| data.get_temp::<f32>(egui::Id::new("rows_offset")).unwrap_or(0.0))
}


fn grid(ui: &mut egui::Ui, state: &mut State) {
    // Borrowed, not copied. The filtered list is one number per sound and a
    // hundred and twenty thousand of them; copying it to satisfy the borrow
    // checker was a megabyte of memcpy on every frame the list was on screen,
    // which is what the scrolling felt like. Taking it leaves an empty vec
    // behind for the length of the draw and puts the same allocation back.
    let list = std::mem::take(&mut state.filtered);
    // Fit whole tiles across whatever room the middle has.
    let room = ui.available_width();
    let columns = (room / 180.0).floor().max(1.0);
    let width = (room / columns) - 8.0;
    let columns = columns as usize;
    let mut clicked: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("grid")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("tiles").spacing([6.0, 6.0]).show(ui, |ui| {
                for (position, index) in list.iter().enumerate() {
                    let entry = &state.catalog.entries[*index];
                    let selected = state.cursor == Some(*index);
                    let frame = egui::Frame::new()
                        .fill(if selected { theme::PRESSED } else { theme::BG })
                        .stroke(egui::Stroke::new(
                            1.0,
                            if selected { theme::WAVE } else { theme::HAIRLINE },
                        ))
                        .inner_margin(egui::Margin::same(5));
                    let response = frame
                        .show(ui, |ui| {
                            ui.set_width(width - 16.0);
                            ui.colored_label(
                                ink(entry, false),
                                crate::hm::discord::clip(&entry.display(), 20),
                            );
                            ui.colored_label(
                                theme::DIM,
                                format!("{:.2}s  {}ch", entry.seconds(), entry.channels),
                            );
                        })
                        .response;
                    if response.interact(egui::Sense::click()).clicked() {
                        clicked = Some(*index);
                    }
                    if (position + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        });
    state.filtered = list;
    if let Some(index) = clicked {
        state.select(index, true);
    }
}

fn waveforms(ui: &mut egui::Ui, state: &mut State) {
    let list: Vec<usize> = state.filtered.iter().copied().take(400).collect();
    let mut clicked: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("waves")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for index in list {
                let entry = &state.catalog.entries[index];
                let selected = state.cursor == Some(index);
                ui.horizontal(|ui| {
                    ui.colored_label(
                        ink(entry, selected),
                        crate::hm::discord::clip(&entry.display(), 22),
                    );
                    let bins = state
                        .loaded
                        .as_ref()
                        .filter(|loaded| loaded.id == entry.id)
                        .map(|loaded| loaded.peaks.bins.clone())
                        .unwrap_or_default();
                    let width = ui.available_width() - 60.0;
                    let response =
                        widgets::waveform(ui, egui::vec2(width.max(80.0), 22.0), &bins, None);
                    if response.clicked() {
                        clicked = Some(index);
                    }
                    ui.colored_label(theme::DIM, format!("{:.2}s", entry.seconds()));
                });
            }
        });
    if let Some(index) = clicked {
        state.select(index, true);
    }
}

fn tree(ui: &mut egui::Ui, state: &mut State) {
    // The same again, and worse: the tree is a map of maps holding every sound
    // in the game, and it was being copied whole every frame purely to hand
    // `branch` something that was not borrowed from `state`.
    let root = std::mem::take(&mut state.tree);
    let mut clicked: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("browse-tree")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            branch(ui, state, &root, 0, &mut clicked);
        });
    state.tree = root;
    if let Some(index) = clicked {
        state.select(index, true);
    }
}

fn branch(
    ui: &mut egui::Ui,
    state: &State,
    node: &crate::hm::catalog::group::Node,
    depth: usize,
    clicked: &mut Option<usize>,
) {
    for (label, child) in &node.children {
        let header = format!("{label}  {}", widgets::tally(child.count));
        egui::CollapsingHeader::new(egui::RichText::new(header).color(theme::TEXT))
            .id_salt(format!("{depth}-{label}"))
            .default_open(depth == 0 && node.children.len() < 8)
            .show(ui, |ui| {
                branch(ui, state, child, depth + 1, clicked);
                for id in child.entries.iter().take(500) {
                    let index = id.0 as usize;
                    if let Some(entry) = state.catalog.entries.get(index)
                        && ui
                            .selectable_label(
                                state.cursor == Some(index),
                                egui::RichText::new(entry.display()).color(ink(entry, false)),
                            )
                            .clicked()
                    {
                        *clicked = Some(index);
                    }
                }
            });
    }
}

fn recent(ui: &mut egui::Ui, state: &mut State) {
    let list = state.recent.clone();
    let mut clicked: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("recent")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for id in list {
                let index = id.0 as usize;
                let Some(entry) = state.catalog.entries.get(index) else {
                    continue;
                };
                if ui
                    .selectable_label(
                        state.cursor == Some(index),
                        egui::RichText::new(entry.display()).color(ink(entry, false)),
                    )
                    .clicked()
                {
                    clicked = Some(index);
                }
            }
        });
    if let Some(index) = clicked {
        state.select(index, true);
    }
}
fn rows(ui: &mut egui::Ui, state: &mut State, detailed: bool) {
    if detailed {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        let offset = sideways(ui);
        heading(ui, offset);
        widgets::hairline(ui);
    }

    // Rows are one line each: let them run off to the side rather than wrap.
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);

    let list = std::mem::take(&mut state.filtered);
    let mut clicked: Option<(usize, bool, bool)> = None;
    let mut dragged: Option<usize> = None;
    let mut action: Option<Action> = None;
    let scrolled = egui::ScrollArea::both()
        .id_salt("rows")
        .auto_shrink([false, false])
        .show_rows(ui, theme::ROW_HEIGHT, list.len(), |ui, range| {
            for position in range {
                let index = list[position];
                let picked = state.cursor == Some(index) || state.selection.contains(&index);
                let entry = &state.catalog.entries[index];
                let colour = ink(entry, false);
                let text = egui::RichText::new(line(state, index, detailed)).color(colour);
                let response = ui
                    .selectable_label(picked, text)
                    .interact(egui::Sense::click_and_drag());
                if response.clicked() {
                    let (ctrl, shift) =
                        ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                    clicked = Some((index, ctrl, shift));
                }
                // Pulling a row off the list is a drag out of the window: the
                // files are written while the card is up, then handed over.
                if response.drag_started() {
                    dragged = Some(index);
                }
                response.context_menu(|ui| menu(ui, state, index, &mut action));
            }
        });
    // Kept for the header, which is drawn before the list gets a chance to say
    // where it has been scrolled to.
    ui.data_mut(|data| data.insert_temp(egui::Id::new("rows_offset"), scrolled.state.offset.x));
    // Back before anything below it reads the list again.
    state.filtered = list;
    if let Some(action) = action {
        run(state, action);
    }
    if let Some((index, ctrl, shift)) = clicked {
        pick(state, index, ctrl, shift);
    }
    if let Some(index) = dragged {
        state.drag_out(Some(index));
    }
}

/// What a row's right-click menu asked for, applied once the list is done with.
pub enum Action {
    Favorite(usize),
    Drag(usize),
    Extract(usize),
    Similar(usize),
    Tag(usize, String),
    SelectBranch(usize),
}

fn menu(ui: &mut egui::Ui, state: &State, index: usize, action: &mut Option<Action>) {
    if ui.button("play").clicked() {
        *action = Some(Action::Similar(index));
        ui.close();
    }
    if ui.button("extract").clicked() {
        *action = Some(Action::Extract(index));
        ui.close();
    }
    if ui.button("drag out").clicked() {
        *action = Some(Action::Drag(index));
        ui.close();
    }
    if ui.button("favorite").clicked() {
        *action = Some(Action::Favorite(index));
        ui.close();
    }
    if ui.button("select the rest of this group").clicked() {
        *action = Some(Action::SelectBranch(index));
        ui.close();
    }
    ui.separator();
    for tag in ["keep", "loud", "layer", "variation"] {
        if ui.button(format!("tag {tag}")).clicked() {
            *action = Some(Action::Tag(index, tag.to_string()));
            ui.close();
        }
    }
    if ui.button("copy name").clicked() {
        ui.ctx().copy_text(state.catalog.entries[index].display());
        ui.close();
    }
}

fn run(state: &mut State, action: Action) {
    match action {
        Action::Favorite(index) => state.toggle_favorite(index),
        Action::Drag(index) => state.drag_out(Some(index)),
        Action::Extract(index) => {
            state.selection.clear();
            state.selection.insert(index);
            state.extract(false);
        }
        Action::Similar(index) => state.select(index, true),
        Action::Tag(index, tag) => state.tag(index, &tag),
        Action::SelectBranch(index) => {
            let Some(entry) = state.catalog.entries.get(index) else {
                return;
            };
            let branch = state.branch_label(entry);
            state.selection = state
                .filtered
                .iter()
                .copied()
                .filter(|other| {
                    state.branch_label(&state.catalog.entries[*other]) == branch
                })
                .collect();
        }
    }
}

/// Plain click auditions, ctrl adds to the selection, shift takes a run.
fn pick(state: &mut State, index: usize, ctrl: bool, shift: bool) {
    if ctrl {
        if !state.selection.insert(index) {
            state.selection.remove(&index);
        }
        state.cursor = Some(index);
        return;
    }
    if shift && let Some(from) = state.cursor {
        let positions = |want: usize| state.filtered.iter().position(|i| *i == want);
        if let (Some(a), Some(b)) = (positions(from), positions(index)) {
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            for position in low..=high {
                state.selection.insert(state.filtered[position]);
            }
        }
        state.cursor = Some(index);
        return;
    }
    state.selection.clear();
    state.select(index, true);
}
