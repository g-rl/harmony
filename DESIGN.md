# harmony

An audio asset explorer for Call of Duty, in Rust. `eframe`/`egui`, dark, monospace, flat —
the same window lydia has. Extraction is one of the things it does, not what it is: the
identity is *explorer*, so waveform analysis, relationship graphs, collections, cross-game
comparison and batch export all fit without the app outgrowing its own name.

The icon and the wordmark are `images/chinchou.png`.

**Every string in the ui is lowercase.** Headings, buttons, column titles, game names,
status lines, menu items, tooltips. `mwiii`, not `MWIII`; `weapons`, not `Weapons`. The
only exceptions are values that come out of the game data itself and a hash, which is
printed as it is.

---

## what was verified before this was written

Nothing below is guessed. Each line was measured against
`D:\Steam\steamapps\common\Call of Duty Modern Warfare III` with a working Rust prototype.

| fact | evidence |
| --- | --- |
| containers are `KAPI` (`0x4950414B`) packages, header version `23` | header of every `.xpak` / `.xsub` in `zone/`, `cod23/`, `xpak_cache/` |
| `Type` at `0x10`: `1` = cache (data in a sibling `.xpakdata`), `2` = metadata only, `3` = data | `xpak_cache/*.xpak` is 1, `zone/*.xpak` is 2, every `.xsub` is 3 |
| entry table is `HashCount` x 20 bytes at `HashOffset` | `4771 * 20 = 95420 = HashSize`; `54097 * 20 = 1081940` |
| entry = `{ u64 key, u64 packed, u32 packed_ex }`, `offset = (packed >> 32) << 7`, `size = (packed >> 1) & 0x3FFFFFFF` | every entry in both files resolved to a decodable block chain |
| blocks are Oodle (`6`), LZ4 (`3`) or stored (`0`), 21-byte descriptors, count at `block + 22` | 58 868 entries extracted, no failures |
| Oodle is the game's own `oo2core_8_win64.dll` in the install root | `LoadLibrary` + `OodleLZ_Decompress` |
| sound payload = `[32-byte header][4 bytes per packet][u16 len + opus packet]...` | seek table size exactly `32 + 4n` on every sound sampled |
| sounds come in **two shapes**: packed ones carry the 32-byte header, stored ones open straight into the seek table at offset 0 | `eng_codhq-00000.xsub`: 147 packed, 40 708 stored; both decode. Stored entries are the ones whose key has the top bit set |
| a sound can be identified from its first 2 KB | the seek table and first packets sit at the front, so `probe_head` reads 2 KB instead of the whole blob: 40 854 entries classified in 0.5 s instead of 55 s |
| packages must be read in **offset order** | walking the hash table in key order took 78 s for the localised packages; walking them in disk order takes 1.5 s |
| opus is 48 kHz, 20 ms, CELT fullband, mostly mono | decoded with a pure-Rust decoder; a 3.56 s clip read -18.3 dB mean, -0.0 dB peak |
| language packages are pure audio | `zone/eng_codhq_shared-00000.xsub`: 1521 entries, ~100 % opus |
| MWIII ships no `SAB` banks | full scan of 58 868 entries: zero `2UX#` magics |
| **MWIII fastfiles decompress offline with no keys** | `IWffa100`, header `0x18`, scan for `0x43574902`, skip `0x8000` when the next word is `IWff`, then 12-byte block headers. 130 MB / 5379 blocks reproduced the header's size exactly |
| the zone's asset list parses | `mp_jup_arcade.ff`: 41 043 / 41 043 records, `{ u64 type, u64 ptr }`, sorted by type |
| **MWIII fastfiles contain sound assets** | type `0xC1` (`sndasset`): 1366 in `mp_jup_arcade.ff`, 2004 in `mp_jup_benchmark.ff` |
| the in-memory `SndAsset` layout is not the zone layout | no record in the decompressed zone matches it; the zone stores a frame-rate index, not `48000` |

So: audio, duration and channel counts are solved. Names are not, and the last row says
exactly why. That is what the tiers below are for.

### the older engines, measured the same way

Each line below was read off the installs named in the table, with the same prototype.

