//! What can be typed at the console, and what it does.
//!
//! A command is a name, a shape, a sentence about itself and a function. The
//! window knows none of them by name: it takes a line, hands it here, and puts
//! whatever comes back on the log. Completion works the same way — the console
//! asks what could follow what has been typed so far, and every answer comes
//! from the same table the commands themselves are in, so nothing can be
//! suggested that cannot then be run.
//!
//! Names are dotted rather than spaced — `export.library`, `lane.split` — so
//! that a family of commands reads as one thing and tab completion on the stem
//! gets somewhere useful.

use crate::hm::app::State;
use crate::hm::console::{self, Channel, Level};
use crate::hm::export::{FORMATS, layout};
use crate::hm::game::TitleId;

/// What a command did.
pub enum Said {
    /// Said and done: one line back.
    Ok(String),
    /// Several lines back, as one folded entry.
    Deep(String, Vec<String>),
    /// It could not be done, and why.
    No(String),
    /// Nothing worth saying; the command has already said it.
    Quiet,
}

pub struct Command {
    pub name: &'static str,
    /// What follows the name, for the help and the prompt's own hint.
    pub shape: &'static str,
    pub about: &'static str,
    pub run: fn(&mut State, &[String]) -> Said,
}

/// Everything that can be typed, in the order the help lists it.
pub const ALL: &[Command] = &[
    Command {
        name: "help",
        shape: "[command]",
        about: "what there is, or what one of them does",
        run: help,
    },
    Command {
        name: "clear",
        shape: "",
        about: "empty the scrollback (the file on disk keeps it)",
        run: clear,
    },
    Command {
        name: "stat",
        shape: "",
        about: "what harmony is holding: sounds, names, lanes, memory",
        run: stat,
    },
    Command {
        name: "paths",
        shape: "",
        about: "every folder harmony reads or writes",
        run: paths,
    },
    Command {
        name: "open",
        shape: "<output|cache|scratch|settings|logs|crashes|resume>",
        about: "open one of those folders in explorer",
        run: open,
    },
    Command {
        name: "game.list",
        shape: "",
        about: "the titles harmony knows and where it last saw them",
        run: game_list,
    },
    Command {
        name: "game.open",
        shape: "<id>",
        about: "switch to a title by its id: t7, jup, iw8",
        run: game_open,
    },
    Command {
        name: "search",
        shape: "<text>",
        about: "type into the search box from here",
        run: search,
    },
    Command {
        name: "scan.start",
        shape: "",
        about: "scan the game that is open",
        run: scan_start,
    },
    Command {
        name: "scan.stop",
        shape: "",
        about: "stop every scan that is running",
        run: scan_stop,
    },
    Command {
        name: "scan.depth",
        shape: "<quick|deep|full>",
        about: "how deep the next scan goes",
        run: scan_depth,
    },
    Command {
        name: "export.library",
        shape: "",
        about: "extract the whole library of the game that is open",
        run: export_library,
    },
    Command {
        name: "export.shown",
        shape: "",
        about: "extract everything the list is showing",
        run: export_shown,
    },
    Command {
        name: "export.picked",
        shape: "",
        about: "extract what is selected",
        run: export_picked,
    },
    Command {
        name: "export.split",
        shape: "[on|off]",
        about: "send the next export to a lane of its own",
        run: export_split,
    },
    Command {
        name: "export.pause",
        shape: "[lane]",
        about: "pause a lane, or every lane",
        run: export_pause,
    },
    Command {
        name: "export.resume",
        shape: "[lane]",
        about: "let a paused lane carry on",
        run: export_resume,
    },
    Command {
        name: "export.cancel",
        shape: "[lane|all]",
        about: "stop what a lane is doing; what it wrote stays",
        run: export_cancel,
    },
    Command {
        name: "export.format",
        shape: "<wav|flac|ogg|raw>",
        about: "what the next export writes",
        run: export_format,
    },
    Command {
        name: "export.tree",
        shape: "<file only|sound path|package|category/package|language/category>",
        about: "how deep a folder tree the next export builds",
        run: export_tree,
    },
    Command {
        name: "queue",
        shape: "",
        about: "every lane, what it is writing and what is behind it",
        run: queue,
    },
    Command {
        name: "lane.split",
        shape: "",
        about: "open another queue window with its own line",
        run: lane_split,
    },
    Command {
        name: "lane.close",
        shape: "<n>",
        about: "close an idle lane",
        run: lane_close,
    },
    Command {
        name: "resume.list",
        shape: "",
        about: "exports that were put down and can be picked up",
        run: resume_list,
    },
    Command {
        name: "resume.start",
        shape: "<n>",
        about: "pick one of them up where it left off",
        run: resume_start,
    },
    Command {
        name: "resume.forget",
        shape: "<n>",
        about: "throw one of those notes away",
        run: resume_forget,
    },
    Command {
        name: "skip.list",
        shape: "",
        about: "the buckets a library extract is leaving out",
        run: skip_list,
    },
    Command {
        name: "skip.add",
        shape: "<bucket>",
        about: "leave a bucket out of the next library extract",
        run: skip_add,
    },
    Command {
        name: "skip.clear",
        shape: "",
        about: "put every bucket back in",
        run: skip_clear,
    },
    Command {
        name: "crash.dump",
        shape: "[why]",
        about: "write a report of what harmony is doing right now",
        run: crash_dump,
    },
    Command {
        name: "crash.list",
        shape: "",
        about: "the crash reports harmony has kept",
        run: crash_list,
    },
    Command {
        name: "play",
        shape: "",
        about: "play what the cursor is on",
        run: play,
    },
    Command {
        name: "stop",
        shape: "",
        about: "stop the player",
        run: stop,
    },
    Command {
        name: "discord",
        shape: "[on|off]",
        about: "rich presence",
        run: discord,
    },
    Command {
        name: "quit",
        shape: "",
        about: "close harmony, asking first if anything is running",
        run: quit,
    },
];

