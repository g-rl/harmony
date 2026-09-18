//! One way in to every kind of container harmony reads.
//!
//! The modern titles keep their audio in kapi packages; the older ones keep it
//! in zip archives, sab banks or sound paks. A [`Store`] is whichever of those
//! a mounted game turned out to use, and the rest of harmony only ever asks it
//! for a package list and for the bytes of an entry.

use std::path::Path;

use anyhow::{Result, anyhow};

use crate::hm::pack::oodle::Oodle;
use crate::hm::pack::{PackageId, PackageInfo, PackageSet, iwd, sab};
use crate::hm::sound::{Codec, decode};

/// What a container already knows about one of its entries, before anything is
/// decoded. The kapi packages know almost none of this and have to be probed;
/// the older containers carry it in their tables.
#[derive(Clone, Debug)]
pub struct Listing {
    pub index: u32,
    pub key: u64,
    /// Whether `key` came from the container.
    ///
    /// False where harmony worked one out itself, which is every carved
    /// stream: there is nothing to match a name list against, and nothing
    /// worth printing as if it were an id.
    pub keyed: bool,
    pub name: Option<String>,
    pub bytes: u64,
    pub rate: u32,
    pub channels: u8,
    pub frames: u64,
    pub codec: Codec,
}

pub enum Store {
    Kapi(PackageSet),
    Iwd(IwdSet),
    Sab(SabSet),
    Pak(PakSet),
    /// A battle.net install, where the sound paks are blobs in casc rather
    /// than files on disk.
    Casc(CascSet),
    /// Modern warfare 3, whose audio is inside its fastfiles.
    Ff(FfSet),
    /// Both of the above, for a game that keeps audio in each.
    Both(BothSet),
}

impl Default for Store {
    fn default() -> Store {
        Store::Kapi(PackageSet::default())
    }
}

impl Store {
    pub fn info(&self) -> &[PackageInfo] {
        match self {
            Store::Kapi(set) => &set.info,
            Store::Iwd(set) => &set.info,
            Store::Sab(set) => &set.info,
            Store::Pak(set) => &set.info,
            Store::Casc(set) => &set.info,
            Store::Ff(set) => &set.info,
            Store::Both(set) => &set.info,
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.info().iter().map(|info| info.name.clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.info().len()
    }

    pub fn is_empty(&self) -> bool {
        self.info().is_empty()
    }

    /// How many distinct entries the whole store holds.
    pub fn keys(&self) -> usize {
        match self {
            Store::Kapi(set) => set.keys(),
            other => other.info().iter().map(|info| info.entries).sum(),
        }
    }

    /// Everything a package holds, as far as its own tables say. Kapi packages
    /// answer with nothing: their entries have to be read to be known, which is
    /// what the scanner does.
    pub fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        match self {
            Store::Kapi(_) => Vec::new(),
            Store::Iwd(set) => set.listing(id),
            Store::Sab(set) => set.listing(id),
            Store::Pak(set) => set.listing(id),
            Store::Casc(set) => set.listing(id),
            Store::Ff(set) => set.listing(id),
            Store::Both(set) => set.listing(id),
        }
    }

    pub fn read(&mut self, id: PackageId, index: usize, oodle: Option<&Oodle>) -> Result<Vec<u8>> {
        match self {
            Store::Kapi(set) => set.read(id, index, oodle),
            Store::Iwd(set) => set.read(id, index),
            Store::Sab(set) => set.read(id, index),
            Store::Pak(set) => set.read(id, index),
            Store::Casc(set) => set.read(id, index),
            Store::Ff(set) => set.read(id, index),
            Store::Both(set) => set.read(id, index),
        }
    }

    pub fn kapi(&mut self) -> Option<&mut PackageSet> {
        match self {
            Store::Kapi(set) => Some(set),
            _ => None,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Store::Kapi(_) => "kapi packages",
            Store::Iwd(_) => "iwd archives",
            Store::Sab(_) => "sab banks",
            Store::Pak(_) => "sound paks",
            Store::Casc(_) => "casc sound paks",
            Store::Ff(_) => "fastfiles",
            Store::Both(_) => "iwd archives and fastfiles",
        }
    }
}

// ----------------------------------------------------------------------------

#[derive(Default)]
pub struct IwdSet {
    archives: Vec<iwd::Archive>,
    /// Which member of an archive each catalogued entry is, so the audio can be
    /// numbered without the rest of the zip getting in the way.
    audio: Vec<Vec<usize>>,
    pub info: Vec<PackageInfo>,
}

impl IwdSet {
    pub fn mount(&mut self, path: &Path) -> Result<PackageId> {
        let archive = iwd::Archive::open(path)?;
        let audio: Vec<usize> = archive
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| iwd::is_audio(&entry.name))
            .map(|(index, _)| index)
            .collect();
        let id = PackageId(self.archives.len() as u32);
        self.info.push(PackageInfo {
            id,
            name: archive.name(),
            path: archive.path.clone(),
            entries: audio.len(),
            version: 0,
            kind: 0,
        });
        self.audio.push(audio);
        self.archives.push(archive);
        Ok(id)
    }

    fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        let Some(members) = self.audio.get(id.0 as usize).cloned() else {
            return Vec::new();
        };
        let Some(archive) = self.archives.get_mut(id.0 as usize) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(members.len());
        for (index, member) in members.iter().enumerate() {
            let entry = archive.entries[*member].clone();
            let mut listing = Listing {
                index: index as u32,
                key: crate::hm::catalog::hash::fnv1a64(&entry.name),
                keyed: true,
                name: Some(entry.name.clone()),
                bytes: entry.plain,
                rate: 0,
                channels: 0,
                frames: 0,
                codec: codec_of(&entry.name),
            };
            // The wav header is a short read away, and carries the rate, the
            // channel count and the length. A deflated member is inflated only
            // as far as that header, not read through.
            if let Ok(head) = archive.read_head(*member, 8192) {
                if let Some(riff) = decode::riff(&head) {
                    listing.rate = riff.rate;
                    listing.channels = riff.channels;
                    listing.frames = riff.frames();
                    listing.codec = riff.codec();
                } else if let Some(t5) = crate::hm::sound::t5::head(&head) {
                    // Black ops states all of this in its own header, so the
                    // row is exact without decoding a sample.
                    listing.rate = t5.rate;
                    listing.channels = t5.channels;
                    listing.frames = t5.frames;
                    listing.codec = Codec::Adpcm;
                } else if let Some((rate, channels, bitrate)) = decode::mp3_shape(&head) {
                    // An mp3's length is its size over its bit rate: near enough
                    // for a row, and it costs one read instead of a decode.
                    listing.rate = rate;
                    listing.channels = channels;
                    listing.codec = Codec::Mp3;
                    if bitrate > 0 {
                        let seconds = entry.plain as f64 * 8.0 / bitrate as f64;
                        listing.frames = (seconds * rate as f64) as u64;
                    }
                }
            }
            out.push(listing);
        }
        out
    }

    fn read(&mut self, id: PackageId, index: usize) -> Result<Vec<u8>> {
        let member = *self
            .audio
            .get(id.0 as usize)
            .and_then(|members| members.get(index))
            .ok_or_else(|| anyhow!("no such member"))?;
        self.archives
            .get_mut(id.0 as usize)
            .ok_or_else(|| anyhow!("no such archive"))?
            .read(member)
    }

    pub fn head(&mut self, id: PackageId, index: usize, want: usize) -> Result<Vec<u8>> {
        let member = *self
            .audio
            .get(id.0 as usize)
            .and_then(|members| members.get(index))
            .ok_or_else(|| anyhow!("no such member"))?;
        self.archives
            .get_mut(id.0 as usize)
            .ok_or_else(|| anyhow!("no such archive"))?
            .read_head(member, want)
    }
}

fn codec_of(name: &str) -> Codec {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".mp3") {
        Codec::Mp3
    } else if lower.ends_with(".flac") {
        Codec::Flac
    } else if lower.ends_with(".ogg") {
        Codec::Opus
    } else {
        Codec::Pcm16
    }
}

// ----------------------------------------------------------------------------

#[derive(Default)]
pub struct SabSet {
    banks: Vec<sab::Bank>,
    pub info: Vec<PackageInfo>,
}

impl SabSet {
    pub fn mount(&mut self, path: &Path) -> Result<PackageId> {
        let bank = sab::open(path)?;
        Ok(self.take(path, bank))
    }

