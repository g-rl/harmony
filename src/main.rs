#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hm;

use hm::app::Harmony;

/// Headless check: open a battle.net casc storage and say what is in it.
fn casc(root: &std::path::Path, wanted: Option<&str>) {
    let started = std::time::Instant::now();
    let mut report = |at: usize, of: usize, what: &str| println!("  {at}/{of} {what}");
    let storage = match hm::pack::casc::Storage::open(root, &mut report) {
        Ok(storage) => storage,
        Err(error) => {
            println!("casc: {error}");
            return;
        }
    };
    println!(
        "product {} build {} - {} files in {:.1}s",
        storage.product,
        storage.build.clone().unwrap_or_else(|| "-".into()),
        storage.files.len(),
        started.elapsed().as_secs_f32()
    );

    // What kinds of file this install actually holds, biggest groups first.
    let mut kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for file in &storage.files {
        let ext = file
            .path
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_string())
            .unwrap_or_else(|| "(none)".into());
        *kinds.entry(ext).or_insert(0) += 1;
    }
    let mut kinds: Vec<(String, usize)> = kinds.into_iter().collect();
    kinds.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (ext, count) in kinds.iter().take(12) {
        println!("  {count:>7}  .{ext}");
    }

    let Some(wanted) = wanted else { return };
    let picked: Vec<&hm::pack::casc::tvfs::File> = storage.with_extension(&[wanted]);
    println!("{} files ending .{wanted}", picked.len());
    for file in picked.iter().take(10) {
        println!("  {} {} bytes", file.path, file.size);
    }
    // And prove one of them actually comes out.
    if let Some(file) = picked.first() {
        match storage.read_key(&file.ekey) {
            Ok(blob) => println!(
                "read {}: {} bytes, starts {:02x?}",
                file.path,
                blob.len(),
                &blob[..blob.len().min(8)]
            ),
            Err(error) => println!("{} could not be read: {error}", file.path),
        }
    }
}

/// What harmony can write and where, which is the same measurement the window
/// blocks on. With no path, it reports the folders harmony uses itself.
fn room(path: Option<std::path::PathBuf>) {
    use hm::storage::space;
    let places = match path {
        Some(path) => vec![("asked".to_string(), path)],
        None => vec![
            ("cache".to_string(), hm::storage::cache_dir()),
            (
                "scratch".to_string(),
                hm::window::dragout::scratch_dir(),
            ),
            ("settings".to_string(), hm::storage::config_dir()),
        ],
    };
    for (label, path) in places {
        match space::free(&path) {
            Some(free) => println!(
                "{label:<9} {}  {} free  headroom {}",
                path.display(),
                bytes(free),
                bytes(space::HEADROOM)
            ),
            None => println!("{label:<9} {}  room unknown", path.display()),
        }
    }
}

fn bytes(value: u64) -> String {
    const UNITS: [&str; 5] = ["b", "k", "m", "g", "t"];
    let mut size = value as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1}{}", UNITS[unit])
}

