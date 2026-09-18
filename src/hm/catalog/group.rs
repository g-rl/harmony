use std::collections::BTreeMap;

use crate::hm::catalog::{Catalog, Entry, SoundId};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupBy {
    Category,
    Mode,
    Package,
    Language,
    Path,
    Codec,
}

impl GroupBy {
    pub fn label(self) -> &'static str {
        match self {
            GroupBy::Category => "category",
            GroupBy::Mode => "mode",
            GroupBy::Package => "package",
            GroupBy::Language => "language",
            GroupBy::Path => "path",
            GroupBy::Codec => "codec",
        }
    }
}

pub const ALL: &[GroupBy] = &[
    GroupBy::Category,
    GroupBy::Mode,
    GroupBy::Package,
    GroupBy::Language,
    GroupBy::Path,
    GroupBy::Codec,
];

#[derive(Clone, Debug, Default)]
pub struct Node {
    pub label: String,
    pub count: usize,
    pub children: BTreeMap<String, Node>,
    pub entries: Vec<SoundId>,
}

impl Node {
    pub fn insert(&mut self, path: &[String], id: SoundId) {
        self.count += 1;
        match path.split_first() {
            None => self.entries.push(id),
            Some((head, rest)) => {
                let child = self.children.entry(head.clone()).or_insert_with(|| Node {
                    label: head.clone(),
                    ..Node::default()
                });
                child.insert(rest, id);
            }
        }
    }
}

pub fn path_of(entry: &Entry) -> Vec<String> {
    let name = entry.display();
    let mut parts: Vec<String> = name
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect();
    if parts.len() > 1 {
        parts.pop();
    } else {
        parts.clear();
    }
    parts
}

/// The deeper tree: which part of the game, what kind of sound, which finer
/// bucket, then the folders the name itself carries.
///
/// Everything in it is already known about the sound — nothing new is guessed.
/// It is the same facts arranged so that a list of a hundred thousand names
/// can be walked down to a handful.
pub fn build_plus(catalog: &Catalog, indices: &[usize], packages: &[String]) -> Node {
    let mut root = Node {
        label: "all".into(),
        ..Node::default()
    };
    for index in indices {
        let entry = &catalog.entries[*index];
        let name = entry.display();
        let package = packages
            .get(entry.package.0 as usize)
            .map(|name| name.as_str())
            .unwrap_or("");
        let mut path = vec![
            crate::hm::catalog::mode::of(&name, package).label().to_string(),
            entry.category.label().to_string(),
        ];
        // The bucket was worked out when the name was set, so nothing is read
        // out of a hash here and nothing is re-matched per frame.
        if entry.sub != entry.category.label() {
            path.push(entry.sub.to_string());
        }
        path.extend(path_of(entry));
        root.insert(&path, entry.id);
    }
    root
}

pub fn build(catalog: &Catalog, indices: &[usize], by: GroupBy, packages: &[String]) -> Node {
    let mut root = Node {
        label: "all".into(),
        ..Node::default()
    };
    for index in indices {
        let entry = &catalog.entries[*index];
        let path = match by {
            GroupBy::Category => vec![entry.category.label().to_string()],
            GroupBy::Mode => {
                let package = packages
                    .get(entry.package.0 as usize)
                    .map(|name| name.as_str())
                    .unwrap_or("");
                vec![crate::hm::catalog::mode::of(&entry.display(), package).label().to_string()]
            }
            GroupBy::Package => vec![
                packages
                    .get(entry.package.0 as usize)
                    .cloned()
                    .unwrap_or_else(|| "unknown".into()),
            ],
            GroupBy::Language => vec![entry.language.clone().unwrap_or_else(|| "shared".into())],
            GroupBy::Codec => vec![entry.codec.label().to_string()],
            GroupBy::Path => {
                let parts = path_of(entry);
                if parts.is_empty() {
                    vec!["unnamed".to_string()]
                } else {
                    parts
                }
            }
        };
        root.insert(&path, entry.id);
    }
    root
}