    /// A bank that is a blob in casc rather than a file: black ops 4 keeps
    /// its banks under `zone\snd\` the way black ops iii does, but inside
    /// battle.net's storage rather than on disk.
    pub fn mount_casc(
        &mut self,
        storage: std::sync::Arc<crate::hm::pack::casc::Storage>,
        file: &crate::hm::pack::casc::tvfs::File,
    ) -> Result<PackageId> {
        let path = storage.root().join(&file.path);
        let source = sab::Source::Casc {
            storage,
            spans: file.spans.clone(),
        };
        let bank = sab::open_source(source, file.size as u64)?;
        Ok(self.take(&path, bank))
    }

    fn take(&mut self, path: &Path, bank: sab::Bank) -> PackageId {
        let id = PackageId(self.banks.len() as u32);
        self.info.push(PackageInfo {
            id,
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
            path: path.to_path_buf(),
            entries: bank.entries.len(),
            version: bank.header.version as u16,
            kind: 0,
        });
        self.banks.push(bank);
        id
    }

    fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        let Some(bank) = self.banks.get(id.0 as usize) else {
            return Vec::new();
        };
        bank.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| Listing {
                index: index as u32,
                key: entry.key,
                keyed: true,
                name: entry.name.clone(),
                bytes: entry.size as u64,
                rate: entry.rate,
                channels: entry.channels.max(1),
                frames: entry.frames as u64,
                codec: sab_codec(entry.format),
            })
            .collect()
    }

    /// An entry's bytes, with the header the bank leaves off put back.
    ///
    /// Sab banks store their audio bare: flac starting at a frame, pcm with no
    /// wav header at all. Everything that header would say is in the bank's own
    /// table, so it is rebuilt here and the audio comes out as a file that
    /// plays anywhere, rather than a slice only harmony understands.
    fn read(&mut self, id: PackageId, index: usize) -> Result<Vec<u8>> {
        let bank = self
            .banks
            .get(id.0 as usize)
            .ok_or_else(|| anyhow!("no such bank"))?;
        let entry = bank
            .entries
            .get(index)
            .ok_or_else(|| anyhow!("no such entry"))?;
        // Read now, from the file: the bank is open, not held.
        let body = bank.bytes(entry.offset, entry.size as usize)?;
        let body = body.as_slice();
        let channels = entry.channels.max(1);

        Ok(match sab_codec(entry.format) {
            Codec::Flac => {
                let start = decode::first_flac_frame(body);
                let mut out = decode::flac_header(entry.rate, channels, entry.frames as u64);
                out.extend_from_slice(&body[start..]);
                out
            }
            Codec::Pcm16 => {
                let mut out = decode::wav_header(entry.rate, channels, 16, body.len() as u32);
                out.extend_from_slice(body);
                out
            }
            _ => body.to_vec(),
        })
    }
}

/// What a sab entry's format byte means, as Black Ops II writes it.
fn sab_codec(format: u8) -> Codec {
    match format {
        0 => Codec::Pcm16,
        4 => Codec::Xma,
        5 => Codec::Mp3,
        8 => Codec::Flac,
        _ => Codec::Unknown,
    }
}

// ----------------------------------------------------------------------------

/// The `soundfileN.pak` containers Ghosts and Advanced Warfare stream from.
/// They have no table harmony can read yet, so the streams are found by their
/// own magic and carved out whole. That gives playable, extractable audio with
/// no names, which is better than nothing and honest about what it is.
#[derive(Default)]
pub struct PakSet {
    paks: Vec<Pak>,
    pub info: Vec<PackageInfo>,
}

struct Pak {
    path: std::path::PathBuf,
    spans: Vec<Span>,
}

#[derive(Clone, Copy)]
struct Span {
    at: u64,
    len: u64,
    rate: u32,
    channels: u8,
    frames: u64,
}

impl PakSet {
    pub fn mount(&mut self, path: &Path) -> Result<PackageId> {
        let spans = carve(path)?;
        let id = PackageId(self.paks.len() as u32);
        self.info.push(PackageInfo {
            id,
            name: path
                .file_stem()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
            path: path.to_path_buf(),
            entries: spans.len(),
            version: 0,
            kind: 0,
        });
        self.paks.push(Pak {
            path: path.to_path_buf(),
            spans,
        });
        Ok(id)
    }

    fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        let Some(pak) = self.paks.get(id.0 as usize) else {
            return Vec::new();
        };
        let stem = pak.path.to_string_lossy().to_string();
        pak.spans
            .iter()
            .enumerate()
            .map(|(index, span)| Listing {
                index: index as u32,
                key: crate::hm::catalog::hash::fnv1a64(&format!("{stem}:{:x}", span.at)),
                keyed: false,
                name: None,
                bytes: span.len,
                rate: span.rate,
                channels: span.channels,
                frames: span.frames,
                codec: Codec::Flac,
            })
            .collect()
    }

    fn read(&mut self, id: PackageId, index: usize) -> Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let pak = self
            .paks
            .get(id.0 as usize)
            .ok_or_else(|| anyhow!("no such pak"))?;
        let span = *pak
            .spans
            .get(index)
            .ok_or_else(|| anyhow!("no such stream"))?;
        let mut file = std::fs::File::open(&pak.path)?;
        file.seek(SeekFrom::Start(span.at))?;
        let mut out = vec![0u8; span.len as usize];
        file.read_exact(&mut out)?;
        Ok(out)
    }
}

/// Modern warfare 3's fastfiles.
///
/// One package per `.ff`. Mounting a zone means inflating it once and keeping
/// only what was found — name, offset, size and shape — because the zones are
/// a couple of hundred megabytes each and there are hundreds of them. Reading
/// a sound inflates that one zone again, and the last one stays in hand since
/// sounds are browsed a zone at a time.
pub struct FfSet {
    zones: Vec<Ff>,
    pub info: Vec<PackageInfo>,
    held: Option<(u32, std::sync::Arc<Vec<u8>>)>,
    /// The game whose zone index this set reads and writes, when it has one.
    game: Option<String>,
    /// What every zone of this game held last time, by the file it came from.
    known: std::collections::HashMap<std::path::PathBuf, crate::hm::storage::CachedZone>,
    /// Whether anything was inflated this time, and so worth writing back.
    learned: bool,
}

struct Ff {
    path: std::path::PathBuf,
    sounds: Vec<crate::hm::zone::iw5::Sound>,
}

impl Default for FfSet {
    fn default() -> FfSet {
        FfSet {
            zones: Vec::new(),
            info: Vec::new(),
            held: None,
            game: None,
            known: std::collections::HashMap::new(),
            learned: false,
        }
    }
}

impl FfSet {
    /// Read this game's zone index, so a zone that has not changed since the
    /// last run is not inflated again.
    pub fn remember(&mut self, game: &str) {
        self.known = crate::hm::storage::load_zones(game)
            .zones
            .into_iter()
            .map(|zone| (zone.file.path.clone(), zone))
            .collect();
        self.game = Some(game.to_string());
    }

    /// Write the index back, once everything is mounted.
    ///
    /// Only when something was actually inflated: a run that found every zone
    /// already indexed has nothing new to say.
    pub fn keep(&mut self) {
        let (Some(game), true) = (self.game.clone(), self.learned) else {
            return;
        };
        let zones: Vec<crate::hm::storage::CachedZone> = self
            .zones
            .iter()
            .filter_map(|zone| self.known.get(&zone.path).cloned())
            .collect();
        crate::hm::storage::save_zones(&game, &crate::hm::storage::CachedZones { zones });
        self.learned = false;
    }

    /// What a zone holds, out of the index where the file has not been touched
    /// since it was written, and out of the zone itself where it has.
    fn index(&mut self, path: &Path) -> Result<Vec<crate::hm::zone::iw5::Sound>> {
        let now = crate::hm::storage::stamp_of(path);
        if let (Some(now), Some(kept)) = (now.as_ref(), self.known.get(path))
            && kept.file == *now
        {
            return Ok(kept
                .sounds
                .iter()
                .map(|sound| crate::hm::zone::iw5::Sound {
                    name: sound.name.clone(),
                    at: sound.at as usize,
                    bytes: sound.bytes,
                    rate: sound.rate,
                    channels: sound.channels,
                    bits: sound.bits,
                })
                .collect());
        }
        let zone = crate::hm::zone::iw5::inflate(path)?;
        let sounds = crate::hm::zone::iw5::sounds(&zone);
        if let Some(now) = now {
            self.known.insert(
                path.to_path_buf(),
                crate::hm::storage::CachedZone {
                    file: now,
                    sounds: sounds
                        .iter()
                        .map(|sound| crate::hm::storage::CachedZoneSound {
                            name: sound.name.clone(),
                            at: sound.at as u64,
                            bytes: sound.bytes,
                            rate: sound.rate,
                            channels: sound.channels,
                            bits: sound.bits,
                        })
                        .collect(),
                },
            );
            self.learned = true;
        }
        Ok(sounds)
    }