| fact | evidence |
| --- | --- |
| World at War and Modern Warfare 2 keep named audio in `main\iw_NN.iwd`, which are plain zip archives | 35 archives / 17 080 sounds in WaW, 43 / 18 549 in MW2, all under `sound/...` paths |
| WaW stores its effects as **Microsoft ADPCM** wavs (`wFormatTag` 2), mostly stored, not deflated | `iw_14`..`iw_19` sampled: format tag `0200` on all but a handful of tag `0100` |
| MW2 stores 16-bit PCM wavs with a `bext` chunk in front of `fmt `, and music as MP3, all deflated | `elm_airport_jet_interior1.wav`: `bext` 0x25A bytes, then `fmt ` 1 / 2ch / 44100 / 16; `sound/music/*.mp3`, 55 of them in `iw_19` |
| a deflated member's header can be read without inflating the member | the head read stops after 8 KB of output: 18 549 entries described in 32 s instead of inflating ~10 GB |
| Black Ops II banks are `2UX#` version `0x0E`, 20-byte entries, and carry **no names** | `cmn_root.all.sabl`: entry size 0x14, 1862 entries, name table absent — Sound Studio gets its names from a list it ships, not from the bank |
| sab entry formats are `0` PCM16, `4` XMA4, `5` MP3, `8` FLAC | `Black-Ops-II-Sound-Studio/Format/AudioFormat.cs`, and both kinds decode: `.sabl` entries are PCM, `.sabs` entries are FLAC |
| sab audio is stored **headerless** | a `.sabs` entry begins at a flac frame; rebuilding a stream info block from the bank's own rate, channels and sample count makes it decode, and 3 000 entries came out with no failures |
| Ghosts streams flac from `zone\**\eng_soundfileN.pak`, one stream after another with no index | `eng_soundfile1.pak` opens with `fLaC`; 88 paks carve to 32 663 streams, durations 0.9–1.6 s where sampled |
| Advanced Warfare uses the same pak names | `zone\soundfileN.pak`, `zone\english\eng_soundfile48.pak`; in the install to hand every one of them is zero-filled, so the reader is written but unverified |
| MW2 Campaign Remastered has no container files on disk: everything is inside CASC | `D:\MW2 Campaign Remastered` holds `Data\data\*.idx` and `data.NNN`, and nothing that looks like a pak until the storage is opened |
| its local indices are version 7, and the entry table starts on the next 16-byte boundary after the header | `EncodedSizeLength=4, StorageOffsetLength=5, EncodedKeyLength=9, FileOffsetBits=30`; reading the table at `8 + head_size` misses every key, at `(8 + head_size).div_ceil(16) * 16` it finds them all |
| the encoding table's espec block length is at byte 18, not 17 | read at 17 the page index lands mid-string and the root ckey is "not listed"; read at 18 the root resolves first try |
| the root is TVFS, and 546 of its files are sound paks | magic `TVFS`, path table at 0x0C, vfs at 0x14, cft at 0x1C; `soundfile*.pak` carves to 141,329 flac streams in 177.3 s |
| Modern Warfare 3 keeps no audio in its archives at all | 29 `.iwd` in `main\`, not one `sound/` member; all 311 `.ff` under `zone\` hold it instead |
| an iw5 fastfile is a 21-byte header and one zlib stream | `IWffu100`, version 1, then nine bytes, then `78 da`; `common.ff` inflates to 186,051,375 bytes |
| the dlc fastfiles are signed and their bodies are encrypted | `IWff0100` carries a second magic, `IWffs100`, and high-entropy bytes after it; harmony refuses them and says so, because it ships no keys |
| a loaded sound in an inflated zone is a wave format block, a marker pair, the name, then the samples | format 1 / channels / rate / bytes a second / block align / 16 bits, five empty words, `FE FF FF FF FF FF FF FF`, the name, then `size` bytes of pcm; 82 zones carry 14,607 named sounds |
| Black Ops keeps named audio in `main\iw_NN.iwd` like world at war, but the members are not riff files | 88 archives / 75,765 sounds, every name under `sound/...`; the first four bytes are `01 00 00 00`, not `RIFF` |
| a black ops sound is a 64-byte header then ms-adpcm at 262 bytes a block per channel | frame count at byte 4, rate at 8, channels at 12, sample offset at 16, channel mask at 32, loop flag at 36; file size is `frames / 512 * 262 * channels` to the byte across the whole install |
| the first byte of every block is a microsoft coefficient index | it stays inside 0..=6, which is the whole of the table, and decoding on that assumption gives 0.77 s of monkey at rms 5,912 with nothing near full scale, where a wrong guess clips constantly |
| Modern Warfare Remastered keeps the same flac paks as ghosts, but beside the executable rather than in `zone\` | `soundfile1.pak` opens with `fLaC` in the install root; `imagefile*.pak` beside it opens `S1ffu100` and is not audio |
| its localised audio is one folder per installed language | `english\eng_soundfileN.pak`, 29 of them; searching the root two levels deep finds every language installed without naming any of them |
| the whole install reads as 49 paks / 20,244 sounds | mounted in 61.0 s; a pulled stream is `RIFF`/`WAVE`, pcm16 mono 22050 |
| some blocks are BLTE mode `E` | those refuse with "this block is encrypted, and harmony ships no keys for it" — harmony ships no keys and does not guess them |

---

## the three sources

Not alternatives — they stack, and the catalog merges them.

### tier A — packages (`.xpak` / `.xsub`, `.ipak`, loose `.sabs`)

No game running, no injection, no keys. Entry table, blob by key, decompress, sniff. Every
sound in the game with correct audio, correct duration, correct channels. No names for
MWIII; full names for any title that ships SAB banks, because those carry a name table.

Done and proven. It is what ships first.

### tier B — fastfiles (`.ff`)

The container half is **done**: `zone/xfile.rs` decompresses MWIII fastfiles offline and the
asset list parses, so the app can already say *this zone holds 1366 sound assets*. The open
half is the `sndasset` record layout inside the zone stream, which turns those counts into
`{ name_hash, stream_key, frames, rate, channels }` and lights up names, categories and the
relationship views.

Until it lands, tier B still earns its place: the per-zone asset inventory is what tells
the browser which map, weapon or mode a package belongs to.

### tier C — a running game (Parasyte / Cordycep)

Reads asset pools out of a patched process. Not a goal. `Mount` is shaped so it could be
added without touching the catalog or the ui.

---

## games

### the selector

A dedicated strip across the top of the window, before anything else. One row per known
title, plus whatever was detected. Selecting one switches the whole app.

Tabs carry the name players use, not the marketing one: a strip of full titles is a strip
of nothing but the word *warfare*.

```
 bo7   bo6   mwiii   mwii   mw19   aw   bo2   ghosts   mw2   waw     + detected
