//! The console: tilde, and everything harmony has been doing is on screen.
//!
//! It is not a terminal and does not pretend to be one. A terminal has one
//! stream and colours words in it; this has channels, levels and detail, and
//! draws each of them in its own lane so that a thousand lines can be looked
//! at rather than read:
//!
//! ```text
//!   21:04:11  ▍export  done   lane 1: bo3 · 27,605 sounds finished
//!                             written 27,601 of 27,605, 4 failed
//!                             took    22m 54s
//! ```
//!
//! The clock on the left, then the channel in its own colour, then what kind
//! of line it is, then the sentence. Anything with more to say carries it
//! underneath, folded unless it went wrong, so the shape of a long run is
//! readable from across the room and the detail is one click away.
//!
//! Channels are the point. A library extract writes a line a second and a scan
//! writes ten; turning everything off but the one being watched is a click or
//! a number key, and the filters are remembered while harmony is open.

use std::collections::HashSet;

use eframe::egui;

use crate::hm::app::State;
use crate::hm::console::{self, Channel, Level, Line};
use crate::hm::ui::{theme, widgets};

/// How much of the window the console covers when it has never been moved.
const HEIGHT: f32 = 0.55;
/// The smallest it can be dragged to and still be worth having open.
const NARROWEST: f32 = 380.0;
const SHORTEST: f32 = 160.0;
/// The strip along the top the console is not allowed onto. Harmony wears no
/// system title bar, so its own one carries close, maximise and the drag that
/// moves the window: a console lying across it is a window that cannot be
/// closed.
const TITLE_BAR: f32 = 34.0;
/// How long it takes to arrive, and how long to leave. Leaving is quicker:
/// something being put away should not be waited on.
const IN: f64 = 0.20;
const OUT: f64 = 0.14;
/// How many completions are offered at once.
const OFFERS: usize = 8;
/// How far the sentence sits from the left edge, in characters of the lane in
/// front of it: clock, channel, level.
const LANE: f32 = 8.0;
const CLOCK_LANE: f32 = 62.0;
const CHANNEL_LANE: f32 = 62.0;
const LEVEL_LANE: f32 = 34.0;
const HISTORY: usize = 80;

pub struct Console {
    pub open: bool,
    pub input: String,
    history: Vec<String>,
    /// How far back through the history the arrows have walked.
    walk: Option<usize>,
    /// What was being typed when that walk started.
    held: String,
    channels: [bool; Channel::ALL.len()],
    levels: [bool; Level::ALL.len()],
    /// Only lines with this in them.
    find: String,
    /// Lines whose detail has been unfolded by hand, and lines whose detail
    /// has been folded away by hand. Everything else follows its level:
    /// anything that went wrong shows its detail without being asked.
    unfolded: HashSet<u64>,
    folded: HashSet<u64>,
    /// Which completion is highlighted.
    pick: usize,
    /// The prompt is owed the caret.
    focus: bool,
    /// Where it was left: moved, resized, and kept between sessions.
    rect: Option<egui::Rect>,
    /// What the animation is doing: which way it is going, and since when.
    /// Kept here rather than in the toggle, because the console is opened
    /// from four places and none of them know what time it is.
    showing: bool,
    at: f64,
}

impl Default for Console {
    fn default() -> Console {
        Console {
            open: false,
            input: String::new(),
            history: Vec::new(),
            walk: None,
            held: String::new(),
            // Everything but debug, which is a burst of detail the console can
            // ask for rather than something to wade through.
            channels: Channel::ALL.map(|channel| channel != Channel::Debug),
            levels: [true; Level::ALL.len()],
            find: String::new(),
            unfolded: HashSet::new(),
            folded: HashSet::new(),
            pick: 0,
            focus: false,
            rect: None,
            showing: false,
            at: 0.0,
        }
    }
}

impl Console {
    pub fn show(&mut self) {
        self.open = true;
        self.focus = true;
        console::take_unread();
    }

    pub fn hide(&mut self) {
        self.open = false;
        self.walk = None;
    }

