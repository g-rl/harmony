use crate::hm::query::{Key, Op, Query, Term};

pub fn parse(source: &str) -> Query {
    let mut terms = Vec::new();
    for raw in split(source) {
        if let Some(term) = term(&raw) {
            terms.push(term);
        }
    }
    Query {
        terms,
        source: source.to_string(),
    }
}

fn split(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in source.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn term(raw: &str) -> Option<Term> {
    if raw.is_empty() {
        return None;
    }
    if let Some(rest) = raw.strip_prefix('-') {
        return term(rest).map(|t| Term::Not(Box::new(t)));
    }
    if raw.contains('|') {
        let any: Vec<Term> = raw.split('|').filter_map(term).collect();
        return if any.is_empty() {
            None
        } else {
            Some(Term::Any(any))
        };
    }

    for (mark, op) in [
        (":<=", Op::AtMost),
        (":>=", Op::AtLeast),
        (":<", Op::Less),
        (":>", Op::More),
        ("<=", Op::AtMost),
        (">=", Op::AtLeast),
        ("<", Op::Less),
        (">", Op::More),
        (":", Op::Is),
    ] {
        if let Some((left, right)) = raw.split_once(mark)
            && let Some(key) = Key::parse(&left.to_ascii_lowercase())
            && !right.is_empty()
        {
            return Some(Term::Field {
                key,
                op,
                value: right.to_ascii_lowercase(),
            });
        }
    }

    Some(Term::Text(raw.to_ascii_lowercase()))
}