```

Hovering one says the full name and how far harmony's support for it goes.

A tab is a switch, not a label. Each folder harmony is pointed at is written into
`settings.roots` under its title id, so a tab with a folder behind it mounts it on one
click and the one open now is the one drawn lit; a tab with nothing behind it opens the
folder picker instead, and right-clicking any tab repoints it. Nothing has to be dropped
twice.

Under it, the card for whichever is selected:

```
 game          modern warfare iii
 id            jup  (cod23)
 path          d:\steam\steamapps\common\call of duty modern warfare iii
 sounds in     zone\  cod23\           .xsub, .xpak
 support       audio yes  names no     tier a
 formats       opus 48k
 discovered    58 812
 extracted     4 281
 last scan     2026-09-17 16:38
 build         1.62.3.0  (from bootstrap.data.bin)
```

Every one of those lines is state, so every one earns its place. Nothing on that card
explains what a word means. One more line joins them once a folder is open — where the scan
is being cached, and what is free there — because a catalogue of a big install is tens of
megabytes, and that is worth knowing before the disk says so.

A tab is a switch, and a double-click on the tab already open is how a game is moved: it
asks for the folder again, the same as right-clicking it.

### detection, not selection

The app recognises a game from its files. Dropping a folder on the window, or picking one,
runs every registered fingerprint and scores them:

```rust
pub struct Fingerprint {
    pub title: TitleId,
    pub score: u8,          // 0..100
    pub roots: Vec<PathBuf>,// the folders that actually hold audio
    pub reason: String,     // "cod23/ + 110 kapi v23 packages"
}
```

A title answers with a score, the folders it wants, and the one-line reason the card shows.
The highest score wins outright; a tie asks, and only then. A folder nothing recognises
becomes `unknown / unsupported`, which is a real entry rather than an error — it still
lists what it found, so a new title shows up as *93 kapi packages, header version 26,
no reader* instead of silence.

### what each title needs, said plainly

The card names the folders the app reads for that title, because a user who points at the
wrong directory should be told which one is right rather than shown an empty list:

| tab | title | id | audio lives in | containers |
| --- | --- | --- | --- | --- |
| mwiii | modern warfare iii | `jup` | `zone\`, `cod23\` | `.xsub`, `.xpak` |
| mwii | modern warfare ii | `iw9` | `zone\`, `sp22\`, `mp22\` | `.xsub`, `.xpak` |
| mw19 | modern warfare 2019 | `iw8` | `zone\` | `.xsub`, `.xpak` |
| bo6 | black ops 6 | `t10` | `zone\` | `.xsub`, `.xpak` |
| bo7 | black ops 7 | `t11` | `zone\` | `.xsub`, `.xpak` |
| — | warzone | shares | the installed host title | `.xsub`, `.xpak` |
| bo2 | black ops ii | `t6` | `sound\` | `.sabs`, `.sabl` |
| bo1 | black ops | `t5` | `main\`, `zone\` | `.iwd` |
| aw | advanced warfare | `s1` | `zone\**` | `soundfile*.pak` |
| mw3 | modern warfare 3 | `iw5` | `zone\`, `zone\<language>\` | `.ff` |
| ghosts | ghosts | `iw6` | `zone\**` | `soundfile*.pak` |
| mwr | modern warfare remastered | `h1` | install folder, `<language>\` | `soundfile*.pak` |
| mw2cr | mw2 campaign remastered | `h2` | `Data\` | casc archives, `soundfile*.pak` inside them |
| mw2 | modern warfare 2 | `iw4` | `main\`, `zone\` | `.iwd` |
| waw | world at war | `t4` | `main\`, `zone\` | `.iwd` |

Two installs of different games can look alike — World at War and Modern Warfare 2 both
hold `main\iw_NN.iwd` — so a legacy fingerprint also looks for the game's own binaries in
the root, and says in its reason which of the two decided it.

Warzone is not a separate install — it rides on whichever title is present, so it is shown
as a filter over that title rather than a mount of its own.

### support status is honest

Each title carries its own status, and the card prints it rather than implying it:

| status | means |
| --- | --- |
| `verified` | read end to end against a real install |
| `expected` | same container and codec as a verified title, untested here |
| `partial` | audio comes out, names or categories do not |
| `detect only` | recognised, inventoried, not read |

MWIII is `verified`. MWII is `expected`. BO6 and BO7 are `detect only` until someone runs
them through. Modern Warfare Remastered and MW2 Campaign
Remastered are `partial`: the streams come out, the names do not, because the sound paks carry no index inside the storage either. The ui never
claims more than that.

---

## layout

One crate, modules under `src/hm/`, the way lydia keeps its own under `src/ly/`.

```
src/hm/
  app.rs            the eframe App: state, frame, keyboard
  ui/
    theme.rs        the palette, carried forward from lydia
    games.rs        the selector strip and the game card
    browser.rs      the seven views
    detail.rs       the selected sound
    lab.rs          waveform, spectrogram, spectrum, meters, transients
    compare.rs      a/b, sum, difference
    queue.rs        the extraction queue
    collections.rs  collections, favorites, tags
    search.rs       the query field and its parser's feedback
    status.rs       mount, scan and export progress
    widgets.rs      chips, rows, meters
  game/
    mod.rs          Title, TitleId, the registry, fingerprinting
    jup.rs iw9.rs t10.rs t11.rs t6.rs
  pack/
    mod.rs kapi.rs ipak.rs sab.rs oodle.rs
  zone/
    mod.rs xfile.rs assets.rs jup.rs
  sound/
    mod.rs opus.rs pcm.rs flac.rs adpcm.rs decode.rs
  catalog/
    mod.rs category.rs mode.rs names.rs hash.rs group.rs relations.rs
  query/
    mod.rs parse.rs eval.rs
  analysis/
    mod.rs peaks.rs spectrum.rs loudness.rs transient.rs similarity.rs fingerprint.rs
  export/
    mod.rs queue.rs wav.rs flac.rs ogg.rs raw.rs layout.rs manifest.rs preset.rs
  player/
    mod.rs transport.rs
  storage/
    mod.rs settings.rs scans.rs collections.rs tags.rs