    pub fn toggle(&mut self) {
        match self.open {
            true => self.hide(),
            false => self.show(),
        }
    }

    fn shows(&self, line: &Line) -> bool {
        if !self.channels[slot(line.channel)] || !self.levels[level_slot(line.level)] {
            return false;
        }
        if self.find.is_empty() {
            return true;
        }
        let needle = self.find.to_ascii_lowercase();
        line.text.to_ascii_lowercase().contains(&needle)
            || line
                .under
                .iter()
                .any(|row| row.to_ascii_lowercase().contains(&needle))
    }

    /// Is this line's detail on show?
    ///
    /// A warning or a failure says everything it knows without being asked —
    /// that is the moment somebody wants it — and anything else keeps its
    /// detail folded until it is clicked.
    fn open_row(&self, line: &Line) -> bool {
        if self.unfolded.contains(&line.seq) {
            return true;
        }
        if self.folded.contains(&line.seq) {
            return false;
        }
        line.level.loud() || line.level == Level::Reply
    }

    fn fold(&mut self, line: &Line) {
        match self.open_row(line) {
            true => {
                self.unfolded.remove(&line.seq);
                self.folded.insert(line.seq);
            }
            false => {
                self.folded.remove(&line.seq);
                self.unfolded.insert(line.seq);
            }
        }
    }

    fn remember(&mut self, line: &str) {
        self.walk = None;
        self.held.clear();
        if line.trim().is_empty() || self.history.last().map(String::as_str) == Some(line) {
            return;
        }
        self.history.push(line.to_string());
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
    }

    fn earlier(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.walk {
            None => {
                self.held = self.input.clone();
                0
            }
            Some(at) => (at + 1).min(self.history.len() - 1),
        };
        self.walk = Some(next);
        self.input = self.history[self.history.len() - 1 - next].clone();
    }

    fn later(&mut self) {
        let Some(at) = self.walk else {
            return;
        };
        if at == 0 {
            self.walk = None;
            self.input = std::mem::take(&mut self.held);
            return;
        }
        self.walk = Some(at - 1);
        self.input = self.history[self.history.len() - 1 - (at - 1)].clone();
    }
}

/// Fast at first, slow into place: what something arriving looks like.
fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Slow to start, quick away: what something leaving looks like.
fn ease_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

fn slot(channel: Channel) -> usize {
    Channel::ALL
        .into_iter()
        .position(|other| other == channel)
        .unwrap_or(0)
}

fn level_slot(level: Level) -> usize {
    Level::ALL
        .into_iter()
        .position(|other| other == level)
        .unwrap_or(0)
}