pub fn find(name: &str) -> Option<&'static Command> {
    ALL.iter().find(|command| command.name == name)
}

/// Take a line the user typed and do what it says.
///
/// The line goes on the log first, exactly as typed, so the scrollback reads
/// as a conversation rather than as a list of answers to questions nobody
/// wrote down.
pub fn run(state: &mut State, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    console::say(Channel::App, Level::Typed, line.to_string());
    let mut words = split(line);
    let name = words.remove(0);
    let Some(command) = find(&name) else {
        let near = nearest(&name);
        match near {
            Some(other) => console::say(
                Channel::App,
                Level::Error,
                format!("no command called {name} - did you mean {other}?"),
            ),
            None => console::say(
                Channel::App,
                Level::Error,
                format!("no command called {name}; type help"),
            ),
        };
        return;
    };
    match (command.run)(state, &words) {
        Said::Ok(text) => {
            console::say(Channel::App, Level::Reply, text);
        }
        Said::Deep(text, under) => {
            console::deep(Channel::App, Level::Reply, text, under);
        }
        Said::No(text) => {
            console::say(Channel::App, Level::Error, text);
        }
        Said::Quiet => {}
    }
}

/// A line into words, with quotes holding a name with spaces in it together.
pub fn split(line: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for glyph in line.chars() {
        match glyph {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            other => word.push(other),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    if words.is_empty() {
        words.push(String::new());
    }
    words
}

/// The closest command to something that was not one, when it is close enough
/// to be worth suggesting.
fn nearest(typed: &str) -> Option<&'static str> {
    ALL.iter()
        .map(|command| (command.name, apart(typed, command.name)))
        .filter(|(_, far)| *far <= 3)
        .min_by_key(|(_, far)| *far)
        .map(|(name, _)| name)
}

/// How many single-letter changes turn one word into another.
fn apart(one: &str, two: &str) -> usize {
    let one: Vec<char> = one.chars().collect();
    let two: Vec<char> = two.chars().collect();
    let mut row: Vec<usize> = (0..=two.len()).collect();
    for (i, left) in one.iter().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, right) in two.iter().enumerate() {
            let next = row[j + 1];
            row[j + 1] = match left == right {
                true => last,
                false => 1 + last.min(row[j]).min(row[j + 1]),
            };
            last = next;
        }
    }
    row[two.len()]
}