```

---

## the title abstraction

Adding a game is one file and one registry line.

```rust
pub trait Title: Send + Sync {
    fn id(&self) -> TitleId;
    fn label(&self) -> &'static str;          // lowercase, as shown
    fn status(&self) -> Support;

    fn fingerprint(root: &Path) -> Option<Fingerprint> where Self: Sized;
    fn sound_roots(&self) -> &'static [&'static str];
    fn containers(&self) -> &'static [&'static str];

    fn mount(&self, root: &Path, report: &mut dyn Progress) -> Result<Mount>;
    fn discover(&self, mount: &Mount, sink: &mut dyn FnMut(SoundEntry)) -> Result<()>;
    fn build_id(&self, root: &Path) -> Option<String>;

    fn name_hash(&self) -> HashFn;
    fn categories(&self) -> &'static [CategoryRule];
}
```

```rust
pub struct Mount {
    pub title: TitleId,
    pub root: PathBuf,
    pub packages: PackageSet,     // every key in every container
    pub zones: Option<ZoneSet>,   // tier B inventories
    pub names: NameDb,
    pub oodle: Option<Oodle>,     // loaded from the install, never shipped
}
```

A title produces entries and turns one entry's bytes into samples. It never reaches into
the ui and never decides how something is exported.

---

## the entry

```rust
pub struct SoundEntry {
    pub id: SoundId,
    pub name: Name,               // Resolved(String) | Hash(u64) | Anonymous
    pub source: Source,           // Stream { key } | Bank { bank, index } | Loaded
    pub codec: Codec,
    pub rate: u32,
    pub channels: u8,
    pub frames: u64,
    pub bytes: u64,
    pub origin: PackageId,
    pub language: Option<Lang>,
    pub category: Category,
    pub facets: Facets,           // weapon, character, map, sound type, when known
    pub flags: EntryFlags,
    pub tags: TagSet,             // the user's own
}
```

`frames` and `channels` come from tier B when it is up and from the opus stream itself when
it is not — the TOC byte carries the stereo bit and the frame size, and packets times 960
is the length. Tier A still shows a real duration.

---

## browsing

Seven views over one catalog. The catalog is a flat vector; a view is an index over it, so
switching never rescans.

| view | what it is for |
| --- | --- |
| `grid` | tiles with a small waveform. skimming a category |
| `compact` | one line per sound, name only. thousands at a time |
| `detailed` | the full table: name, kind, ch, rate, length, format, size, package |
| `waveform` | a row per sound, drawn full width. finding the one that looks right |
| `tree` | the folder or inferred hierarchy |
| `recent` | what was extracted, newest first |
| `favorites` | starred, across every game |

A sound's detail panel:

```
 weapon/akimbo/ar/shot/fire_03
 ─────────────────────────────
 [ waveform ]
 length      0.82s
 channels    stereo
 rate        48 khz
 format      opus
 category    weapon
 game        mwiii
 package     eng_codhq_shared-00000