fn tint(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// One drawn row: a line, or one of the rows of detail underneath it.
#[derive(Clone, Copy)]
enum Row {
    Head(usize),
    Under(usize, usize),
}

pub fn window(ctx: &egui::Context, state: &mut State) {
    let now = ctx.input(|input| input.time);
    // Which way it is going, and how far through. Worked out here because
    // opening happens from the tilde, the status bar, a command and a click,
    // and none of them are holding a clock.
    let flipped = state.console.showing != state.console.open;
    if flipped {
        state.console.showing = state.console.open;
        state.console.at = now;
    }
    let age = now - state.console.at;
    let going = match state.console.open {
        true => (age / IN).min(1.0) as f32,
        false => (age / OUT).min(1.0) as f32,
    };
    // Shut, and the sliding away has finished: nothing to draw.
    if !state.console.open && going >= 1.0 {
        return;
    }
    // Taken out of the app while it draws, so the console can read the rest of
    // the app for its completions without borrowing it twice. What is taken is
    // replaced by a default one, so the three things this frame has already
    // worked out are carried across by hand: putting `open` back as `true`
    // would reopen a console that is in the middle of leaving, every frame,
    // which is a console that cannot be shut.
    let open = state.console.open;
    let showing = state.console.showing;
    let at = state.console.at;
    let mut console = std::mem::take(&mut state.console);
    console.open = open;
    console.showing = showing;
    console.at = at;

    // Where it was left last, or a lid across the top of the window the first
    // time it is ever opened.
    let screen = ctx.input(|input| input.viewport_rect());
    // Everything below harmony's own title bar. The console lives in here and
    // cannot be dragged or grown out of it.
    let room = egui::Rect::from_min_max(
        screen.min + egui::vec2(2.0, TITLE_BAR),
        screen.max - egui::vec2(2.0, 2.0),
    );
    let start = console
        .rect
        .or_else(|| {
            state
                .settings
                .console_rect
                .map(|[x, y, width, height]| {
                    egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(width, height),
                    )
                })
        })
        .unwrap_or_else(|| {
            let height = (room.height() * HEIGHT).clamp(220.0, room.height());
            egui::Rect::from_min_size(room.min, egui::vec2(room.width(), height))
        });
    let mut typed: Option<String> = None;
    let held = console.rect.unwrap_or(start);

    // Out the way it came: up when it is sitting in the top half of the
    // window, down when it has been dragged to the bottom. An eased curve
    // rather than a straight one, so it reads as something with weight behind
    // it rather than a rectangle being teleported.
    let away = match held.center().y < room.center().y {
        true => -held.height() - 12.0,
        false => screen.height() - held.min.y + 12.0,
    };
    let travel = match console.open {
        true => away * (1.0 - ease_out(going)),
        false => away * ease_in(going),
    };
    let solid = match console.open {
        true => 0.25 + 0.75 * ease_out(going),
        false => 1.0 - ease_in(going),
    };
    let moving = going < 1.0;
    if moving {
        ctx.request_repaint();
    }

    let mut sheet = egui::Window::new("console")
        .id(egui::Id::new("console"))
        // No title bar of its own: the head row is the handle, and dragging
        // any of the quiet parts of it moves the whole thing.
        .title_bar(false)
        .resizable(true)
        .movable(true)
        .order(egui::Order::Foreground)
        .default_rect(start)
        .min_width(NARROWEST)
        .min_height(SHORTEST)
        // Never taller than the room it has, never dragged so far that there
        // is nothing left to take hold of, and never over the title bar.
        .max_height(room.height())
        .max_width(room.width())
        .constrain(true)
        .constrain_to(room)
        .frame(
            egui::Frame::new()
                // Solid: the list underneath showing through a console is a
                // wall of two things at once, and neither can be read.
                .fill(theme::BG)
                .stroke(egui::Stroke::new(1.0, theme::HAIRLINE))
                .inner_margin(egui::Margin::symmetric(10, 6)),
        );
    // While it is arriving or leaving, where it sits belongs to the animation;
    // the moment it settles the window belongs to whoever is using it again,
    // to be dragged and resized like any other.
    if moving {
        sheet = sheet.current_pos(held.min + egui::vec2(0.0, travel));
    }
    let shown = sheet.show(ctx, |ui| {
        // The height the window actually has this frame. It is only a real
        // answer because the window is given a max height: without one an
        // auto-sized window offers the rest of the screen, and the scrollback
        // grows to fill it. Taking it from the rect of the frame before would
        // be worse still - the content would then be sized by the window and
        // the window by the content, and a drag on the corner would spring
        // straight back to where it was.
        // A hair under, never over. Resize grows the window back to whatever
        // the content measured last frame, so content that comes out one pixel
        // taller than the window walks it down the screen a pixel a frame.
        let budget = ui.available_height() - 2.0;
        ui.multiply_opacity(solid);
        body(ui, &mut console, state, &mut typed, budget);
    });
    if let Some(shown) = shown {
        // While it is sliding the rect on screen is not where it lives, so
        // only a console standing still says where that is.
        if !moving {
            console.rect = Some(shown.response.rect);
        }
    }

    // Kept between sessions, written down the frame it is put away rather than
    // on every frame of a drag or of the slide out.
    let keep = console
        .rect
        .map(|rect| [rect.min.x, rect.min.y, rect.width(), rect.height()]);
    let put_away = flipped && !console.open;
    state.console = console;
    if put_away && keep != state.settings.console_rect {
        state.settings.console_rect = keep;
        crate::hm::storage::save(&state.settings);
    }
    if let Some(line) = typed {
        crate::hm::console::cmd::run(state, &line);
    }
    // While it is open the window is redrawn: lines arrive from threads that
    // have no way to ask for a frame.
    ctx.request_repaint_after(std::time::Duration::from_millis(150));
}

