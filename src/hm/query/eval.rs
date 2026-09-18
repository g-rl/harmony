use crate::hm::catalog::Entry;
use crate::hm::query::{Key, Op, Query, Term};

pub struct Context<'a> {
    pub game: &'a str,
    pub packages: &'a [String],
}

pub fn matches(entry: &Entry, query: &Query, context: &Context<'_>) -> bool {
    query
        .terms
        .iter()
        .all(|term| term_matches(entry, term, context))
}

fn term_matches(entry: &Entry, term: &Term, context: &Context<'_>) -> bool {
    match term {
        Term::Text(text) => entry.text().contains(text.as_str()),
        Term::Not(inner) => !term_matches(entry, inner, context),
        Term::Any(any) => any.iter().any(|t| term_matches(entry, t, context)),
        Term::Field { key, op, value } => field(entry, *key, *op, value, context),
    }
}

fn number(value: &str) -> Option<f32> {
    value
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .ok()
}

fn compare(left: f32, op: Op, right: f32) -> bool {
    match op {
        Op::Is => (left - right).abs() < f32::EPSILON.max(right * 0.001),
        Op::Less => left < right,
        Op::More => left > right,
        Op::AtMost => left <= right,
        Op::AtLeast => left >= right,
    }
}

fn text(left: &str, op: Op, right: &str) -> bool {
    match op {
        Op::Is => left.contains(right),
        _ => left.contains(right),
    }
}

fn field(entry: &Entry, key: Key, op: Op, value: &str, context: &Context<'_>) -> bool {
    let name = entry.text();
    match key {
        Key::Name => text(name, op, value),
        Key::Path => text(name, op, value),
        Key::Game => text(context.game, op, value),
        Key::Category => text(entry.category.label(), op, value),
        Key::Mode => {
            let package = context
                .packages
                .get(entry.package.0 as usize)
                .map(|name| name.as_str())
                .unwrap_or("");
            text(crate::hm::catalog::mode::of(name, package).label(), op, value)
        }
        Key::Kind => entry
            .facets
            .kind
            .as_deref()
            .map(|k| text(k, op, value))
            .unwrap_or_else(|| text(name, op, value)),
        Key::Weapon => entry
            .facets
            .weapon
            .as_deref()
            .map(|w| text(w, op, value))
            .unwrap_or(false),
        Key::Character => entry
            .facets
            .character
            .as_deref()
            .map(|c| text(c, op, value))
            .unwrap_or(false),
        Key::Map => entry
            .facets
            .map
            .as_deref()
            .map(|m| text(m, op, value))
            .unwrap_or(false),
        Key::Format => text(entry.codec.label(), op, value),
        Key::Length => number(value)
            .map(|want| compare(entry.seconds(), op, want))
            .unwrap_or(false),
        Key::Rate => number(value)
            .map(|want| compare(entry.rate as f32, op, want))
            .unwrap_or(false),
        Key::Channels => {
            let want = match value {
                "mono" => Some(1.0),
                "stereo" => Some(2.0),
                other => number(other),
            };
            want.map(|w| compare(entry.channels as f32, op, w))
                .unwrap_or(false)
        }
        Key::Package => context
            .packages
            .get(entry.package.0 as usize)
            .map(|p| text(&p.to_ascii_lowercase(), op, value))
            .unwrap_or(false),
        Key::Language => entry
            .language
            .as_deref()
            .map(|l| text(l, op, value))
            .unwrap_or_else(|| value == "shared"),
        Key::Tag => entry.tags.iter().any(|t| text(t, op, value)),
        Key::Named => {
            let want = matches!(value, "yes" | "true" | "1");
            entry.name.resolved() == want
        }
        Key::Favorite => {
            let want = !matches!(value, "no" | "false" | "0");
            entry.favorite == want
        }
    }
}