    pub fn mount(&mut self, path: &Path) -> Result<PackageId> {
        let sounds = self.index(path)?;
        if sounds.is_empty() {
            return Err(anyhow!("no sounds in this fastfile"));
        }
        let id = PackageId(self.zones.len() as u32);
        self.info.push(PackageInfo {
            id,
            name: path
                .file_stem()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
            path: path.to_path_buf(),
            entries: sounds.len(),
            version: 0,
            kind: 0,
        });
        self.zones.push(Ff {
            path: path.to_path_buf(),
            sounds,
        });
        Ok(id)
    }

    fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        let Some(zone) = self.zones.get(id.0 as usize) else {
            return Vec::new();
        };
        zone.sounds
            .iter()
            .enumerate()
            .map(|(index, sound)| Listing {
                index: index as u32,
                key: crate::hm::catalog::hash::fnv1a64(&sound.name),
                keyed: true,
                name: Some(sound.name.clone()),
                bytes: sound.bytes as u64,
                rate: sound.rate,
                channels: sound.channels,
                frames: sound.frames(),
                codec: Codec::Pcm16,
            })
            .collect()
    }

    /// The zone's bytes, held in case the next sound comes from the same one.
    fn blob(&mut self, id: PackageId) -> Result<std::sync::Arc<Vec<u8>>> {
        if let Some((held, blob)) = &self.held
            && *held == id.0
        {
            return Ok(blob.clone());
        }
        let path = self
            .zones
            .get(id.0 as usize)
            .map(|zone| zone.path.clone())
            .ok_or_else(|| anyhow!("no such zone"))?;
        let blob = std::sync::Arc::new(crate::hm::zone::iw5::inflate(&path)?);
        self.held = Some((id.0, blob.clone()));
        Ok(blob)
    }

    /// The samples, with a wav header put in front of them: the zone stores
    /// them bare, and everything downstream of here expects a file.
    fn read(&mut self, id: PackageId, index: usize) -> Result<Vec<u8>> {
        let sound = self
            .zones
            .get(id.0 as usize)
            .and_then(|zone| zone.sounds.get(index))
            .cloned()
            .ok_or_else(|| anyhow!("no such sound"))?;
        let blob = self.blob(id)?;
        let end = (sound.at + sound.bytes as usize).min(blob.len());
        let body = blob
            .get(sound.at..end)
            .ok_or_else(|| anyhow!("this sound is past the end of its zone"))?;
        let mut out = decode::wav_header(sound.rate, sound.channels, sound.bits, body.len() as u32);
        out.extend_from_slice(body);
        Ok(out)
    }
}

// ----------------------------------------------------------------------------

/// An install that keeps audio in two kinds of container at once.
///
/// Modern Warfare 2 has named wavs in its `.iwd` archives and more of them
/// inside the fastfiles under `zone\`, and neither set is a copy of the
/// other: the archives hold what the game streams, the zones hold what it
/// loads with a level. Reading one and calling it the install is leaving half
/// the game's audio on the floor.
///
/// The two readers are unchanged. This puts one package list in front of them
/// and remembers where the archives end and the zones begin.
#[derive(Default)]
pub struct BothSet {
    iwd: IwdSet,
    ff: FfSet,
    /// The first package that belongs to the zones.
    split: u32,
    pub info: Vec<PackageInfo>,
}

impl BothSet {
    pub fn remember(&mut self, game: &str) {
        self.ff.remember(game);
    }

    pub fn keep(&mut self) {
        self.ff.keep();
    }

    /// Mount by what the file is. Anything else is not for this store.
    pub fn mount(&mut self, path: &Path) -> Result<PackageId> {
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "iwd" => self.iwd.mount(path),
            "ff" => self.ff.mount(path),
            other => Err(anyhow!("{other} is not a container this store reads")),
        }
    }

    /// Settle the package list, once everything is mounted.
    ///
    /// The two readers each number their packages from zero, so the zones are
    /// moved up past the archives and keep that numbering for good.
    pub fn seal(&mut self) {
        self.split = self.iwd.info.len() as u32;
        self.info = self.iwd.info.clone();
        for info in &self.ff.info {
            let mut moved = info.clone();
            moved.id = PackageId(info.id.0 + self.split);
            self.info.push(moved);
        }
    }

    /// Which reader a package belongs to, and what it calls it.
    fn route(&self, id: PackageId) -> (bool, PackageId) {
        match id.0 < self.split {
            true => (true, id),
            false => (false, PackageId(id.0 - self.split)),
        }
    }

    fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        match self.route(id) {
            (true, at) => self.iwd.listing(at),
            (false, at) => self.ff.listing(at),
        }
    }

    fn read(&mut self, id: PackageId, index: usize) -> Result<Vec<u8>> {
        match self.route(id) {
            (true, at) => self.iwd.read(at, index),
            (false, at) => self.ff.read(at, index),
        }
    }

    pub fn head(&mut self, id: PackageId, index: usize, want: usize) -> Result<Vec<u8>> {
        match self.route(id) {
            (true, at) => self.iwd.head(at, index, want),
            (false, at) => self.ff.read(at, index).map(|mut raw| {
                raw.truncate(want);
                raw
            }),
        }
    }
}