/// What could follow what has been typed so far.
///
/// The console asks for this on every keystroke, so it stays cheap: names are
/// a table walk, and the values that need the app — games, lanes, notes — are
/// short lists that are already in hand.
pub fn offers(state: &State, line: &str) -> Vec<Offer> {
    let ends_open = line.ends_with(' ');
    let words = split(line);
    if words.len() == 1 && !ends_open {
        return name_offers(&words[0]);
    }
    let Some(command) = find(&words[0]) else {
        return Vec::new();
    };
    let typed = match ends_open {
        true => String::new(),
        false => words.last().cloned().unwrap_or_default(),
    };
    values(state, command.name)
        .into_iter()
        .filter(|offer| matches(&typed, &offer.text))
        .collect()
}

/// One thing the prompt could be completed with.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub text: String,
    pub about: String,
}

fn name_offers(typed: &str) -> Vec<Offer> {
    let mut found: Vec<(usize, Offer)> = ALL
        .iter()
        .filter_map(|command| {
            rank(typed, command.name).map(|score| {
                (
                    score,
                    Offer {
                        text: command.name.to_string(),
                        about: command.about.to_string(),
                    },
                )
            })
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.text.cmp(&b.1.text)));
    found.into_iter().map(|(_, offer)| offer).collect()
}

/// Does this word match what has been typed? A prefix first, then the letters
/// in order anywhere in it, so `elib` finds `export.library`.
pub fn matches(typed: &str, against: &str) -> bool {
    rank(typed, against).is_some()
}

/// How good a match it is: lower is better. A prefix beats a run of letters
/// found scattered through the name.
fn rank(typed: &str, against: &str) -> Option<usize> {
    if typed.is_empty() {
        return Some(0);
    }
    let typed = typed.to_ascii_lowercase();
    let against_low = against.to_ascii_lowercase();
    if against_low.starts_with(&typed) {
        return Some(0);
    }
    // The stem after a dot counts as a start of its own: `library` should
    // find `export.library`.
    if against_low
        .split('.')
        .any(|part| part.starts_with(&typed))
    {
        return Some(1);
    }
    let mut hunting = typed.chars();
    let mut want = hunting.next();
    let mut gaps = 0usize;
    for glyph in against_low.chars() {
        match want {
            Some(letter) if letter == glyph => want = hunting.next(),
            Some(_) => gaps += 1,
            None => break,
        }
    }
    match want.is_none() {
        true => Some(2 + gaps),
        false => None,
    }
}