fn body(
    ui: &mut egui::Ui,
    console: &mut Console,
    state: &State,
    typed: &mut Option<String>,
    budget: f32,
) {
    head(ui, console);
    ui.add_space(2.0);
    widgets::hairline(ui);

    let counts = counts();
    chips(ui, console, &counts);
    widgets::hairline(ui);
    ui.add_space(2.0);

    // The prompt is drawn last but measured first, so the scrollback gets what
    // is left of the console rather than pushing the prompt off the bottom. The
    // room is worked out against the height the console was given: an area has
    // no bottom of its own, so asking what is available would answer with the
    // rest of the screen.
    let offers = crate::hm::console::cmd::offers(state, &console.input);
    let room = budget
        - ui.min_rect().height()
        - 30.0
        - match offers.is_empty() || console.input.is_empty() {
            true => 0.0,
            false => (offers.len().min(OFFERS) as f32) * 16.0 + 6.0,
        };

    scrollback(ui, console, room.max(80.0));
    ui.add_space(2.0);
    widgets::hairline(ui);
    prompt(ui, console, &offers, typed);
}

fn head(ui: &mut egui::Ui, console: &mut Console) {
    ui.horizontal(|ui| {
        ui.colored_label(theme::ACCENT, "console");
        ui.colored_label(
            theme::DIM,
            format!(
                "{} lines  \u{b7}  up {}",
                widgets::tally(console::read(|lines| lines.len())),
                widgets::seconds(console::uptime() as f32)
            ),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("close").clicked() {
                console.hide();
            }
            if ui
                .small_button("clear")
                .on_hover_text("empty what is on screen; the file on disk keeps it all")
                .clicked()
            {
                console::clear();
            }
            if ui
                .small_button("open log")
                .on_hover_text(console::path().to_string_lossy().to_ascii_lowercase())
                .clicked()
            {
                let _ = crate::hm::ui::queue::open_folder(&crate::hm::storage::config_dir());
            }
            ui.add_space(8.0);
            ui.colored_label(theme::DIM, "find");
            ui.add(
                egui::TextEdit::singleline(&mut console.find)
                    .desired_width(160.0)
                    .hint_text("anything"),
            );
        });
    });
}

/// The channel and level chips.
///
/// A channel with nothing in it is still drawn, so the row does not move about
/// as work starts and stops, and the count beside each one says whether there
/// is anything to see before it is turned on.
fn chips(ui: &mut egui::Ui, console: &mut Console, counts: &[usize]) {
    ui.horizontal_wrapped(|ui| {
        for channel in Channel::ALL {
            let at = slot(channel);
            let on = console.channels[at];
            let count = counts[at];
            let text = match count {
                0 => channel.label().to_string(),
                many => format!("{} {}", channel.label(), widgets::tally(many)),
            };
            let colour = match on {
                true => tint(channel.tint()),
                false => theme::DIM,
            };
            let hit = ui.add(
                egui::Button::new(egui::RichText::new(text).color(colour).size(10.0))
                    .fill(match on {
                        true => theme::HOVER,
                        false => theme::PANEL,
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        match on {
                            true => tint(channel.tint()).gamma_multiply(0.5),
                            false => theme::HAIRLINE,
                        },
                    )),
            );
            if hit
                .on_hover_text(format!("{}  ({})", channel.about(), channel.number()))
                .clicked()
            {
                console.channels[at] = !on;
            }
        }
        ui.add_space(10.0);
        for level in Level::ALL {
            let at = level_slot(level);
            let on = console.levels[at];
            if widgets::chip(ui, level.label(), on).clicked() {
                console.levels[at] = !on;
            }
        }
    });
}