/// The sound paks of a battle.net install.
///
/// Same streams as ghosts and advanced warfare keep in `soundfile*.pak`, and
/// the same carve finds them; the difference is where the pak comes from. It
/// is not a file here but a blob inside casc, so it is decoded whole into
/// memory, carved, and the last one is kept in case the next sound comes from
/// the same place, which it usually does.
pub struct CascSet {
    storage: std::sync::Arc<crate::hm::pack::casc::Storage>,
    paks: Vec<CascPak>,
    pub info: Vec<PackageInfo>,
    /// The pak most recently read, held whole. One is enough: sounds are
    /// browsed a pak at a time.
    held: Option<(u32, std::sync::Arc<Vec<u8>>)>,
}

struct CascPak {
    name: String,
    ekey: Vec<u8>,
    spans: Vec<Span>,
    /// Whether the spans have been worked out yet. Carving means decoding the
    /// whole pak, so it waits until something asks what is in it.
    carved: bool,
}

impl CascSet {
    pub fn open(storage: std::sync::Arc<crate::hm::pack::casc::Storage>, stems: &[&str]) -> CascSet {
        let mut set = CascSet {
            storage,
            paks: Vec::new(),
            info: Vec::new(),
            held: None,
        };
        let wanted: Vec<(String, Vec<u8>)> = set
            .storage
            .files
            .iter()
            .filter(|file| file.path.ends_with(".pak"))
            .filter(|file| stems.is_empty() || stems.iter().any(|stem| file.path.contains(stem)))
            .map(|file| (file.path.clone(), file.ekey.clone()))
            .collect();
        for (name, ekey) in wanted {
            let id = PackageId(set.paks.len() as u32);
            set.info.push(PackageInfo {
                id,
                name: name.trim_end_matches(".pak").to_string(),
                path: set.storage.root().join(&name),
                entries: 0,
                version: 0,
                kind: 0,
            });
            set.paks.push(CascPak {
                name,
                ekey,
                spans: Vec::new(),
                carved: false,
            });
        }
        set
    }

    /// The pak's bytes, out of casc, held in case the next call wants the same
    /// one.
    fn blob(&mut self, id: PackageId) -> Result<std::sync::Arc<Vec<u8>>> {
        if let Some((held, blob)) = &self.held
            && *held == id.0
        {
            return Ok(blob.clone());
        }
        let pak = self
            .paks
            .get(id.0 as usize)
            .ok_or_else(|| anyhow!("no such pak"))?;
        let blob = std::sync::Arc::new(self.storage.read_key(&pak.ekey)?);
        self.held = Some((id.0, blob.clone()));
        Ok(blob)
    }

    fn listing(&mut self, id: PackageId) -> Vec<Listing> {
        if !self.paks.get(id.0 as usize).is_some_and(|pak| pak.carved) {
            let Ok(blob) = self.blob(id) else {
                return Vec::new();
            };
            let spans = carve_bytes(&blob);
            if let Some(pak) = self.paks.get_mut(id.0 as usize) {
                pak.spans = spans;
                pak.carved = true;
            }
            if let Some(info) = self.info.get_mut(id.0 as usize) {
                info.entries = self.paks[id.0 as usize].spans.len();
            }
        }
        let Some(pak) = self.paks.get(id.0 as usize) else {
            return Vec::new();
        };
        let stem = pak.name.clone();
        pak.spans
            .iter()
            .enumerate()
            .map(|(index, span)| Listing {
                index: index as u32,
                key: crate::hm::catalog::hash::fnv1a64(&format!("{stem}:{:x}", span.at)),
                keyed: false,
                name: None,
                bytes: span.len,
                rate: span.rate,
                channels: span.channels,
                frames: span.frames,
                codec: Codec::Flac,
            })
            .collect()
    }

