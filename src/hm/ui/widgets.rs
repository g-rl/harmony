use egui::{Align2, Color32, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::hm::ui::theme;

pub fn chip(ui: &mut Ui, text: &str, on: bool) -> Response {
    let colour = if on { theme::WAVE } else { theme::DIM };
    let response = ui.add(
        egui::Button::new(egui::RichText::new(text).color(colour))
            .fill(if on { theme::PRESSED } else { theme::PANEL })
            .stroke(Stroke::new(1.0, if on { theme::WAVE } else { theme::HAIRLINE })),
    );
    response
}

/// A window button: a glyph in a square, no fill until it is under the pointer.
/// Small, because the title row is one line and these are the least of what is
/// on it.
pub fn tiny(ui: &mut Ui, glyph: &str) -> Response {
    let side = theme::INTERACT_HEIGHT;
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let hot = response.hovered();
    if hot {
        ui.painter().rect_filled(rect, theme::CORNER, theme::HOVER);
    }
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        egui::FontId::monospace(theme::UI_FONT_SIZE),
        if hot { theme::PLAYHEAD } else { theme::DIM },
    );
    response
}

/// One `name  value` line.
///
/// The label keeps a fixed column, narrowing with the panel rather than
/// pushing it wider, and the value takes what is left and truncates. A value
/// too long to show says the whole of itself on hover.
pub fn field(ui: &mut Ui, name: &str, value: &str) {
    ui.horizontal(|ui| {
        let label = (ui.available_width() * 0.34).clamp(48.0, 96.0);
        ui.add_sized(
            vec2(label, theme::ROW_HEIGHT),
            egui::Label::new(egui::RichText::new(name).color(theme::DIM)).selectable(false),
        );
        let response = ui.add(
            egui::Label::new(egui::RichText::new(value).color(theme::TEXT))
                .truncate()
                .selectable(false),
        );
        if response.on_hover_text(value).hovered() {
            // The hover is the whole value; nothing else to do.
        }
    });
}

pub fn hairline(ui: &mut Ui) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::HAIRLINE);
}

pub fn waveform(ui: &mut Ui, size: Vec2, bins: &[(f32, f32)], head: Option<f32>) -> Response {
    waveform_empty(ui, size, bins, head, "no audio")
}

/// A waveform that says something of the caller's choosing when it has no bins
/// to draw — `reading ...` while a sound is still being decoded, say.
///
/// The empty text belongs to the widget rather than being painted over it by
/// the caller: two centred strings in the same rectangle collide, which is
/// exactly what "no audio" and "reading ..." used to do.
pub fn waveform_empty(
    ui: &mut Ui,
    size: Vec2,
    bins: &[(f32, f32)],
    head: Option<f32>,
    empty: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme::CORNER, theme::BG);

    if bins.is_empty() {
        // Laid out to the rectangle's own width so a long name is cut with an
        // ellipsis instead of running out past both edges.
        let mut job = egui::text::LayoutJob::single_section(
            empty.to_string(),
            egui::TextFormat {
                font_id: theme::label_font(),
                color: theme::DIM,
                ..Default::default()
            },
        );
        job.wrap.max_width = (rect.width() - 16.0).max(32.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
        let galley = painter.layout_job(job);
        painter.galley(
            rect.center() - galley.size() * 0.5,
            galley,
            theme::DIM,
        );
        return response;
    }

    let middle = rect.center().y;
    let half = rect.height() * 0.48;
    let step = rect.width() / bins.len() as f32;
    for (i, (low, high)) in bins.iter().enumerate() {
        let x = rect.left() + i as f32 * step;
        let top = middle - high * half;
        let bottom = middle - low * half;
        painter.rect_filled(
            Rect::from_min_max(pos2(x, top.min(bottom)), pos2(x + step.max(1.0), bottom.max(top))),
            0.0,
            theme::WAVE,
        );
    }
    painter.line_segment(
        [pos2(rect.left(), middle), pos2(rect.right(), middle)],
        Stroke::new(1.0, theme::GRID),
    );

    if let Some(at) = head {
        let x = rect.left() + rect.width() * at.clamp(0.0, 1.0);
        painter.line_segment(
            [pos2(x, rect.top()), pos2(x, rect.bottom())],
            Stroke::new(1.0, theme::PLAYHEAD),
        );
    }
    response
}