```

---

## search

One field, and it searches everything: name, internal path, category, game, weapon,
character, map, sound type, format, duration, rate, channels, tags.

Bare words match name and path. Prefixed terms match a field. Terms combine with an implied
and; `|` is or, `-` negates.

```
weapon:ar
game:mwiii
type:reload
length:<2
channels:stereo
game:mwiii weapon:smg type:fire
-type:foley  rate:48000  tag:punchy
```

```rust
pub enum Term {
    Text(String),
    Field { key: Key, op: Op, value: Value },
    Not(Box<Term>),
    Any(Vec<Term>),
}
```

Keys: `name`, `path`, `game`, `category`, `type`, `weapon`, `character`, `map`, `format`,
`length`, `rate`, `channels`, `package`, `language`, `tag`, `collection`, `named`, `mode`.
Operators: `:`, `<`, `>`, `<=`, `>=`, `..` for ranges. An unparseable query never wipes the
list — the field turns and the last good result stays.

Facets (`weapon:`, `character:`, `map:`, `type:`) are inferred from the name and path when
a name exists, and from the package and zone inventory when it does not.

### mode

`mode:` answers the question a player actually asks — is this campaign, multiplayer or
zombies. Nothing in a container says so, so it is read out of the name and the package the
sound sits in: `campaign`, `multiplayer`, `zombies`, `spec ops`, `warzone`, or `shared`
when no marker appears.

The markers are matched two ways, and the difference matters. Short ones (`mp`, `sp`, `zm`,
`wz`, `so`) are matched as whole tokens, split on `/`, `_`, `-` and `.`, so `mp_crash`
is multiplayer and `amp` is not. Long ones (`zombie`, `nazi_zombi`, `warzone`, `specops`)
are matched as substrings, because they are unambiguous wherever they land. A sound with no
marker in either place is `shared` rather than guessed at: a wrong mode is worse than none.

Grouping by mode builds the same buckets in the tree, so the question can be browsed as
well as searched.

### a zone is read once, ever

Opening Modern Warfare 3 meant inflating three hundred and eleven fastfiles: four minutes
and twenty seconds before a sound could be played. What comes out of that is small — a
name, an offset, a size and a shape per sound — and it does not change until the game is
patched, so it is kept beside the scan caches as `zones-<game>.json` with each zone stamped
by the file it came from. A zone whose file is the same size and date as when it was read
is not inflated again.

| game | first open | after |
| --- | --- | --- |
| mw3 | 4m21s | 5.4s |
| mw2 | 2m50s | 9.6s |
| cod4 | 1m25s | 2.8s |

The index is a cache like any other: it moves when the cache folder moves, it counts
towards what the folder card says is held, and a patched zone simply reads again.

### a fastfile does not say where its stream starts

Every fastfile is `IWffu100`, a version, and one zlib stream. Call of Duty 4 and Black Ops
put the stream twelve bytes in, right after the version; Modern Warfare 2 and 3 put nine
more bytes in front of it. Rather than keep a table of versions, harmony looks for the
stream: a zlib header is two bytes that have to divide by thirty-one, and the offsets the
games actually use are tried first. A stream that inflates to the end wins outright — two
bytes of a header can pass for a zlib header and hand back a few kilobytes of nonsense,
so a partial inflate is only kept when nothing else works.

Inside, a sound is a wave format block, a pointer written as `-2` to say the name follows,
the name, then the samples. On the sixty-four bit games that pointer is eight bytes and
the block has five words of padding; on the thirty-two bit ones the pointer is four bytes
and the block is eleven words with the name pointer written at both ends. The short marker
is the front half of the long one, so the long shape is tried first at every marker and
the short one only where it fails. Both shapes are thrown out unless every field agrees
with every other: the alignment against the channels and the bit depth, the sample count
against the size, and the two name pointers against each other.

### a bank's version does not say what its rows look like

Black Ops III writes bank version fifteen with thirty-six byte rows; Black Ops II writes
fourteen and fifteen with twenty. Reading the one as the other gives offsets into the
middle of nowhere, so the row's own width settles it and the version alone never does. The
fields are the same either way — key, size, frames, offset, and four bytes at the end for
the rate index, the channel count, the loop flag and the codec — just spread further apart,
with a sixty-four bit offset where the older table had a thirty-two bit one.

Its banks are two folders down as well, under `zone\snd\all` and `zone\snd\en`, and the
same folder is what tells the kapi titles to leave the install alone: Black Ops III ships
`.xpak` files carrying the same header version as Modern Warfare 2019, so without that
check the 2019 tab claimed a Black Ops III folder outright.

### banks are opened, not read in

Black Ops II ships nine and a half gigabytes of sound banks and harmony mounts every one
of them. Holding each bank's bytes meant the whole install sat in memory for as long as
the tab was open — eight and a half gigabytes of it, which is what the task manager was
showing. A bank is now opened and its tables read, a few kilobytes apiece; the audio is
read from the file when a sound is actually played or written out. Infinite Warfare's
two hundred and sixty banks cost the same nothing.

The name table has two shapes and the header does not say which: names packed one after
another, and names in fixed-width slots padded with zeroes. Infinite Warfare writes a
forty-four character name every hundred and twenty-eight bytes and calls the stride
sixty-four, so the stride is not believed either — the names are read as runs of text
with the padding skipped, which is both shapes at once.

### what a sound is waiting on

Reading a sound out of an install that is still being opened waits on the container being
opened, not on the sound. The panel used to take its measurements away the moment the
selection moved and put them back when they arrived, which reads as a flash at every
keypress. It now keeps the block and says `still loading <container>..` in it, naming
whichever container the mount was on when the sound was clicked — a name that moves is
the difference between slow and stuck. The waveform says the same thing in the same words.

### a game put down is not forgotten

Clicking another tab used to mean reading a fifty megabyte cache back and opening two
hundred containers again, and clicking back meant doing both a second time. The last three
games are kept as they were: their rows, their packages and the install they were read
from. Nothing is read to fill it — only what was already in hand is kept — and a scan,
a forget or a new folder drops what it would make wrong.

### a key is not a name

Three things can stand where a name goes, and they are not the same thing. A name harmony
recovered is drawn as text. A key the container carried is drawn as `_f6a6b431ac13033b`
in the unnamed colour: it is an id, and a name list may yet answer for it. A sound in a
container with no ids at all is drawn as the place it sits, `soundfile12#00042`, because
the key it is filed under is one harmony worked out and there is nothing to look up.

Nothing is read out of any of them. A hash contains `ac130` often enough by accident, so
the bucket is worked out once, from a real name, and an unnamed sound has none.

Matching happens on a worker: the keys go out, names come back in batches of four thousand,
and each batch is put on as it arrives, so the list fills in rather than freezing. A pass
that found anything saves the cache, so the next run starts already named.

### the finer buckets, and `tree+`