/// What the values after a given command could be.
fn values(state: &State, name: &str) -> Vec<Offer> {
    match name {
        "help" => ALL
            .iter()
            .map(|command| Offer {
                text: command.name.to_string(),
                about: command.about.to_string(),
            })
            .collect(),
        "open" => ["output", "cache", "scratch", "settings", "logs", "crashes", "resume"]
            .iter()
            .map(|what| Offer {
                text: (*what).to_string(),
                about: String::new(),
            })
            .collect(),
        "game.open" => crate::hm::ui::games::KNOWN
            .iter()
            .map(|id| Offer {
                text: id.key().to_string(),
                about: format!(
                    "{}{}",
                    id.abbr(),
                    match state.settings.roots.contains_key(id.key()) {
                        true => " - harmony knows where it is",
                        false => "",
                    }
                ),
            })
            .collect(),
        "scan.depth" => crate::hm::scan::DEPTHS
            .iter()
            .map(|depth| Offer {
                text: depth.label().to_string(),
                about: depth.note().to_string(),
            })
            .collect(),
        "export.format" => FORMATS
            .iter()
            .map(|format| Offer {
                text: format.label().to_string(),
                about: format.about().to_string(),
            })
            .collect(),
        "export.tree" => layout::ALL
            .iter()
            .map(|tree| Offer {
                text: format!("\"{}\"", tree.label()),
                about: tree.about().to_string(),
            })
            .collect(),
        "export.split" | "discord" => ["on", "off"]
            .iter()
            .map(|word| Offer {
                text: (*word).to_string(),
                about: String::new(),
            })
            .collect(),
        "export.pause" | "export.resume" | "export.cancel" | "lane.close" => state
            .queue
            .list()
            .iter()
            .map(|lane| Offer {
                text: lane.id.to_string(),
                about: lane.line(),
            })
            .chain(match name == "export.cancel" {
                true => Some(Offer {
                    text: "all".into(),
                    about: "every lane".into(),
                }),
                false => None,
            })
            .collect(),
        "resume.start" | "resume.forget" => state
            .resumes
            .iter()
            .enumerate()
            .map(|(at, note)| Offer {
                text: (at + 1).to_string(),
                about: format!("{} - {} of {} written", note.folder, note.done, note.total),
            })
            .collect(),
        "skip.add" => state
            .buckets
            .iter()
            .map(|(name, count)| Offer {
                text: (*name).to_string(),
                about: format!("{count} sounds"),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn help(_state: &mut State, words: &[String]) -> Said {
    if let Some(name) = words.first().filter(|word| !word.is_empty()) {
        let Some(command) = find(name) else {
            return Said::No(format!("no command called {name}"));
        };
        return Said::Deep(
            format!("{} {}", command.name, command.shape),
            vec![command.about.to_string()],
        );
    }
    Said::Deep(
        format!("{} commands; tab completes, up walks back", ALL.len()),
        ALL.iter()
            .map(|command| {
                format!(
                    "{:<16}{:<46}{}",
                    command.name, command.shape, command.about
                )
            })
            .collect(),
    )
}

fn clear(_state: &mut State, _words: &[String]) -> Said {
    console::clear();
    Said::Quiet
}

fn stat(state: &mut State, _words: &[String]) -> Said {
    let (done, failed, total, queued) = state.queue.totals();
    let mut under = vec![
        format!(
            "game     {}",
            state
                .title
                .map(crate::hm::app::title_label)
                .unwrap_or("none open")
        ),
        format!(
            "sounds   {} in the catalogue, {} shown, {} picked",
            state.catalog.len(),
            state.filtered.len(),
            state.selection.len()
        ),
        format!("names    {} matched", state.names.len()),
        format!(
            "lanes    {} open, {} threads each",
            state.queue.count(),
            crate::hm::export::queue::Queue::workers()
        ),
        format!("exports  {done} written, {failed} failed of {total}, {queued} waiting"),
        format!("up for   {:.0} seconds", console::uptime()),
        format!("console  {} lines kept, {} dropped", console::read(|lines| lines.len()), console::dropped()),
    ];
    if state.scanning() {
        under.push("scanning right now".into());
    }
    Said::Deep("what harmony is holding".into(), under)
}

fn paths(state: &mut State, _words: &[String]) -> Said {
    let say = |path: std::path::PathBuf| path.to_string_lossy().to_ascii_lowercase();
    Said::Deep(
        "where everything is".into(),
        vec![
            format!("settings {}", say(crate::hm::storage::config_dir())),
            format!("caches   {}", say(crate::hm::storage::cache_dir())),
            format!("scratch  {}", say(crate::hm::storage::temp_dir())),
            format!(
                "output   {}",
                state
                    .output
                    .clone()
                    .map(say)
                    .unwrap_or_else(|| "not chosen".into())
            ),
            format!("console  {}", say(console::path())),
            format!("crashes  {}", crate::hm::crash::where_they_are()),
            format!("resume   {}", say(crate::hm::storage::resume_dir())),
        ],
    )
}

fn open(state: &mut State, words: &[String]) -> Said {
    let what = words.first().map(String::as_str).unwrap_or("output");
    let path = match what {
        "output" => match state.output.clone() {
            Some(path) => path,
            None => return Said::No("no output folder chosen yet".into()),
        },
        "cache" => crate::hm::storage::cache_dir(),
        "scratch" => crate::hm::storage::temp_dir(),
        "settings" => crate::hm::storage::config_dir(),
        "logs" => crate::hm::storage::config_dir(),
        "crashes" => crate::hm::crash::folder(),
        "resume" => crate::hm::storage::resume_dir(),
        other => return Said::No(format!("nothing called {other} to open")),
    };
    let _ = std::fs::create_dir_all(&path);
    match crate::hm::ui::queue::open_folder(&path) {
        Ok(()) => Said::Ok(path.to_string_lossy().to_ascii_lowercase()),
        Err(error) => Said::No(error.to_string()),
    }
}

fn game_list(state: &mut State, _words: &[String]) -> Said {
    Said::Deep(
        "the titles harmony knows".into(),
        crate::hm::ui::games::KNOWN
            .iter()
            .map(|id| {
                format!(
                    "{:<5}{:<8}{}",
                    id.key(),
                    id.abbr(),
                    state
                        .settings
                        .roots
                        .get(id.key())
                        .map(|path| path.to_string_lossy().to_ascii_lowercase())
                        .unwrap_or_else(|| "not pointed anywhere".into())
                )
            })
            .collect(),
    )
}

fn game_open(state: &mut State, words: &[String]) -> Said {
    let Some(key) = words.first().filter(|word| !word.is_empty()) else {
        return Said::No("which game? try game.list".into());
    };
    let Some(id) = crate::hm::ui::games::KNOWN
        .iter()
        .copied()
        .find(|id: &TitleId| id.key() == key.as_str())
    else {
        return Said::No(format!("no title with the id {key}"));
    };
    state.switch(id);
    Said::Ok(format!("opened {}", crate::hm::app::title_label(id)))
}

fn search(state: &mut State, words: &[String]) -> Said {
    let text = words.join(" ");
    state.query_text = text.clone();
    state.dirty = true;
    Said::Ok(match text.is_empty() {
        true => "search cleared".into(),
        false => format!("searching for {text}"),
    })
}

fn scan_start(state: &mut State, _words: &[String]) -> Said {
    if state.title.is_none() {
        return Said::No("no game is open".into());
    }
    state.start_scan();
    Said::Quiet
}

fn scan_stop(state: &mut State, _words: &[String]) -> Said {
    state.stop_all();
    Said::Ok("every scan asked to stop".into())
}

fn scan_depth(state: &mut State, words: &[String]) -> Said {
    let Some(word) = words.first() else {
        return Said::Ok(format!("scans go {} deep", state.depth.label()));
    };
    let Some(depth) = crate::hm::scan::DEPTHS
        .iter()
        .copied()
        .find(|depth| depth.label() == word.as_str())
    else {
        return Said::No(format!("no depth called {word}"));
    };
    state.depth = depth;
    Said::Ok(format!("the next scan goes {} deep", depth.label()))
}

fn export_library(state: &mut State, _words: &[String]) -> Said {
    if state.title.is_none() {
        return Said::No("no game is open".into());
    }
    state.extract_library();
    Said::Quiet
}

fn export_shown(state: &mut State, _words: &[String]) -> Said {
    state.extract(true);
    Said::Quiet
}

fn export_picked(state: &mut State, _words: &[String]) -> Said {
    if state.selection.is_empty() {
        return Said::No("nothing is picked".into());
    }
    state.extract(false);
    Said::Quiet
}

fn export_split(state: &mut State, words: &[String]) -> Said {
    state.split_next = match words.first().map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => !state.split_next,
    };
    Said::Ok(match state.split_next {
        true => "the next export opens a lane of its own".into(),
        false => "the next export joins the line in lane 1".into(),
    })
}

fn lane_of(state: &State, words: &[String]) -> Option<usize> {
    words.first().and_then(|word| word.parse::<usize>().ok()).or(Some(state.lane_focus))
}

fn export_pause(state: &mut State, words: &[String]) -> Said {
    let Some(id) = lane_of(state, words) else {
        return Said::No("which lane?".into());
    };
    let Some(lane) = state.queue.find(id) else {
        return Said::No(format!("there is no lane {id}"));
    };
    lane.queue
        .paused
        .store(true, std::sync::atomic::Ordering::Relaxed);
    Said::Ok(format!("lane {id} paused"))
}

fn export_resume(state: &mut State, words: &[String]) -> Said {
    let Some(id) = lane_of(state, words) else {
        return Said::No("which lane?".into());
    };
    let Some(lane) = state.queue.find(id) else {
        return Said::No(format!("there is no lane {id}"));
    };
    lane.queue
        .paused
        .store(false, std::sync::atomic::Ordering::Relaxed);
    Said::Ok(format!("lane {id} carrying on"))
}

fn export_cancel(state: &mut State, words: &[String]) -> Said {
    if words.first().map(String::as_str) == Some("all") {
        state.queue.stop_all();
        return Said::Ok("every lane stopped; what was written stays".into());
    }
    let Some(id) = lane_of(state, words) else {
        return Said::No("which lane?".into());
    };
    let Some(lane) = state.queue.find(id) else {
        return Said::No(format!("there is no lane {id}"));
    };
    lane.queue.stop();
    Said::Ok(format!(
        "lane {id} stopped; it is written down and can be picked up"
    ))
}

fn export_format(state: &mut State, words: &[String]) -> Said {
    let Some(word) = words.first() else {
        return Said::Ok(format!("exports write {}", state.options.format.label()));
    };
    let Some(format) = FORMATS
        .iter()
        .copied()
        .find(|format| format.label() == word.as_str())
    else {
        return Said::No(format!("no format called {word}"));
    };
    state.options.format = format;
    state.save_options();
    Said::Ok(format!("exports write {} now", format.label()))
}

fn export_tree(state: &mut State, words: &[String]) -> Said {
    let wanted = words.join(" ");
    if wanted.is_empty() {
        return Said::Ok(format!("exports build {}", state.options.layout.label()));
    }
    let Some(tree) = layout::ALL
        .iter()
        .copied()
        .find(|tree| tree.label() == wanted.as_str())
    else {
        return Said::No(format!("no tree called {wanted}"));
    };
    state.options.layout = tree;
    state.save_options();
    Said::Ok(format!("exports build {} now", tree.label()))
}

fn queue(state: &mut State, _words: &[String]) -> Said {
    let lines: Vec<String> = state
        .queue
        .list()
        .iter()
        .map(|lane| lane.line())
        .collect();
    Said::Deep(format!("{} lanes", lines.len()), lines)
}

fn lane_split(state: &mut State, _words: &[String]) -> Said {
    let id = state.queue.split();
    state.lane_focus = id;
    Said::Ok(format!("lane {id} opened"))
}

fn lane_close(state: &mut State, words: &[String]) -> Said {
    let Some(id) = words.first().and_then(|word| word.parse::<usize>().ok()) else {
        return Said::No("which lane?".into());
    };
    match state.queue.close(id) {
        true => Said::Ok(format!("lane {id} closed")),
        false => Said::No(format!(
            "lane {id} is either busy or lane 1, which never closes"
        )),
    }
}

fn resume_list(state: &mut State, _words: &[String]) -> Said {
    if state.resumes.is_empty() {
        return Said::Ok("nothing was put down".into());
    }
    Said::Deep(
        format!("{} put down", state.resumes.len()),
        state
            .resumes
            .iter()
            .enumerate()
            .map(|(at, note)| {
                format!(
                    "{}. {:<34}{} of {} written, {} failed  ({})",
                    at + 1,
                    note.folder,
                    note.done,
                    note.total,
                    note.failed,
                    note.at
                )
            })
            .collect(),
    )
}

fn note_at(state: &State, words: &[String]) -> Option<String> {
    let at = words.first()?.parse::<usize>().ok()?;
    state
        .resumes
        .get(at.checked_sub(1)?)
        .map(|note| note.file_name())
}

fn resume_start(state: &mut State, words: &[String]) -> Said {
    let Some(which) = note_at(state, words) else {
        return Said::No("which one? try resume.list".into());
    };
    state.resume_export(which);
    Said::Quiet
}

fn resume_forget(state: &mut State, words: &[String]) -> Said {
    let Some(which) = note_at(state, words) else {
        return Said::No("which one? try resume.list".into());
    };
    state.forget_resume(which);
    Said::Ok("forgotten".into())
}

fn skip_list(state: &mut State, _words: &[String]) -> Said {
    match state.settings.skip.is_empty() {
        true => Said::Ok("nothing is being left out".into()),
        false => Said::Deep(
            format!("{} left out of a library extract", state.settings.skip.len()),
            state.settings.skip.clone(),
        ),
    }
}

fn skip_add(state: &mut State, words: &[String]) -> Said {
    let Some(name) = words.first().filter(|word| !word.is_empty()) else {
        return Said::No("which bucket? try skip.list".into());
    };
    state.toggle_left_out(name);
    Said::Ok(match state.settings.skip.iter().any(|other| other == name) {
        true => format!("{name} will be left out"),
        false => format!("{name} is back in"),
    })
}

fn skip_clear(state: &mut State, _words: &[String]) -> Said {
    state.settings.skip.clear();
    crate::hm::storage::save(&state.settings);
    state.dirty = true;
    Said::Ok("everything is back in".into())
}

fn crash_dump(state: &mut State, words: &[String]) -> Said {
    state.tell_crash(true);
    let why = match words.is_empty() {
        true => "asked at the console".to_string(),
        false => words.join(" "),
    };
    match crate::hm::crash::dump(&why) {
        Some(path) => Said::Ok(path.to_string_lossy().to_ascii_lowercase()),
        None => Said::No("the report could not be written".into()),
    }
}

fn crash_list(_state: &mut State, _words: &[String]) -> Said {
    let reports = crate::hm::crash::reports();
    if reports.is_empty() {
        return Said::Ok("no crash reports; nothing has gone that wrong".into());
    }
    Said::Deep(
        format!("{} reports in {}", reports.len(), crate::hm::crash::where_they_are()),
        reports
            .iter()
            .take(12)
            .map(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect(),
    )
}

fn play(state: &mut State, _words: &[String]) -> Said {
    let Some(index) = state.cursor else {
        return Said::No("nothing is under the cursor".into());
    };
    state.select(index, true);
    Said::Quiet
}

fn stop(state: &mut State, _words: &[String]) -> Said {
    if let Some(transport) = state.transport.as_mut() {
        transport.stop();
    }
    Said::Quiet
}

fn discord(state: &mut State, words: &[String]) -> Said {
    state.settings.discord = match words.first().map(String::as_str) {
        Some("on") => true,
        Some("off") => false,
        _ => !state.settings.discord,
    };
    crate::hm::storage::save(&state.settings);
    state.push_presence();
    Said::Ok(match state.settings.discord {
        true => "rich presence on".into(),
        false => "rich presence off".into(),
    })
}

fn quit(state: &mut State, _words: &[String]) -> Said {
    state.wants_close = true;
    Said::Quiet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_spelled_once() {
        let mut names: Vec<&str> = ALL.iter().map(|command| command.name).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn a_line_splits_on_spaces_and_keeps_quotes_whole() {
        assert_eq!(split("export.tree \"category/package\""), vec![
            "export.tree".to_string(),
            "category/package".to_string()
        ]);
        assert_eq!(split("  help  "), vec!["help".to_string()]);
    }

    #[test]
    fn a_few_letters_find_the_command_they_meant() {
        let names: Vec<String> = name_offers("elib")
            .into_iter()
            .map(|offer| offer.text)
            .collect();
        assert_eq!(names.first().map(String::as_str), Some("export.library"));

        let stem: Vec<String> = name_offers("lane")
            .into_iter()
            .map(|offer| offer.text)
            .collect();
        assert!(stem.iter().any(|name| name == "lane.split"));
    }

    #[test]
    fn a_typo_is_pointed_at_the_thing_it_nearly_was() {
        assert_eq!(nearest("quti"), Some("quit"));
        assert_eq!(nearest("xyzzy"), None);
    }

    #[test]
    fn a_prefix_beats_letters_found_scattered_about() {
        // A prefix scores zero; letters found scattered through a name score
        // worse, and a letter that is not there at all does not match.
        assert_eq!(rank("que", "queue"), Some(0));
        assert!(rank("lane", "lane.split") < rank("lsplit", "lane.split"));
        assert!(rank("zzz", "queue").is_none());
    }
}
