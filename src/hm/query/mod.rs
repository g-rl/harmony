pub mod eval;
pub mod parse;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Name,
    Path,
    Game,
    Category,
    Mode,
    Kind,
    Weapon,
    Character,
    Map,
    Format,
    Length,
    Rate,
    Channels,
    Package,
    Language,
    Tag,
    Named,
    Favorite,
}

impl Key {
    pub fn parse(text: &str) -> Option<Key> {
        Some(match text {
            "name" => Key::Name,
            "path" => Key::Path,
            "game" => Key::Game,
            "category" | "cat" => Key::Category,
            "mode" => Key::Mode,
            "type" | "kind" => Key::Kind,
            "weapon" | "wpn" => Key::Weapon,
            "character" | "char" => Key::Character,
            "map" => Key::Map,
            "format" | "codec" => Key::Format,
            "length" | "len" | "dur" => Key::Length,
            "rate" | "hz" => Key::Rate,
            "channels" | "ch" => Key::Channels,
            "package" | "pack" | "pkg" => Key::Package,
            "language" | "lang" => Key::Language,
            "tag" => Key::Tag,
            "named" => Key::Named,
            "favorite" | "favourite" | "fav" => Key::Favorite,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Key::Name => "name",
            Key::Path => "path",
            Key::Game => "game",
            Key::Category => "category",
            Key::Mode => "mode",
            Key::Kind => "type",
            Key::Weapon => "weapon",
            Key::Character => "character",
            Key::Map => "map",
            Key::Format => "format",
            Key::Length => "length",
            Key::Rate => "rate",
            Key::Channels => "channels",
            Key::Package => "package",
            Key::Language => "language",
            Key::Tag => "tag",
            Key::Named => "named",
            Key::Favorite => "favorite",
        }
    }
}

pub const KEYS: &[&str] = &[
    "name", "path", "game", "category", "mode", "type", "weapon", "character", "map", "format", "length",
    "rate", "channels", "package", "language", "tag", "named", "favorite",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Is,
    Less,
    More,
    AtMost,
    AtLeast,
}

#[derive(Clone, Debug)]
pub enum Term {
    Text(String),
    Field { key: Key, op: Op, value: String },
    Not(Box<Term>),
    Any(Vec<Term>),
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    pub terms: Vec<Term>,
    pub source: String,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }
}