Every rule also names a bucket — birds, glass, doors, efforts — which is simply
the rule that matched, written down. `tree+` is the browser view that uses them: mode,
then category, then bucket, then the folders in the name itself. The `+` is drawn in the
accent colour because it is the same tree with more levels in it, not a different one.

A folder is a whole word, so a short word that a folder can be — `dog`, `wind`, `rain`,
`ape` — is matched as a whole word before the substring rules run. `aml/dog/pain/pain_00`
is a dog in pain and not a soldier in pain, and only the folder says so; `glass_window`
stays destruction rather than becoming weather.

### category is first match, and voice is first

Category runs down an ordered rule list and takes the first marker that hits, so the order
is the decision. Voice is checked before everything else, because dialogue names carry
weapon words all the time: `battlechatter/.../order_action_suppress` is a line of speech
about a weapon, not a weapon sound, and a weapons rule reading `wpn`, `gun` or `reload`
before a voice rule reading `battlechatter` would take 13 374 of Modern Warfare 2's files
for the wrong bucket.

The language of the archive is not evidence of voice on the older engines. A localised
archive there holds the whole game, not its dialogue: Modern Warfare 2 keeps every asset in
`localized_english_iwNN`. The archive is recorded as the sound's language and kept out of
the categorising, which is decided from the name. On the kapi titles, where a localised
package really does hold dialogue and nothing else, the language still counts.

---

## grouping and relationships

Names in this engine are paths, and paths carry structure. The grouper reads it:

```
ak-47
├── fire
│   ├── close
│   ├── far
│   ├── indoor
│   └── suppressed
├── reload
│   ├── start
│   ├── magazine
│   └── end
├── equip
├── unequip
├── dry fire
└── inspect
```

An event gets its own view, because one gunshot is never one file:

```
 ak fire
 ─────────────────────────
 variation 01  02  03  04  05

 distance      near  medium  far
 environment   indoor  outdoor  underground
```

with a play-through that walks the group in order.

The relationship view is the same data as a graph: an event in the middle, every asset that
belongs to it around it, one click to any of them. Where the zone inventory gives real
references, those are used; where it does not, the inference from names is marked as
inference rather than presented as fact.

---

## analysis

Per sound, computed lazily and cached: peaks, spectrogram, rms, peak, lufs, spectrum,
stereo image, zero crossings, duration, silence.

Transients are detected and marked — attack, sustain, tail, silence — which is what makes
`trim silence`, `detect tail` and `split variations` possible on a gunshot or an impact.

### similar

A fingerprint per sound: duration, loudness, spectral centroid and rolloff, a coarse
frequency histogram, an envelope shape, plus the name and path distance. `find similar`
ranks the catalog against it. Given how many variations of one sound this engine ships,
this is the feature that finds the other thirty-six.

### discovery

Automatic buckets nobody has to build a query for: unreferenced, duplicate, near-duplicate,
very short, very long, silent, corrupt, unknown format, never extracted.

### comparison

Two or more sounds side by side, playback synchronised, with `a/b`, `a+b`, `difference`,
and both waveform and spectrogram comparison. This is also how game-to-game works:

```
 mwii  vs  mwiii

 ak      14      21
 reload   8      11
 fire    23      31
```

with the assets that appear in both marked, and the ones unique to one title marked too.

---

## extraction

### the queue is a subsystem

```
 ak_fire_01.wav    ████████████  100%
 ak_fire_02.wav    ████████░░░░   72%
 ak_reload_01.wav  waiting
 ak_reload_02.wav  waiting

 4 281 / 7 942
```

Pause, resume, cancel, retry failed, open output, error details, statistics. A failed entry
never kills the batch.

### batch is a selection, not a file

Whatever the current query matches can be extracted in one action — all weapon sounds, all
reloads, all footsteps, one weapon's whole tree, one game.

```
 2 481 selected

 [ extract ]

 ☑ preserve paths
 ☑ normalise names
 ☑ skip duplicates
 ☑ convert to wav
 ☑ write metadata
 ☑ write manifest
```

### formats

| format | what it is |
| --- | --- |
| `wav` | decoded 16-bit pcm. what everyone wants |
| `flac` | decoded, losslessly packed. half the size, same samples |
| `ogg` | the opus packets remuxed. bit-exact, no decode, fastest |
| `raw` | the blob as it sits in the package |

`ogg` matters: the game's audio *is* opus, so wrapping it costs nothing and loses nothing.
Right default for archiving; `wav` is the right default for a daw.

### layout, presets, manifests

```
{category}/{package}/{name}.{ext}
{language}/{category}/{name}.{ext}
{game}/{weapon}/{type}/{name}.{ext}
flat
```

A preset saves the categories, the format, the layout and the switches, and runs from one
button. Every extraction can write `manifest.json`: asset name, game, category, original
path, output path, format, rate, channels, duration, timestamp — so the extracted library
is usable by other tools without guessing.

---

## collections, favorites, tags

Collections are the user's own shelves and are independent of the game's categories. A
sound can sit in several without being copied.

```
 best gunshots
 explosions
 footsteps
 ui
 character
 weird / unused
 favorites
```

Tags are free text — `punchy`, `bass-heavy`, `mechanical`, `dark`, `metallic`, `short`,
`long-tail` — and are searchable with `tag:`.

Right-click on a sound:

```
 favorite
 tag
 rename
 copy path
 copy internal name
 find similar
 find related
 extract
 extract selected
 open location
```

---

## version tracking

