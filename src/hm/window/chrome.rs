//! The window's own frame.
//!
//! Harmony draws its title bar rather than wearing the system one: the top row
//! carries the name, the icon and the buttons, and the rest of the strip is the
//! app. A window with no decorations has no resize border either, so the edges
//! are handled here too.

use eframe::egui::{
    Context, CursorIcon, Id, LayerId, Order, Pos2, Rect, ResizeDirection, ViewportCommand,
};

/// How far in from an edge counts as a grab for resizing.
pub const EDGE: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl Edge {
    pub fn cursor(self) -> CursorIcon {
        match self {
            Edge::North => CursorIcon::ResizeNorth,
            Edge::South => CursorIcon::ResizeSouth,
            Edge::East => CursorIcon::ResizeEast,
            Edge::West => CursorIcon::ResizeWest,
            Edge::NorthEast => CursorIcon::ResizeNorthEast,
            Edge::NorthWest => CursorIcon::ResizeNorthWest,
            Edge::SouthEast => CursorIcon::ResizeSouthEast,
            Edge::SouthWest => CursorIcon::ResizeSouthWest,
        }
    }

    pub fn direction(self) -> ResizeDirection {
        match self {
            Edge::North => ResizeDirection::North,
            Edge::South => ResizeDirection::South,
            Edge::East => ResizeDirection::East,
            Edge::West => ResizeDirection::West,
            Edge::NorthEast => ResizeDirection::NorthEast,
            Edge::NorthWest => ResizeDirection::NorthWest,
            Edge::SouthEast => ResizeDirection::SouthEast,
            Edge::SouthWest => ResizeDirection::SouthWest,
        }
    }
}

/// Which edge, if any, the pointer is close enough to to resize by.
pub fn edge_at(rect: Rect, pointer: Pos2, margin: f32) -> Option<Edge> {
    if rect.width() <= margin * 2.0 || rect.height() <= margin * 2.0 {
        return None;
    }
    if !rect.expand(1.0).contains(pointer) {
        return None;
    }
    let west = pointer.x - rect.left() <= margin;
    let east = rect.right() - pointer.x <= margin;
    let north = pointer.y - rect.top() <= margin;
    let south = rect.bottom() - pointer.y <= margin;
    match (north, south, east, west) {
        (true, _, true, _) => Some(Edge::NorthEast),
        (true, _, _, true) => Some(Edge::NorthWest),
        (_, true, true, _) => Some(Edge::SouthEast),
        (_, true, _, true) => Some(Edge::SouthWest),
        (true, _, _, _) => Some(Edge::North),
        (_, true, _, _) => Some(Edge::South),
        (_, _, true, _) => Some(Edge::East),
        (_, _, _, true) => Some(Edge::West),
        _ => None,
    }
}

/// Let the pointer resize the window from its edges, and show the cursor that
/// says so. Nothing happens while a drag is already under way, so a drag out of
/// the list that wanders into an edge does not turn into a resize.
pub fn edges(ctx: &Context, busy: bool) {
    if busy {
        return;
    }
    let (pointer, pressed, down) = ctx.input(|i| {
        (
            i.pointer.hover_pos(),
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
        )
    });
    if down && !pressed {
        return;
    }
    let Some(pointer) = pointer else {
        return;
    };
    let Some(edge) = edge_at(ctx.viewport_rect(), pointer, EDGE) else {
        return;
    };
    // The cursor is set on a foreground layer so the widgets under it do not
    // win the argument about what the pointer looks like.
    let _ = LayerId::new(Order::Foreground, Id::new("harmony_edge"));
    ctx.set_cursor_icon(edge.cursor());
    if pressed {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(edge.direction()));
    }
}

pub fn drag(ctx: &Context) {
    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
}

pub fn minimize(ctx: &Context) {
    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
}

pub fn close(ctx: &Context) {
    ctx.send_viewport_cmd(ViewportCommand::Close);
}

pub fn maximized(ctx: &Context) -> bool {
    ctx.input(|state| state.viewport().maximized.unwrap_or(false))
}

/// Fill the screen, or give it back.
pub fn toggle_maximize(ctx: &Context) {
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized(ctx)));
}

/// How big the window is, for opening it the same size next time. Where it is
/// is left to the shell: a remembered position outlives the monitor it was on.
pub fn size(ctx: &Context) -> Option<[f32; 2]> {
    ctx.input(|state| {
        state
            .viewport()
            .inner_rect
            .map(|rect| [rect.width(), rect.height()])
    })
}