    fn read(&mut self, id: PackageId, index: usize) -> Result<Vec<u8>> {
        if !self.paks.get(id.0 as usize).is_some_and(|pak| pak.carved) {
            let _ = self.listing(id);
        }
        let span = *self
            .paks
            .get(id.0 as usize)
            .and_then(|pak| pak.spans.get(index))
            .ok_or_else(|| anyhow!("no such stream"))?;
        let blob = self.blob(id)?;
        let at = span.at as usize;
        let end = (at + span.len as usize).min(blob.len());
        Ok(blob[at..end].to_vec())
    }
}

/// The same carve as a pak on disk, over bytes already in hand.
fn carve_bytes(raw: &[u8]) -> Vec<Span> {
    const MAGIC: &[u8; 4] = b"fLaC";
    const INFO: usize = 4 + 4 + 34;

    let mut starts: Vec<(u64, u32, u8, u64)> = Vec::new();
    let mut at = 0usize;
    while at + INFO <= raw.len() {
        if &raw[at..at + 4] == MAGIC && stream_info(&raw[at..at + INFO]) {
            let shape = decode::flac_shape(&raw[at..at + INFO]);
            let frames = decode::flac_frames(&raw[at..at + INFO]);
            let (rate, channels) = shape.unwrap_or((0, 0));
            starts.push((at as u64, rate, channels, frames));
            at += 4;
        } else {
            at += 1;
        }
    }

    let mut spans = Vec::with_capacity(starts.len());
    for (i, (start, rate, channels, frames)) in starts.iter().enumerate() {
        let end = starts
            .get(i + 1)
            .map(|(next, _, _, _)| *next)
            .unwrap_or(raw.len() as u64);
        spans.push(Span {
            at: *start,
            len: end - start,
            rate: *rate,
            channels: *channels,
            frames: *frames,
        });
    }
    spans
}

/// Is this really the start of a stream, or four bytes of audio that happen to
/// spell the magic? A real one is followed by a stream info block: block type
/// 0, last or not, and a length of 34.
fn stream_info(head: &[u8]) -> bool {
    head.len() >= 8 && (head[4] == 0x00 || head[4] == 0x80) && head[5..8] == [0, 0, 34]
}

/// Where each flac stream in a pak starts and how far it runs: to the next one,
/// or to the end of the file. A pak can be a gigabyte, so this walks it in
/// windows rather than holding it.
fn carve(path: &Path) -> Result<Vec<Span>> {
    use std::io::Read;

    const MAGIC: &[u8; 4] = b"fLaC";
    const WINDOW: usize = 8 << 20;
    /// A stream info block is 34 bytes after a four byte block header.
    const INFO: usize = 4 + 4 + 34;

    let mut file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut buffer = vec![0u8; WINDOW + INFO];
    let mut base = 0u64;
    let mut carry = 0usize;
    let mut starts: Vec<(u64, u32, u8, u64)> = Vec::new();

    loop {
        let read = file.read(&mut buffer[carry..])?;
        let filled = carry + read;
        if filled < INFO {
            break;
        }
        let last = filled - INFO;
        let mut at = 0usize;
        while at <= last {
            if &buffer[at..at + 4] == MAGIC && stream_info(&buffer[at..at + INFO]) {
                let shape = decode::flac_shape(&buffer[at..at + INFO]);
                let frames = decode::flac_frames(&buffer[at..at + INFO]);
                let (rate, channels) = shape.unwrap_or((0, 0));
                starts.push((base + at as u64, rate, channels, frames));
                at += 4;
            } else {
                at += 1;
            }
        }
        if read == 0 {
            break;
        }
        // Keep the tail so a header straddling the window is still seen whole.
        buffer.copy_within(filled - INFO..filled, 0);
        base += (filled - INFO) as u64;
        carry = INFO;
    }

    let mut spans = Vec::with_capacity(starts.len());
    for (i, (at, rate, channels, frames)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map(|next| next.0).unwrap_or(size);
        let len = end - at;
        if len > 256 {
            spans.push(Span {
                at: *at,
                len,
                rate: *rate,
                channels: *channels,
                frames: *frames,
            });
        }
    }
    Ok(spans)
}