pub fn spectrogram(ui: &mut Ui, size: Vec2, columns: &[Vec<f32>]) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme::CORNER, theme::BG);
    if columns.is_empty() {
        return;
    }
    let step_x = rect.width() / columns.len() as f32;
    for (x, column) in columns.iter().enumerate() {
        if column.is_empty() {
            continue;
        }
        let step_y = rect.height() / column.len() as f32;
        for (y, value) in column.iter().enumerate() {
            let shade = value.clamp(0.0, 1.0);
            if shade < 0.02 {
                continue;
            }
            let colour = Color32::from_rgb(
                (theme::WAVE.r() as f32 * shade) as u8,
                (theme::WAVE.g() as f32 * shade) as u8,
                (theme::WAVE.b() as f32 * shade).max(40.0 * shade) as u8,
            );
            let left = rect.left() + x as f32 * step_x;
            let bottom = rect.bottom() - y as f32 * step_y;
            painter.rect_filled(
                Rect::from_min_max(
                    pos2(left, bottom - step_y),
                    pos2(left + step_x.max(1.0), bottom),
                ),
                0.0,
                colour,
            );
        }
    }
}

pub fn meter(ui: &mut Ui, width: f32, value: f32, colour: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme::CORNER, theme::BG);
    let filled = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * value.clamp(0.0, 1.0), rect.height()),
    );
    painter.rect_filled(filled, theme::CORNER, colour);
}

pub fn seconds(value: f32) -> String {
    let whole = value.max(0.0);
    let minutes = (whole / 60.0) as u32;
    let rest = whole - (minutes as f32 * 60.0);
    format!("{minutes}:{rest:05.2}")
}

pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["b", "k", "m", "g", "t"];
    let mut size = value as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value}b")
    } else {
        format!("{size:.1}{}", UNITS[unit])
    }
}

/// A number with its thousands marked: `32,661`.
pub fn tally(value: usize) -> String {
    let text = value.to_string();
    let mut out = String::with_capacity(text.len() + text.len() / 3);
    for (i, c) in text.chars().enumerate() {
        if i > 0 && (text.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A field whose value carries a note after it, in its own colour, still in
/// the value column.
pub fn field_note(ui: &mut Ui, name: &str, value: &str, note: &str, colour: egui::Color32) {
    ui.horizontal(|ui| {
        let label = (ui.available_width() * 0.34).clamp(48.0, 96.0);
        ui.add_sized(
            vec2(label, theme::ROW_HEIGHT),
            egui::Label::new(egui::RichText::new(name).color(theme::DIM)).selectable(false),
        );
        ui.add(
            egui::Label::new(egui::RichText::new(value).color(theme::TEXT))
                .truncate()
                .selectable(false),
        );
        if !note.is_empty() {
            ui.add(
                egui::Label::new(egui::RichText::new(note).color(colour))
                    .truncate()
                    .selectable(false),
            );
        }
    });
}

/// The star beside a sound somebody starred.
///
/// It breathes rather than sits: a favourite is something the eye should find
/// while scrolling a list of a hundred thousand names, and a shape that moves
/// a little is found without being loud about it. The phase comes from the row
/// itself, so a screenful of them is a field of slow lights rather than one
/// blinking thing repeated.
///
/// Drawn as a pentagon with five triangles on it. A five-pointed star is not
/// convex, and egui fills convex shapes: cut this way every piece is.
pub fn star(ui: &mut Ui, side: f32, colour: Color32, phase: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    let time = ui.input(|input| input.time) as f32;
    // Slow enough to read as breathing rather than as flashing.
    let beat = ((time * 1.8 + phase).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let spin = (time * 0.35 + phase * 0.3).sin() * 0.12;
    let outer = side * 0.5 * (0.78 + 0.12 * beat);
    let inner = outer * 0.46;
    let middle = rect.center();
    let lit = colour.gamma_multiply(0.72 + 0.28 * beat);

    let point = |turn: f32, radius: f32| {
        let angle = std::f32::consts::TAU * turn - std::f32::consts::FRAC_PI_2 + spin;
        pos2(
            middle.x + radius * angle.cos(),
            middle.y + radius * angle.sin(),
        )
    };
    let tips: Vec<egui::Pos2> = (0..5).map(|i| point(i as f32 / 5.0, outer)).collect();
    let pits: Vec<egui::Pos2> = (0..5)
        .map(|i| point((i as f32 + 0.5) / 5.0, inner))
        .collect();

    let painter = ui.painter_at(rect);
    painter.add(egui::Shape::convex_polygon(
        pits.clone(),
        lit,
        Stroke::NONE,
    ));
    for (at, tip) in tips.iter().enumerate() {
        let left = pits[(at + 4) % 5];
        let right = pits[at];
        painter.add(egui::Shape::convex_polygon(
            vec![left, *tip, right],
            lit,
            Stroke::NONE,
        ));
    }
    // Something is moving, so the window has to be asked for the next frame:
    // nothing else here would.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));
    response
}