fn counts() -> Vec<usize> {
    let mut counts = vec![0usize; Channel::ALL.len()];
    console::read(|lines| {
        for line in lines {
            counts[slot(line.channel)] += 1;
        }
    });
    counts
}

/// The log itself.
///
/// Only the rows on screen are copied out of the log: the scrollback is four
/// thousand lines and the window is forty of them.
fn scrollback(ui: &mut egui::Ui, console: &mut Console, height: f32) {
    // Which rows there are, as indexes into the log. Cheap: no strings move.
    let rows: Vec<Row> = console::read(|lines| {
        let mut rows = Vec::with_capacity(lines.len());
        for (at, line) in lines.iter().enumerate() {
            if !console.shows(line) {
                continue;
            }
            rows.push(Row::Head(at));
            if console.open_row(line) {
                for under in 0..line.under.len() {
                    rows.push(Row::Under(at, under));
                }
            }
        }
        rows
    });

    let mut turn: Option<u64> = None;
    egui::ScrollArea::vertical()
        .id_salt("console-rows")
        .auto_shrink([false, false])
        .max_height(height)
        .stick_to_bottom(true)
        .show_rows(ui, 15.0, rows.len(), |ui, range| {
            let shown: Vec<(Row, Option<Line>)> = console::read(|lines| {
                range
                    .clone()
                    .map(|at| {
                        let row = rows[at];
                        let which = match row {
                            Row::Head(index) | Row::Under(index, _) => index,
                        };
                        (row, lines.get(which).cloned())
                    })
                    .collect()
            });
            for (row, line) in shown {
                let Some(line) = line else { continue };
                match row {
                    Row::Head(_) => {
                        if head_row(ui, &line) {
                            turn = Some(line.seq);
                        }
                    }
                    Row::Under(_, at) => under_row(ui, &line, at),
                }
            }
        });

    if let Some(seq) = turn {
        let line = console::read(|lines| lines.iter().find(|line| line.seq == seq).cloned());
        if let Some(line) = line {
            console.fold(&line);
        }
    }
}

/// One line: clock, channel, level, sentence, note.
///
/// Drawn as one laid-out run of text rather than a row of labels, so the lanes
/// line up exactly and the whole row is one thing to click.
fn head_row(ui: &mut egui::Ui, line: &Line) -> bool {
    let font = egui::FontId::monospace(11.0);
    let mut job = egui::text::LayoutJob::default();
    let mut put = |text: &str, colour: egui::Color32| {
        job.append(
            text,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: colour,
                ..Default::default()
            },
        );
    };

    put(&line.clock, theme::DIM.gamma_multiply(0.8));
    put("  ", theme::DIM);
    // The bar is what makes a channel findable down a wall of text: the eye
    // follows the colour, not the word.
    put("\u{258d}", tint(line.channel.tint()));
    put(
        &format!("{:<8}", line.channel.label()),
        tint(line.channel.tint()),
    );
    put(line.level.tag(), tint(line.level.tint()));
    put(" ", theme::DIM);
    put(
        &line.text,
        match line.level {
            Level::Trace => theme::DIM,
            Level::Error => tint(Level::Error.tint()),
            Level::Typed => tint(Level::Typed.tint()),
            _ => theme::TEXT,
        },
    );
    if let Some(note) = &line.note {
        put(&format!("  ({note})"), theme::DIM);
    }
    if !line.under.is_empty() {
        put("  \u{2026}", theme::DIM);
    }

    let galley = ui.painter().layout_job(job);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 15.0),
        match line.under.is_empty() {
            true => egui::Sense::hover(),
            false => egui::Sense::click(),
        },
    );
    // A failure is worth seeing before it is read.
    if line.level == Level::Error {
        ui.painter()
            .rect_filled(rect, 0.0, tint(Level::Error.tint()).gamma_multiply(0.07));
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, theme::HOVER);
    }
    ui.painter()
        .galley(rect.min + egui::vec2(0.0, 1.0), galley, theme::TEXT);
    response.clicked()
}