A scan is stored, so scanning the same install again is a diff rather than a replacement:

```
 mwiii

 previous   2 481 204
 current    2 492 817

 +11 613   -2 104   ~4 281 changed
```

Which is what makes a game update legible.

---

## drag and drop

Sounds drag into collections, the queue, a comparison slot and the export area. Dragging a
row out of the window hands the sound to anything else, the way lydia does it: an ole drag
with a `CF_HDROP` data object, in `src/hm/window/dragout.rs`.

The order matters, and it is the reason the card exists:

1. The row is pulled off the list. Nothing exists on disk yet, so nothing can be dragged
   yet: `CF_HDROP` carries paths.
2. A worker writes the selection flat into a scratch folder while the card says what is
   being carried and how far along it is.
3. The files land. The card repaints once more, now saying *carrying*.
4. On the **next** frame, before anything else, `DoDragDrop` takes the ui thread and keeps
   it until the pointer comes up. The window stops painting for the length of the gesture,
   which is what the card painted a frame ago is for.
5. A drop that went nowhere deletes what it wrote; a drop that landed leaves the files to
   the target. Anything left behind by a crash is swept on the next start.

`DoDragDrop` has to be called on the ui thread: ole reads the button state and the capture
off the calling thread, so a drag begun on a worker sees no button held and ends the
instant it starts.

---

## the window

Lydia's palette exactly: `#0e0e14` behind, `#16161e` panels, `#23232e` hairlines, teal
`#39d0d8` for waveforms, pink `#ff2e88` for the accent, monospace everywhere, 12 px, 2 px
corners, 20 px rows, flat widgets with no expansion. All of it lowercase.

```
+---------------------------------------------------------------------------+
| [chinchou]  mwii  mwiii  bo6  bo7  warzone  unknown      search...  scan   |
+-------------+-------------------------------------------------------------+
| games       |                     asset browser                           |
|  mwiii  ok  |   ak_fire_01    ak_fire_02    ak_fire_03                    |
|  mwii  --   |   ak_reload_01  ak_reload_02  ak_equip                      |
|             |                                                             |
| categories  |                                                             |
|  weapons    +-------------------------------------------------------------+
|  voice      |                     audio viewer                            |
|  footsteps  |   ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~     |
|  ui         |   >  <  ----------o------------------  0:01.28              |
|  explosions |   waveform  spectrogram  spectrum  meta                     |
+-------------+-------------------------------------------------------------+
| mounted 110 packages . 58 812 sounds . extracting 412/1204                 |
+---------------------------------------------------------------------------+
```

Words on controls are names, not explanations — `mount`, `scan`, `play`, `extract`,
`names`, `group`, `similar`, `compare` — one word where one word works, and no line
underneath saying what a control is for. What earns a line is state.

Keys: space plays, up and down move and audition, enter extracts the selection, `ctrl+f`
focuses search, `ctrl+a` selects the query, `f` favorites, escape stops.

### the frame is ours

The window is built with decorations off, so the app is one surface rather than a dark
panel hanging under a grey Windows bar. The tab strip is the title bar: the icon and the
word `harmony` on the left, the tabs beside them, and three flat glyph buttons pinned to
the far right of the same row — minimise `–`, maximise `□`, close `×`.

Everything the native frame did is put back by hand. Dragging empty space in that row moves
the window (`ViewportCommand::StartDrag`), double-clicking it toggles maximised, and an
invisible strip around all four sides plus the corners starts a resize
(`ViewportCommand::BeginResize`) with the cursor changing to match. The drag is suppressed
while the pointer is over a button, so a click on close is a close and not a throw, and the
resize edges are switched off entirely while a drag-out is in flight. The size the window is
left at is written to `settings.window_size` and restored on the next run.

Column headings in the list are painted, not laid out as widgets, using the same character
metrics as the rows and the list's own horizontal scroll offset, so `name`, `kind`, `ch`,
`rate` sit exactly above the first character of the column each one names, and stay there
when the list is scrolled sideways.

---

## the work is threaded, and the window is not

The window draws at sixty frames a second, which leaves sixteen milliseconds a frame.
Every piece of work harmony does is longer than that, so none of it happens there.

| work | where it runs | how the window learns |
| --- | --- | --- |
| fingerprinting a folder | a worker | a channel, one message |
| a scan | a worker, plus scoped threads per package | batches of entries as they are read |
| writing the cache | the scan's own relay thread | a message when it lands |
| reading a cache | a worker parses, the window builds entries 6 000 at a time | a slice a frame |
| name lists | a worker | the database arrives whole, and renames what is listed |
| extraction | the queue's worker | progress under a lock |
| free space | taken every five seconds, not every frame | kept on the state |

The relay is the piece worth explaining. A scan used to hand its entries to the window and
the window built the cache file at the end: fifty thousand rows turned into fifty megabytes
of json between two frames, which is exactly what the freeze looked like. Now everything
the walk produces passes through a relay thread on its way out. It forwards each batch
immediately, keeps a cache row for each one as it goes, and writes the file itself when the
walk finishes. A cancelled or failed scan writes nothing: a partial catalogue saved as if
it were whole is worse than no cache.

Searching pays the same attention. Every entry keeps its name folded to lowercase from the
moment it is made, because search reads it on every entry for every term of every
keystroke, and folding it there cost more than the search. The filtered list and the group
tree are rebuilt at most four times a second while entries are pouring in, and immediately
once they stop.

