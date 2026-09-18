pub mod browser;
pub mod detail;
pub mod dragout;
pub mod games;
pub mod player;
pub mod queue;
pub mod space;
pub mod splash;
pub mod theme;
pub mod widgets;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Grid,
    Compact,
    Detailed,
    Waveform,
    Tree,
    /// The tree, but sorted by mode, category and bucket before the path.
    TreePlus,
    Recent,
    Favorites,
}

pub const VIEWS: &[View] = &[
    View::Grid,
    View::Compact,
    View::Detailed,
    View::Waveform,
    View::Tree,
    View::TreePlus,
    View::Recent,
    View::Favorites,
];

impl View {
    /// The part of the label that is written in the accent colour, and the
    /// rest. `tree+` is the tree with more in it, and the plus says so.
    pub fn split(self) -> (&'static str, &'static str) {
        match self {
            View::TreePlus => ("tree", "+"),
            other => (other.label(), ""),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Grid => "grid",
            View::Compact => "compact",
            View::Detailed => "detailed",
            View::Waveform => "waveform",
            View::Tree => "tree",
            View::TreePlus => "tree+",
            View::Recent => "recent",
            View::Favorites => "favorites",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pane {
    Waveform,
    Spectrogram,
    Meta,
    Compare,
}

pub const PANES: &[Pane] = &[Pane::Waveform, Pane::Spectrogram, Pane::Meta, Pane::Compare];

impl Pane {
    pub fn label(self) -> &'static str {
        match self {
            Pane::Waveform => "waveform",
            Pane::Spectrogram => "spectrogram",
            Pane::Meta => "meta",
            Pane::Compare => "compare",
        }
    }
}