/// A row of detail under a line.
fn under_row(ui: &mut egui::Ui, line: &Line, at: usize) {
    let Some(text) = line.under.get(at) else {
        return;
    };
    let last = at + 1 == line.under.len();
    let font = egui::FontId::monospace(11.0);
    let mut job = egui::text::LayoutJob::default();
    // Far enough in to sit under the sentence rather than under the tags.
    let indent = CLOCK_LANE + CHANNEL_LANE + LEVEL_LANE;
    job.append(
        match last {
            true => "\u{2514} ",
            false => "\u{2502} ",
        },
        indent,
        egui::TextFormat {
            font_id: font.clone(),
            color: tint(line.channel.tint()).gamma_multiply(0.5),
            ..Default::default()
        },
    );
    job.append(
        text,
        0.0,
        egui::TextFormat {
            font_id: font,
            color: match line.level {
                Level::Error => tint(Level::Error.tint()).gamma_multiply(0.85),
                Level::Warn => tint(Level::Warn.tint()).gamma_multiply(0.85),
                _ => theme::DIM,
            },
            ..Default::default()
        },
    );
    let galley = ui.painter().layout_job(job);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 15.0), egui::Sense::hover());
    ui.painter().galley(rect.min, galley, theme::DIM);
}

/// The line being typed, what could finish it, and what has been typed before.
fn prompt(
    ui: &mut egui::Ui,
    console: &mut Console,
    offers: &[crate::hm::console::cmd::Offer],
    typed: &mut Option<String>,
) {
    let showing: Vec<&crate::hm::console::cmd::Offer> =
        offers.iter().take(OFFERS).collect();
    let listing = !showing.is_empty() && !console.input.is_empty();
    if console.pick >= showing.len() {
        console.pick = 0;
    }

    if listing {
        ui.add_space(2.0);
        for (at, offer) in showing.iter().enumerate() {
            let on = at == console.pick;
            let mut job = egui::text::LayoutJob::default();
            let font = egui::FontId::monospace(11.0);
            job.append(
                &format!("{:<18}", offer.text),
                LANE * 2.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: match on {
                        true => theme::WAVE,
                        false => theme::TEXT,
                    },
                    ..Default::default()
                },
            );
            if !offer.about.is_empty() {
                job.append(
                    &offer.about,
                    0.0,
                    egui::TextFormat {
                        font_id: font,
                        color: theme::DIM,
                        ..Default::default()
                    },
                );
            }
            let galley = ui.painter().layout_job(job);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), egui::Sense::click());
            if on {
                ui.painter().rect_filled(rect, 0.0, theme::HOVER);
            }
            ui.painter().galley(rect.min, galley, theme::TEXT);
            if response.clicked() {
                console.pick = at;
                accept(console, &showing);
            }
        }
    }

    let keys = ui.input_mut(|input| Keys {
        tab: input.consume_key(egui::Modifiers::NONE, egui::Key::Tab),
        up: input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
        down: input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
        escape: input.key_pressed(egui::Key::Escape),
        wipe: input.consume_key(egui::Modifiers::CTRL, egui::Key::L),
    });

    ui.horizontal(|ui| {
        ui.colored_label(theme::ACCENT, ">");
        let line = ui.add(
            egui::TextEdit::singleline(&mut console.input)
                .desired_width(ui.available_width() - 4.0)
                .font(egui::FontId::monospace(12.0))
                .hint_text("type help"),
        );
        if console.focus {
            line.request_focus();
            console.focus = false;
        }
        // What tab would finish the word with, in front of the caret rather
        // than in a box somewhere else.
        if let Some(first) = showing.first()
            && !console.input.is_empty()
            && first.text.to_ascii_lowercase().starts_with(&console.input.to_ascii_lowercase())
            && first.text.len() > console.input.len()
        {
            let font = egui::FontId::monospace(12.0);
            let width = ui.painter()
                .layout_no_wrap(console.input.clone(), font.clone(), theme::DIM)
                .rect
                .width();
            ui.painter().text(
                line.rect.left_top() + egui::vec2(width + 2.0, 1.0),
                egui::Align2::LEFT_TOP,
                &first.text[console.input.len()..],
                font,
                theme::DIM.gamma_multiply(0.8),
            );
        }
        if line.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            let line = std::mem::take(&mut console.input);
            console.remember(&line);
            console.pick = 0;
            *typed = Some(line);
            console.focus = true;
        }
    });

    if keys.wipe {
        console::clear();
    }
    if keys.tab && listing {
        accept(console, &showing);
    }
    if keys.up {
        match listing {
            true => console.pick = console.pick.saturating_sub(1),
            false => console.earlier(),
        }
    }
    if keys.down {
        match listing {
            true => console.pick = (console.pick + 1).min(showing.len().saturating_sub(1)),
            false => console.later(),
        }
    }
    if keys.escape {
        match console.input.is_empty() {
            true => console.hide(),
            false => console.input.clear(),
        }
    }
}