fn main() -> eframe::Result<()> {
    // Headless runs write the same caches the window reads, so they follow the
    // same folder settings.
    hm::storage::use_folders(&hm::storage::load());
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(root) = args.iter().position(|a| a == "--probe").and_then(|i| args.get(i + 1)) {
        probe(std::path::Path::new(root));
        return Ok(());
    }
    if let Some(root) = args
        .iter()
        .position(|a| a == "--packages")
        .and_then(|i| args.get(i + 1))
    {
        packages(std::path::Path::new(root));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--pull") {
        let (Some(root), Some(count), Some(out)) =
            (args.get(at + 1), args.get(at + 2), args.get(at + 3))
        else {
            println!("--pull <game folder> <count> <output folder>");
            return Ok(());
        };
        pull(
            std::path::Path::new(root),
            count.parse().unwrap_or(8),
            std::path::Path::new(out),
        );
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--casc") {
        let Some(root) = args.get(at + 1) else {
            println!("--casc <game folder> [extension]");
            return Ok(());
        };
        casc(std::path::Path::new(root), args.get(at + 2).map(String::as_str));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--casc") {
        let Some(root) = args.get(at + 1) else {
            println!("--casc <game folder> [extension]");
            return Ok(());
        };
        casc(std::path::Path::new(root), args.get(at + 2).map(String::as_str));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--room") {
        room(args.get(at + 1).map(std::path::PathBuf::from));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--stream") {
        let Some(path) = args.get(at + 1) else {
            println!("--stream <file>");
            return Ok(());
        };
        stream(std::path::Path::new(path));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--inflate") {
        let (Some(path), Some(out)) = (args.get(at + 1), args.get(at + 2)) else {
            println!("--inflate <fastfile> <output file>");
            return Ok(());
        };
        inflate(std::path::Path::new(path), std::path::Path::new(out));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--zone") {
        let Some(path) = args.get(at + 1) else {
            println!("--zone <fastfile> [output folder]");
            return Ok(());
        };
        zone_sounds(
            std::path::Path::new(path),
            args.get(at + 2).map(std::path::Path::new),
        );
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--sort") {
        let (Some(game), Some(depth)) = (args.get(at + 1), args.get(at + 2)) else {
            println!("--sort <game key> <depth>");
            return Ok(());
        };
        sort_of(game, depth);
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--names-probe") {
        let (Some(game), Some(depth), Some(list)) = (args.get(at + 1), args.get(at + 2), args.get(at + 3)) else {
            println!("--names-probe <game key> <depth> <list.csv>");
            return Ok(());
        };
        names_probe(game, depth, std::path::Path::new(list));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--names") {
        let (Some(game), Some(depth)) = (args.get(at + 1), args.get(at + 2)) else {
            println!("--names <game key> <depth> <list...>");
            return Ok(());
        };
        let lists: Vec<std::path::PathBuf> = args[at + 3..]
            .iter()
            .map(std::path::PathBuf::from)
            .collect();
        names_against(game, depth, lists);
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--hash") {
        let names: Vec<&String> = args[at + 1..].iter().collect();
        if names.is_empty() {
            println!("--hash <name> [name...]");
            return Ok(());
        }
        for name in names {
            println!("{name}");
            for how in hm::catalog::hash::HashFn::all() {
                println!("  {how:?}: {:016X}", how.of(name));
            }
        }
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--sound") {
        let Some(path) = args.get(at + 1) else {
            println!("--sound <file> [output.wav]");
            return Ok(());
        };
        sound(std::path::Path::new(path), args.get(at + 2).map(std::path::Path::new));
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--grab") {
        let (Some(root), Some(needle), Some(key)) = (args.get(at + 1), args.get(at + 2), args.get(at + 3)) else {
            println!("--grab <game folder> <package name> <key>");
            return Ok(());
        };
        grab(std::path::Path::new(root), needle, key);
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--dump") {
        let (Some(root), Some(needle)) = (args.get(at + 1), args.get(at + 2)) else {
            println!("--dump <game folder> <package name> [count]");
            return Ok(());
        };
        let count = args.get(at + 3).and_then(|n| n.parse().ok()).unwrap_or(6);
        dump(std::path::Path::new(root), needle, count);
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--opus-keys") {
        let (Some(root), Some(needle)) = (args.get(at + 1), args.get(at + 2)) else {
            println!("--opus-keys <game folder> <package name>");
            return Ok(());
        };
        hits(std::path::Path::new(root), needle, true);
        return Ok(());
    }
    if let Some(at) = args.iter().position(|a| a == "--hits") {
        let (Some(root), Some(needle)) = (args.get(at + 1), args.get(at + 2)) else {
            println!("--hits <game folder> <package name>");
            return Ok(());
        };
        hits(std::path::Path::new(root), needle, false);
        return Ok(());
    }

    // Opening a folder from the command line, for shortcuts and for testing.
    if let Some(at) = args.iter().position(|a| a == "--root")
        && let Some(root) = args.get(at + 1)
    {
        let mut settings = hm::storage::load();
        settings.last_root = Some(std::path::PathBuf::from(root));
        hm::storage::save(&settings);
    }

    // The window wears harmony's own title bar, so the system one is turned
    // off: the name, the icon and the buttons are the top row of the app.
    let size = hm::storage::load()
        .window_size
        .unwrap_or([1360.0, 820.0]);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("harmony")
            .with_app_id("harmony")
            .with_decorations(false)
            .with_resizable(true)
            .with_inner_size(size)
            .with_min_inner_size([980.0, 620.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    // Draw on the gpu where there is one, and fall back rather than fail where
    // there is not: a machine with no vulkan or dx12 still has opengl, and a
    // remote session may have neither working at all. Each renderer is tried in
    // turn, and only a failure of the last one is an error the user sees.
    let ladder = [
        (eframe::Renderer::Wgpu, "gpu"),
        (eframe::Renderer::Glow, "opengl"),
    ];
    let mut last: eframe::Result<()> = Ok(());
    for (renderer, name) in ladder {
        let mut options = options.clone();
        options.renderer = renderer;
        match eframe::run_native(
            "harmony",
            options,
            Box::new(|cc| Ok(Box::new(Harmony::new(cc)))),
        ) {
            Ok(()) => return Ok(()),
            Err(error) => {
                println!("the {name} renderer would not start: {error}");
                last = Err(error);
            }
        }
    }
    last

}

fn window_icon() -> eframe::egui::IconData {
    let bytes = include_bytes!("../images/chinchou.png");
    match image::load_from_memory(bytes) {
        Ok(image) => {
            let image = image.to_rgba8();
            let (width, height) = (image.width(), image.height());
            eframe::egui::IconData {
                rgba: image.into_raw(),
                width,
                height,
            }
        }
        Err(_) => eframe::egui::IconData {
            rgba: vec![0; 4],
            width: 1,
            height: 1,
        },
    }
}

/// Headless check: detect the title in `root`, mount it, run a quick scan and
/// print what was found. Used to test the reader without opening the window.
fn probe(root: &std::path::Path) {
    use hm::game::{detect, title_for};
    use hm::scan::{self, Depth, Msg};

    let prints = detect(root);
    for print in &prints {
        println!(
            "detected {} score {} reason {} build {}",
            print.title.key(),
            print.score,
            print.reason,
            print.build.as_deref().unwrap_or("-")
        );
    }
    let Some(first) = prints.first() else {
        println!("no title matched {}", root.display());
        return;
    };
    let Some(title) = title_for(first.title) else {
        println!("no reader for {}", first.title.key());
        return;
    };

    let deep = std::env::args().any(|a| a == "deep");
    let full = std::env::args().any(|a| a == "full");
    let started = std::time::Instant::now();
    let job = scan::start(
        title,
        root.to_path_buf(),
        if full {
            Depth::Full
        } else if deep {
            Depth::Deep
        } else {
            Depth::Quick
        },
    );
    let mut found = 0usize;
    let mut keep: Vec<hm::catalog::Entry> = Vec::new();
    let mut sample: Vec<String> = Vec::new();
    loop {
        match job.rx.recv() {
            Ok(Msg::Found(batch)) => {
                for entry in &batch {
                    if sample.len() < 5 {
                        sample.push(format!(
                            "{} {}ch {:.2}s {}b",
                            entry.display(),
                            entry.channels,
                            entry.seconds(),
                            entry.bytes
                        ));
                    }
                }
                found += batch.len();
                keep.extend(batch);
            }
            Ok(Msg::Progress { .. }) | Ok(Msg::Checked { .. }) => {}
            Ok(Msg::Ready(mount)) => {
                println!(
                    "mounted {} packages, {} keys, {} zones",
                    mount.store.info().len(),
                    mount.store.keys(),
                    mount.zones.zones.len()
                );
                let packages: Vec<String> = mount
                    .store
                    .info()
                    .iter()
                    .map(|info| info.name.clone())
                    .collect();
                for (index, entry) in keep.iter_mut().enumerate() {
                    entry.id = hm::catalog::SoundId(index as u32);
                }
                let _ = (first, packages);
            }
            // The scan writes its own cache, from the thread that read it.
            Ok(Msg::Cached { sounds }) => {
                println!("cached {sounds} sounds");
                break;
            }
            Ok(Msg::NoRoom(room)) => {
                println!(
                    "not cached: {} needs {} and has {} free",
                    room.path.display(),
                    room.needed(),
                    room.free
                );
                break;
            }
            Ok(Msg::CacheFailed(error)) => {
                println!("the cache could not be written: {error}");
                break;
            }
            Ok(Msg::Failed(error)) => {
                println!("failed: {error}");
                break;
            }
            Err(_) => break,
        }
    }
    println!("{found} sounds in {:.1}s", started.elapsed().as_secs_f32());
    for line in sample {
        println!("  {line}");
    }
}

/// Headless check: mount `root` and print what each package holds.
fn packages(root: &std::path::Path) {
    use hm::game::{detect, title_for};

    let Some(print) = detect(root).into_iter().next() else {
        println!("no title matched {}", root.display());
        return;
    };
    let Some(title) = title_for(print.title) else {
        return;
    };
    let mut quiet = |_: usize, _: usize, _: &str| {};
    let mount = match title.mount(root, &mut quiet) {
        Ok(mount) => mount,
        Err(error) => {
            println!("failed: {error}");
            return;
        }
    };
    let mut total = 0usize;
    for info in mount.store.info() {
        total += info.entries;
        println!("{:>9} {}", info.entries, info.name);
    }
    println!("{total} entries in {} packages", mount.store.info().len());
}

/// Headless check: scan one package and report how much of it is audio.
fn hits(root: &std::path::Path, needle: &str, list: bool) {
    use hm::game::{detect, title_for};
    use hm::pack::kapi::Package;
    use hm::pack::oodle::Oodle;
    use hm::scan::{HEAD, looks_like_sound};
    use hm::sound::opus;

    let Some(print) = detect(root).into_iter().next() else {
        println!("no title matched {}", root.display());
        return;
    };
    let Some(title) = title_for(print.title) else {
        return;
    };
    let mut quiet = |_: usize, _: usize, _: &str| {};
    let Ok(mount) = title.mount(root, &mut quiet) else {
        println!("mount failed");
        return;
    };
    let oodle = Oodle::find(root).ok();

    for info in mount.store.info().iter().filter(|i| i.name.contains(needle)) {
        let Ok(mut package) = Package::open(&info.path) else {
            continue;
        };
        let started = std::time::Instant::now();
        let mut looked = 0usize;
        let mut flagged = 0usize;
        let mut sounds = 0usize;
        let mut broken = 0usize;
        let mut bytes = 0u64;
        let mut order = package.entries.clone();
        order.sort_by_key(|entry| entry.offset);
        for entry in order {
            if entry.size < 64 {
                continue;
            }
            looked += 1;
            bytes += entry.size as u64;
            let Ok(head) = package.read_head(entry, oodle.as_ref(), HEAD) else {
                broken += 1;
                continue;
            };
            if !looks_like_sound(&head) {
                continue;
            }
            flagged += 1;
            if opus::probe_head(&head).is_some() {
                sounds += 1;
            } else if let Ok(blob) = package.read(entry, oodle.as_ref())
                && opus::probe(&blob).is_some()
            {
                sounds += 1;
            } else {
                continue;
            }
            if list {
                println!("{:016x} {} {}", entry.key, entry.size, info.name);
            }
        }
        let seconds = started.elapsed().as_secs_f32();
        println!(
            "{}: {looked} looked, {broken} unreadable, {flagged} flagged, {sounds} opus, {} packed, {seconds:.1}s ({:.0}/s)",
            info.name,
            hm::ui::widgets::bytes(bytes),
            looked as f32 / seconds.max(0.001)
        );
    }
}

/// Headless check: print the first bytes of the first entries of a package,
/// both as stored on disk and after the block chain, to see how they are laid out.
fn dump(root: &std::path::Path, needle: &str, count: usize) {
    use hm::game::{detect, title_for};
    use hm::pack::kapi::Package;
    use hm::pack::oodle::Oodle;
    use std::io::{Read, Seek, SeekFrom};

    let Some(print) = detect(root).into_iter().next() else {
        return;
    };
    let Some(title) = title_for(print.title) else {
        return;
    };
    let mut quiet = |_: usize, _: usize, _: &str| {};
    let Ok(mount) = title.mount(root, &mut quiet) else {
        return;
    };
    let oodle = Oodle::find(root).ok();

    let Some(info) = mount.store.info().iter().find(|i| i.name == needle) else {
        println!("no package called {needle}");
        return;
    };
    println!("{} ({} entries)", info.name, info.entries);
    let Ok(mut package) = Package::open(&info.path) else {
        return;
    };
    let mut file = std::fs::File::open(&info.path).unwrap();
    let mut keyed = 0usize;
    let mut raw = 0usize;
    for entry in package.entries.clone() {
        let mut probe = [0u8; 8];
        file.seek(SeekFrom::Start(entry.offset + 2)).unwrap();
        if file.read_exact(&mut probe).is_err() {
            continue;
        }
        if u64::from_le_bytes(probe) == entry.key {
            keyed += 1;
        } else {
            raw += 1;
        }
    }
    println!("{keyed} entries start with their key, {raw} do not");

    let odd: Vec<_> = package
        .entries
        .clone()
        .into_iter()
        .filter(|entry| {
            let mut probe = [0u8; 8];
            file.seek(SeekFrom::Start(entry.offset + 2)).unwrap();
            file.read_exact(&mut probe).is_ok() && u64::from_le_bytes(probe) != entry.key
        })
        .take(count)
        .collect();
    for entry in odd {
        let mut head = [0u8; 32];
        file.seek(SeekFrom::Start(entry.offset)).unwrap();
        let _ = file.read(&mut head);
        let stored: String = head.iter().map(|b| format!("{b:02x}")).collect();
        let decoded = package
            .read_head(entry, oodle.as_ref(), 32)
            .map(|blob| {
                blob.iter()
                    .take(32)
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            })
            .unwrap_or_else(|error| error.to_string());
        println!(
            "key {:016x} at {:#x} size {:>8}\n  disk {stored}\n  read {decoded}",
            entry.key, entry.offset, entry.size
        );
    }
}

/// Headless check: dump one entry of a package to a file and print its first words.
fn grab(root: &std::path::Path, needle: &str, key_text: &str) {
    use hm::game::{detect, title_for};
    use hm::pack::kapi::Package;
    use hm::pack::oodle::Oodle;

    let Ok(key) = u64::from_str_radix(key_text, 16) else {
        println!("key must be hex");
        return;
    };
    let Some(print) = detect(root).into_iter().next() else {
        return;
    };
    let Some(title) = title_for(print.title) else {
        return;
    };
    let mut quiet = |_: usize, _: usize, _: &str| {};
    let Ok(mount) = title.mount(root, &mut quiet) else {
        return;
    };
    let oodle = Oodle::find(root).ok();
    let Some(info) = mount.store.info().iter().find(|i| i.name == needle) else {
        return;
    };
    let Ok(mut package) = Package::open(&info.path) else {
        return;
    };
    let Some(entry) = package.entries.clone().into_iter().find(|e| e.key == key) else {
        println!("no entry {key_text} in {needle}");
        return;
    };
    let Ok(blob) = package.read(entry, oodle.as_ref()) else {
        println!("unreadable");
        return;
    };
    let out = std::env::temp_dir().join(format!("{key_text}.bin"));
    let _ = std::fs::write(&out, &blob);
    println!("{} bytes -> {}", blob.len(), out.display());
    for row in 0..8 {
        let words: Vec<String> = (0..8)
            .map(|i| {
                let at = row * 32 + i * 4;
                if at + 4 <= blob.len() {
                    format!("{:>10}", u32::from_le_bytes(blob[at..at + 4].try_into().unwrap()))
                } else {
                    "         -".to_string()
                }
            })
            .collect();
        println!("{:#06x} {}", row * 32, words.join(" "));
    }
}

/// Headless check: take a dumped blob and work out where its seek table ends
/// and whether the packet chain after it agrees with the table.
/// What the rules make of a game harmony has already read: how many sounds
/// land in each category, and in each of the finer buckets.
fn sort_of(game: &str, depth: &str) {
    use std::collections::BTreeMap;

    let Some(cached) = hm::storage::load_cache(game, depth) else {
        println!("no {depth} scan of {game} is cached");
        return;
    };
    let mut counts: BTreeMap<(String, String), (usize, Vec<String>)> = BTreeMap::new();
    let mut named = 0usize;
    for sound in &cached.sounds {
        let Some(name) = sound.name.as_deref() else {
            continue;
        };
        named += 1;
        let category = hm::catalog::category::of(name).label().to_string();
        let sub = hm::catalog::category::sub_of(name).to_string();
        let row = counts.entry((category, sub)).or_insert((0, Vec::new()));
        row.0 += 1;
        if row.1.len() < 3 {
            row.1.push(name.to_string());
        }
    }
    println!("{named} named sounds of {}", cached.sounds.len());
    let mut rows: Vec<_> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    for ((category, sub), (count, samples)) in rows {
        println!("  {category:<14} {sub:<14} {count:<8} {}", samples.join(" | "));
    }
}

/// Which hash function, over which shape of the name, produces the keys a
/// game actually uses.
///
/// A published list is a list of names. Whether those names are the ones the
/// container was keyed by, and whether the engine hashed them whole or after
/// some prefix, is not written down anywhere — so harmony tries the
/// combinations and counts the hits.
fn names_probe(game: &str, depth: &str, list: &std::path::Path) {
    use hm::catalog::hash::HashFn;
    use std::collections::HashSet;

    let Some(cached) = hm::storage::load_cache(game, depth) else {
        println!("no {depth} scan of {game} is cached");
        return;
    };
    let mut keys: HashSet<u64> = HashSet::new();
    for sound in &cached.sounds {
        keys.insert(sound.key);
        keys.insert(sound.key & hm::catalog::hash::ASSET_MASK);
        keys.insert(sound.key & hm::catalog::hash::NAME_MASK);
    }
    let Ok(text) = std::fs::read_to_string(list) else {
        println!("cannot read {}", list.display());
        return;
    };
    let names: Vec<&str> = text
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(','))
        .map(|(_, name)| name.trim())
        .collect();
    println!("{} keys, {} names", keys.len(), names.len());

    let shapes: Vec<(&str, fn(&str) -> String)> = vec![
        ("as written", |n| n.to_string()),
        ("sound/ + name", |n| format!("sound/{n}")),
        ("sounds/ + name", |n| format!("sounds/{n}")),
        ("name + .wav", |n| format!("{n}.wav")),
        ("name + .wem", |n| format!("{n}.wem")),
        ("up to first dot", |n| {
            n.split_once('.').map(|(head, _)| head.to_string()).unwrap_or_else(|| n.to_string())
        }),
        ("last segment", |n| {
            n.rsplit('/').next().unwrap_or(n).to_string()
        }),
        ("last segment to first dot", |n| {
            let last = n.rsplit('/').next().unwrap_or(n);
            last.split_once('.').map(|(head, _)| head.to_string()).unwrap_or_else(|| last.to_string())
        }),
        ("without the first folder", |n| {
            n.split_once('/').map(|(_, rest)| rest.to_string()).unwrap_or_else(|| n.to_string())
        }),
    ];

    for how in HashFn::all() {
        for (label, shape) in &shapes {
            let mut hits = 0usize;
            for name in &names {
                let shaped = shape(name);
                let value = how.of(&shaped);
                if keys.contains(&value)
                    || keys.contains(&(value & hm::catalog::hash::ASSET_MASK))
                    || keys.contains(&(value & hm::catalog::hash::NAME_MASK))
                {
                    hits += 1;
                }
            }
            if hits > 0 {
                println!("  {how:?} over {label}: {hits} hits");
            }
        }
    }
    println!("done");
}

/// How much of a cached scan a set of name lists can actually name.
///
/// Harmony ships no names. This is how a list someone else published is
/// checked against a game harmony has already read, without scanning it again.
fn names_against(game: &str, depth: &str, lists: Vec<std::path::PathBuf>) {
    let Some(cached) = hm::storage::load_cache(game, depth) else {
        println!("no {depth} scan of {game} is cached");
        return;
    };
    let mut names = hm::catalog::names::NameDb::default();
    let started = std::time::Instant::now();
    let added = hm::app::read_names(&mut names, lists);
    println!(
        "{added} names read in {:.1}s, {} keys filed",
        started.elapsed().as_secs_f32(),
        names.len()
    );
    let mut matched = 0usize;
    let mut sample: Vec<String> = Vec::new();
    for sound in &cached.sounds {
        if let Some(name) = names.lookup(sound.key) {
            matched += 1;
            if sample.len() < 8 {
                sample.push(format!("{:016X} {name}", sound.key));
            }
        }
    }
    println!(
        "{matched} of {} sounds named ({:.1}%)",
        cached.sounds.len(),
        100.0 * matched as f32 / cached.sounds.len().max(1) as f32
    );
    for line in sample {
        println!("  {line}");
    }
}

/// Decode one file on its own and say what came out: what the sniffer made of
/// it, and whether the samples look like audio rather than like noise.
fn sound(path: &std::path::Path, out: Option<&std::path::Path>) {
    let Ok(raw) = std::fs::read(path) else {
        println!("cannot read {}", path.display());
        return;
    };
    match hm::sound::decode::sniff(&raw) {
        Some(about) => println!(
            "{} → {} {}hz {}ch, samples from {}",
            path.display(),
            about.codec.label(),
            about.rate,
            about.channels,
            about.data_at
        ),
        None => println!("{}: nothing recognised it", path.display()),
    }
    let about = hm::sound::decode::sniff(&raw).unwrap_or_default();
    let samples = match hm::sound::decode::decode(&raw, about) {
        Ok(samples) => samples,
        Err(error) => {
            println!("decode failed: {error}");
            return;
        }
    };
    let frames = samples.pcm.len() / samples.channels.max(1) as usize;
    let mut sum = 0f64;
    let mut peak = 0i32;
    let mut clipped = 0usize;
    for value in &samples.pcm {
        let v = *value as f64;
        sum += v * v;
        peak = peak.max((*value as i32).abs());
        if value.abs() >= 32_000 {
            clipped += 1;
        }
    }
    let rms = (sum / samples.pcm.len().max(1) as f64).sqrt();
    println!(
        "{} frames, {:.2}s, rms {:.0}, peak {}, {:.2}% near full scale",
        frames,
        frames as f32 / samples.rate.max(1) as f32,
        rms,
        peak,
        100.0 * clipped as f64 / samples.pcm.len().max(1) as f64
    );
    if let Some(out) = out {
        let mut file = hm::sound::decode::wav_header(
            samples.rate,
            samples.channels,
            16,
            (samples.pcm.len() * 2) as u32,
        );
        for value in &samples.pcm {
            file.extend_from_slice(&value.to_le_bytes());
        }
        match std::fs::write(out, &file) {
            Ok(()) => println!("wrote {}", out.display()),
            Err(error) => println!("could not write {}: {error}", out.display()),
        }
    }
}

/// What is inside one fastfile, and optionally the first few of them as wavs.
///
/// The zone readers are the hardest part of harmony to be sure about from the
/// window alone: a zone is a linear dump with no table, so the only proof that
/// a sound was found and not invented is writing it out and listening to it.
/// Write a modern fastfile's zone out decompressed, which is how its asset
/// records get looked at in a hex editor.
fn inflate(path: &std::path::Path, out: &std::path::Path) {
    use hm::pack::oodle::Oodle;
    use hm::zone::{assets, xfile};

    let root = path.parent().and_then(|p| p.parent()).unwrap_or(path);
    let oodle = Oodle::find(root).ok();
    let zone = match xfile::open(path, oodle.as_ref()) {
        Ok(zone) => zone,
        Err(error) => {
            println!("{error}");
            return;
        }
    };
    println!(
        "{} {} bytes, header says {}",
        zone.header.flavour.label(),
        zone.data.len(),
        zone.header.size
    );
    if let Some(inventory) = assets::inventory(&zone.data) {
        println!(
            "{} assets, table at {:#x}..{:#x}",
            inventory.assets, inventory.array_at, inventory.array_end
        );
        for (kind, count) in &inventory.by_type {
            println!("  {kind:#x} {count}");
        }
    }
    match std::fs::write(out, &zone.data) {
        Ok(_) => println!("wrote {}", out.display()),
        Err(error) => println!("{error}"),
    }
}

fn zone_sounds(path: &std::path::Path, out: Option<&std::path::Path>) {
    use hm::sound::decode;
    use hm::zone::iw5;

    let zone = match iw5::inflate(path) {
        Ok(zone) => zone,
        Err(error) => {
            println!("{error}");
            return;
        }
    };
    let sounds = iw5::sounds(&zone);
    println!(
        "{} inflated to {} bytes, {} sounds",
        path.display(),
        zone.len(),
        sounds.len()
    );
    for sound in sounds.iter().take(8) {
        println!(
            "  {} {}ch {}hz {} frames {} bytes",
            sound.name,
            sound.channels,
            sound.rate,
            sound.frames(),
            sound.bytes
        );
    }
    let Some(out) = out else {
        return;
    };
    let _ = std::fs::create_dir_all(out);
    for sound in sounds.iter().take(8) {
        let end = (sound.at + sound.bytes as usize).min(zone.len());
        let body = &zone[sound.at..end];
        let mut wav = decode::wav_header(sound.rate, sound.channels, sound.bits, body.len() as u32);
        wav.extend_from_slice(body);
        let stem: String = sound
            .name
            .chars()
            .map(|c| match c.is_ascii_alphanumeric() {
                true => c,
                false => '_',
            })
            .collect();
        let at = out.join(format!("{stem}.wav"));
        match std::fs::write(&at, &wav) {
            Ok(_) => println!("  wrote {}", at.display()),
            Err(error) => println!("  {error}"),
        }
    }
}

fn stream(path: &std::path::Path) {
    let Ok(raw) = std::fs::read(path) else {
        println!("cannot read {}", path.display());
        return;
    };
    let word = |at: usize| -> Option<u32> {
        raw.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };

    for base in [0usize, 32] {
        let mut count = 0usize;
        let mut last = None;
        while let Some(value) = word(base + count * 4) {
            if let Some(previous) = last
                && value <= previous
            {
                break;
            }
            if (count > 0 && value == 0) || value as usize > raw.len() {
                break;
            }
            last = Some(value);
            count += 1;
        }
        if count < 3 {
            continue;
        }
        let table_end = base + count * 4;
        let span = last.unwrap_or(0) as usize;
        println!(
            "table at {base}: {count} entries, ends {table_end:#x}, last value {span} ({span:#x}), blob {} ({:#x})",
            raw.len(),
            raw.len()
        );
        println!("  table_end + last = {:#x}", table_end + span);
        // Walk the chain from a few candidate starts and see how far it gets.
        for start in [table_end, table_end + (8 - table_end % 8) % 8] {
            let mut at = start;
            let mut packets = 0usize;
            let mut offsets: Vec<usize> = Vec::new();
            while at + 2 <= raw.len() {
                let len = u16::from_le_bytes(raw[at..at + 2].try_into().unwrap()) as usize;
                if len == 0 || len > 8192 || at + 2 + len > raw.len() {
                    break;
                }
                offsets.push(at - start);
                at += 2 + len;
                packets += 1;
            }
            let agrees = offsets
                .iter()
                .enumerate()
                .take(count)
                .filter(|(i, off)| word(base + i * 4) == Some(**off as u32))
                .count();
            println!(
                "  from {start:#x}: {packets} packets, stops at {at:#x}, {agrees}/{} match the table",
                offsets.len().min(count)
            );
            if packets > 2 {
                let guess = hm::sound::opus::Stream {
                    seek_table: start,
                    packets,
                    channels: if (raw[start + 2] >> 2) & 1 == 1 { 2 } else { 1 },
                    frames: (packets * hm::sound::opus::FRAME) as u64,
                    wide: false,
                };
                match hm::sound::opus::decode(&raw, guess) {
                    Ok(pcm) => {
                        let peak = pcm.iter().map(|s| s.unsigned_abs() as u32).max().unwrap_or(0);
                        println!(
                            "    decoded {} samples, {} channels, peak {peak}",
                            pcm.len(),
                            guess.channels
                        );
                    }
                    Err(error) => println!("    decode failed: {error}"),
                }
            }
        }
    }
}

/// Headless check: scan, then extract the first `count` sounds both ways and
/// say what landed where.
fn pull(root: &std::path::Path, count: usize, out: &std::path::Path) {
    use hm::export::{Options, queue::Queue};
    use hm::game::{detect, title_for};
    use hm::scan::{self, Depth, Msg};
    use std::sync::{Arc, Mutex};

    let Some(print) = detect(root).into_iter().next() else {
        println!("no title matched {}", root.display());
        return;
    };
    let Some(title) = title_for(print.title) else {
        return;
    };
    let job = scan::start(title, root.to_path_buf(), Depth::Quick);
    let mut entries = Vec::new();
    let mut mount = None;
    loop {
        match job.rx.recv() {
            Ok(Msg::Found(batch)) => {
                for entry in batch {
                    if entries.len() < count {
                        entries.push(entry);
                    }
                }
                if entries.len() >= count {
                    job.cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Ok(Msg::Ready(ready)) => {
                mount = Some(Arc::new(Mutex::new(*ready)));
                break;
            }
            Ok(Msg::Cached { .. })
            | Ok(Msg::NoRoom(_))
            | Ok(Msg::CacheFailed(_))
            | Ok(Msg::Checked { .. }) => {}
            Ok(Msg::Failed(error)) => {
                println!("failed: {error}");
                return;
            }
            Ok(Msg::Progress { .. }) => {}
            Err(_) => break,
        }
    }
    let Some(mount) = mount else {
        println!("nothing mounted");
        return;
    };
    let packages: Vec<String> = mount
        .lock()
        .unwrap()
        .store
        .info()
        .iter()
        .map(|info| info.name.clone())
        .collect();

    // One queue, four runs, sent one after another without waiting: exactly
    // what the window does when a second export is asked for while one is
    // going. The line runs them in the order they were sent.
    let queue = Queue::default();
    for format in hm::export::FORMATS.iter().copied() {
        let mut options = Options::default();
        options.format = format;
        options.write_manifest = true;
        let folder = out.join(format.label());
        let _ = std::fs::create_dir_all(&folder);
        // The same log a library run leaves, so the headless run tests the
        // thing the window does rather than a shorter path through it.
        let mut log = hm::export::liblog::Log::new(&folder);
        log.say(format!("harmony pull  \u{b7} {}", hm::storage::stamp()));
        log.say(format!("from      {}", root.display()));
        log.say(format!("format    {}", format.label()));
        log.say(format!("sounds    {}", entries.len()));
        log.blank();
        let started = queue.submit(hm::export::queue::Run {
            id: hm::export::queue::next_id(),
            label: format!("{} \u{b7} {} sounds", format.label(), entries.len()),
            mount: mount.clone(),
            entries: entries.clone(),
            packages: packages.clone(),
            game: "jup".into(),
            root: folder,
            options,
            log: Some(log),
            resume: None,
        });
        println!(
            "{}: {}",
            format.label(),
            match started {
                true => "running",
                false => "queued",
            }
        );
    }

    // Wait for the whole line, saying what is going and what is behind it.
    let mut said = String::new();
    loop {
        let (running, waiting, label, done, failed, total) = {
            let progress = queue.progress.lock().unwrap();
            (
                progress.running,
                progress.waiting.len(),
                progress.label.clone(),
                progress.done,
                progress.failed,
                progress.total,
            )
        };
        if !running && waiting == 0 {
            break;
        }
        let now = format!("{label}: {done} written, {failed} failed of {total}, {waiting} waiting");
        if now != said {
            println!("{now}");
            said = now;
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }

    // What actually landed, counted off the disk rather than off the queue.
    for format in hm::export::FORMATS.iter().copied() {
        let folder = out.join(format.label());
        let written = std::fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir() || entry.path().is_file())
                    .count()
            })
            .unwrap_or(0);
        println!("{}: {written} at the top of {}", format.label(), folder.display());
    }
    println!("output under {}", out.display());
}