### refresh, rather than rescan

A scan of a big install is minutes, and most of the time nothing has changed. Refresh opens
the containers again and stats them against the list the cache was written from: path, size
and modified date. All the same, and the catalogue stands; anything new, missing, resized or
restamped, and it goes straight on to a scan. The mount it made is kept either way, so the
sounds are playable the moment it finishes.

That list is written by the relay, from the mount, at the same time as the rows — so a cache
knows what it was read from. A cache re-saved from the window has no mount behind it, so its
list is empty and the next refresh simply rescans, which is the safe way round.

### several games at once

A scan belongs to a game, not to the window, so there is one job per game and switching
tabs does not stop any of them. Only the game on screen keeps its rows: the others are
writing their own caches, and when their tab is opened the catalogue comes back from the
file. If a scan of the game on screen finishes with rows missing, because its tab was not
the one being watched while they went past, the window reads the cache it just wrote.

The bar along the bottom says what is happening in one line, in the words the window uses:
`scanning 3 games (jup, t6, iw4) Â· reading zones Â· 41 208 sounds`, `loading iw4 Â· 12 000 of
18 549`, `extracting 412 of 1 204`. Each tab carries a line under it while its own scan
runs.

### starting up

The window goes up before the work does. The last folder still has to be recognised, its
cache read and any name lists loaded, so all three start on threads and a veil stands over
the window meanwhile: the panels dimmed, `setting up harmony..`, and under it the same
activity line the bottom bar shows. It comes down when there is nothing left to wait for.

### the gpu, and doing without one

Harmony asks for wgpu first, which is vulkan or dx12 on this platform, and falls back to
opengl if that will not start. Neither is a hard requirement: the renderers are tried in
order and only the failure of the last one is an error the user sees.

---

## room to write

Three things harmony writes can be large: a scan cache, an extraction, and the scratch files
a drag out of the window needs. All three go where the user pointed them, which may not be
the system disk, so all three are settings (`cache_dir`, `output`, `temp_dir`) with
harmony's own defaults behind them. The settings file is not one of them: it stays where
harmony can always find it.

The cache folder is read by code that has no settings to hand — scanner threads, the
headless commands — so it is published once into a process-wide slot and read from there.
Moving it carries the caches already written across, and never overwrites a name already
taken at the far end: a cache is rebuildable, somebody else's file is not.

Room is measured before the work starts, not discovered during it:

| work | what is estimated | measured against |
| --- | --- | --- |
| cache a scan | 288 bytes an entry, from the 4.3 MB modern warfare 2's 18 549 sounds come to | the cache folder |
| extract | wav: frames × channels × 2 + header. ogg and raw: the packed size | the export folder |
| drag out | the same estimate, for the selection | the scratch folder |

On top of the estimate sits 256 MB of headroom, because Windows misbehaves on a disk with
nothing left and a truncated cache is worse than no cache. A volume that cannot be measured
is not a shortfall: the work goes ahead.

Too little room and the work does not start. A card says what was being attempted, where it
was writing, what it needs, what is free and what is missing, and offers the three honest
answers: try again, write somewhere else, or give it up. The scan itself is untouched — a
catalogue that could not be cached still browses, plays and extracts.

A disk can also fill up under a queue that is already running. Every sixteenth file, and
before any file over 8 MB, the queue measures again; if the room has gone it pauses itself
and says where it stopped, rather than failing every file that is left. What was written is
fine, and `carry on` resumes at the file it stopped at.

`harmony --room [folder]` prints the same measurement from the command line.

---

## order of work

1. **pack** — the kapi reader, the oodle loader, the block chain. *done.*
2. **sound/opus** — seek table, packet walk, decode, peaks. *done.*
3. **zone/xfile** — the mwiii fastfile reader and the asset list. *done.*
4. **game** — the registry, fingerprinting, the selector strip and the card.
5. **app + ui** — theme, browser, detail, transport, status. The first usable version.
6. **export** — the four writers, the layout template, the queue, manifests.
7. **query** — the parser, the evaluator, the facets.
8. **catalog** — names, categories, grouping, relationships.
9. **analysis** — peaks, spectrogram, meters, transients, similarity, discovery.
10. **collections** — favorites, tags, presets, version tracking, drag and drop.
11. **zone/jup** — the `sndasset` record layout. names stop being a guess.
12. **next titles** — mwii is the same container and codec; bo6 and bo7 are detect-only
    until run against an install; bo2 and bo3 are loose `.sabs` and `.sabl`, which the
    reference implementation in `refs/` already describes.

A milestone is not done until the whole pass runs against a real install, because every one
of the facts at the top of this file was wrong somewhere until it was checked.

---

## what this ships and what it does not

No game data, no name lists, no Oodle. `oo2core_8_win64.dll` is loaded out of the install
the user pointed at — the same copy the game already runs. Name lists are the user's to
bring. Nothing is uploaded anywhere.

---

## references

- `refs/Greyhound` — `XSUBCacheV3`, `GameModernWarfare6`, `SABSupport`.
- `refs/acts` — the offline iw fastfile decompressor, the `jup` asset type table
  (`sndasset` is `0xC1`), and the hash index.
- `refs/Black-Ops-II-Sound-Studio-master` — SAB reading and writing, headerless flac.
- [OpenAssetTools](https://github.com/Laupetin/OpenAssetTools) — how a per-title zone loader
  is structured when it is done properly.