struct Keys {
    tab: bool,
    up: bool,
    down: bool,
    escape: bool,
    wipe: bool,
}

/// Put the highlighted completion into the line.
///
/// Only the last word is replaced, so completing a value leaves the command in
/// front of it alone, and a completed command name gets a space after it
/// because something always follows it.
fn accept(console: &mut Console, showing: &[&crate::hm::console::cmd::Offer]) {
    let Some(offer) = showing.get(console.pick) else {
        return;
    };
    let whole = console.input.clone();
    let head = match whole.rfind(' ') {
        Some(at) => whole[..=at].to_string(),
        None => String::new(),
    };
    let finishing_name = head.is_empty();
    console.input = format!("{head}{}", offer.text);
    if finishing_name
        && crate::hm::console::cmd::find(&offer.text).is_some_and(|command| !command.shape.is_empty())
    {
        console.input.push(' ');
    }
    console.pick = 0;
    console.focus = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(channel: Channel, level: Level, text: &str) -> Line {
        Line {
            seq: 1,
            at: 0.0,
            clock: "00:00:00".into(),
            channel,
            level,
            text: text.into(),
            note: None,
            under: vec!["why".into()],
        }
    }

    #[test]
    fn debug_is_off_until_it_is_asked_for() {
        let console = Console::default();
        assert!(!console.shows(&line(Channel::Debug, Level::Info, "noise")));
        assert!(console.shows(&line(Channel::Export, Level::Info, "writing")));
    }

    #[test]
    fn finding_looks_at_the_detail_as_well_as_the_line() {
        let mut console = Console::default();
        console.find = "WHY".into();
        assert!(console.shows(&line(Channel::App, Level::Info, "something")));
        console.find = "nothing like it".into();
        assert!(!console.shows(&line(Channel::App, Level::Info, "something")));
    }

    #[test]
    fn what_went_wrong_shows_its_detail_without_being_asked() {
        let mut console = Console::default();
        let bad = line(Channel::Export, Level::Error, "could not write");
        let fine = line(Channel::Export, Level::Info, "wrote");
        assert!(console.open_row(&bad));
        assert!(!console.open_row(&fine));
        console.fold(&bad);
        assert!(!console.open_row(&bad));
        console.fold(&fine);
        assert!(console.open_row(&fine));
    }

    #[test]
    fn the_history_walks_back_and_hands_back_what_was_being_typed() {
        let mut console = Console::default();
        console.remember("queue");
        console.remember("stat");
        console.input = "half a line".into();
        console.earlier();
        assert_eq!(console.input, "stat");
        console.earlier();
        assert_eq!(console.input, "queue");
        console.later();
        assert_eq!(console.input, "stat");
        console.later();
        assert_eq!(console.input, "half a line");
    }
}
