# harmony

an audio asset explorer for call of duty titles

![preview](images/preview.png)

## currently supported

| tab | game | id | status | where the sounds live | container |
| --- | --- | --- | --- | --- | --- |
| mw4 | modern warfare 4 | `rex` | partial | `cod26\` | kapi `.xpak` / `.xsub` |
| mwiii | modern warfare iii | `jup` | verified | `cod23\`, `zone\` | kapi `.xpak` / `.xsub` |
| mwii | modern warfare ii | `iw9` | expected | `zone\`, `sp22\`, `mp22\` | kapi |
| cw | black ops cold war | `t9` | partial | `zone\` | kapi `.xsub`, oodle blocks, opus inside |
| mw19 | modern warfare 2019 | `iw8` | partial | `zone\` | kapi |
| bo6 | black ops 6 | `t10` | detect only | `zone\` | kapi |
| bo7 | black ops 7 | `t11` | partial | `zone\` | kapi |
| bo4 | black ops 4 | `t8` | expected | `zone\snd\<language>\` in casc | battle.net casc, sab banks (`.sabs`, `.sabl`), flac |
| bo3 | black ops iii | `t7` | verified | `zone\snd\<language>\` | sab banks (`.sabs`, `.sabl`), flac |
| bo2 | black ops ii | `t6` | verified | `sound\` | sab banks (`.sabs`, `.sabl`) |
| bo1 | black ops | `t5` | verified | `main\` | `.iwd` archives, ms-adpcm |
| iw | infinite warfare | `iw7` | verified | install folder, `<language>\` | sab banks (`.sabs`, `.sabl`), flac |
| mw3 | modern warfare 3 | `iw5` | partial | `zone\`, `zone\<language>\` | `.ff` fastfiles, pcm inside |
| ghosts | ghosts | `iw6` | partial | `zone\**\soundfile*.pak` | flac streams |
| aw | advanced warfare | `s1` | partial | `zone\**\soundfile*.pak` | flac streams |
| wwii | wwii | `s2` | partial | install folder, `<language>\` | `soundfile*.pak`, flac streams |
| mwr | modern warfare remastered | `h1` | partial | install folder, `<language>\` | `soundfile*.pak`, flac streams |
| mw2cr | mw2 campaign remastered | `h2` | partial | `Data\data\` | battle.net casc, flac streams |
| mw2 | modern warfare 2 | `iw4` | verified | `main\`, `zone\` | `.iwd` archives and `.ff` fastfiles |
| waw | world at war | `t4` | verified | `main\`, `zone\` | `.iwd` archives |
| cod4 | call of duty 4 | `iw3` | verified | `main\`, `zone\` | `.iwd` archives and `.ff` fastfiles |

*verified* means harmony has read that install end to end. *expected* means the container
format matches and the same reader should apply. *partial* means the audio comes out but the
names do not, for one of two reasons. The older titles' sound paks have no index harmony can
read yet, so their streams are found by their own magic and carry no names at all. The newer
ones do carry names, hashed, and harmony ships no name lists to turn them back — so their
sounds play and extract under their keys until you point harmony at a list of your own.
Modern Warfare 2019, Cold War, Black Ops 7 and Modern Warfare 4 are all the second kind.
*detect only* means harmony recognises the install but does not yet claim to read its audio.
The Battle.net copy of Modern Warfare 2 Campaign Remastered is the odd one out: there are
no container files on disk at all, only CASC archives, so harmony reads the storage itself
— the local indices, the encoding table and the TVFS root — to find the sound paks inside
it, then carves the same flac streams out of them as it does for ghosts and advanced
warfare. 546 sound
packages, 141,329 sounds. Blocks that are encrypted stay encrypted: harmony ships no keys.
Call of Duty 4 and Modern Warfare 2 keep audio in two places at once: the streamed sounds
sit in the `.iwd` archives, and the sounds a level loads with it sit inside the fastfiles
under `zone`. Neither set is a copy of the other, so harmony reads both — 21,974 sounds
for Call of Duty 4 against 17,112 from the archives alone, and 26,380 for Modern Warfare 2
against 18,923.

Modern Warfare 2 and Modern Warfare 3 were both re-released as 64-bit executables years
after the fact, with the same containers inside. Harmony reads either, and says which one
is installed: the build line on the folder card reads `32-bit` or `64-bit`, taken from the
game's own binary rather than guessed from the folder.

The window says the same
thing in the left card, for whatever folder is open, and marks the folder that is open
right now with a ▸.

The tabs along the top switch games. Every folder harmony has been pointed at is remembered
per title, so a tab that has a folder behind it opens straight into it — clicking it is the
whole gesture. A tab with no folder yet asks for one, and double-clicking the tab that is
already open — or right-clicking any tab — points it somewhere else.

The older titles come with names, because they store paths: `sound/sfx/...` inside the
`.iwd` archives. Black Ops keeps the same archives but wraps its audio in a header of its
own rather than a riff one, so harmony reads that header and decodes the ms-adpcm behind
it. Black Ops II and everything newer store hashes instead, so those stay as
keys until a name list is loaded.

Codecs read: Opus (the modern titles), FLAC, MP3, PCM 16 and 24 bit, and Microsoft and IMA
ADPCM, which is what World at War keeps most of its effects in. Sab banks store their audio
with the header stripped off; harmony rebuilds it from the bank's own table, so what comes
out is a file that plays anywhere.

## using it

Run the binary and drop a game folder on the window, or press **folder** and pick one.
Harmony fingerprints the folder, says what it found and how it knows, and then:

- **refresh** stats the containers against the ones the cached scan was read from. Nothing
  moved, and it says so and leaves the catalogue alone; something did, and it rescans. The
  check runs on a worker, so the window keeps drawing either way.
- **scan** walks the packages. On the kapi titles nothing in a package says what it holds,
  so the depth matters: **quick** reads the localised packages only, which hold the
  dialogue and nothing else — a quick scan that comes back all voice is not a bug, it is
  what quick covers; **deep** samples every package and reads the ones with audio in them
  (minutes, and this is the one that finds weapons, music and ambience); **full** reads
  every entry of every package. The older titles carry a table of their own, so the whole
  catalogue arrives in seconds whatever the depth says.
- Results stream in while the scan runs, and the scan writes its own cache as it goes, so
  opening the same install again brings the catalogue back at once.
- Scans of different games run at the same time. Switching tabs does not stop one: the tab
  of a game being scanned carries a progress line, and the bar along the bottom says what
  is happening — `scanning 3 games (jup, t6, iw4) · reading zones · 41 208 sounds`. Only
  the game on screen keeps its rows in memory; the others come back from their caches.
- Click a sound to hear it. Space plays and pauses, the arrows walk the list, `f`
  favorites, escape stops.
- The search box takes plain words and `key:value` terms: `length:<2`, `channels:2`,
  `package:eng_codhq`, `language:english`, `named:no`, `tag:keep`, `fav:yes`, and
  `mode:zombies`. A leading `-` negates, `|` is or.
- **mode** is worked out from the name and the package the sound sits in, by whole tokens
  rather than substrings, so `mp_` in `sound/mp_crash/...` counts and `amp` does not. The
  values are `campaign`, `multiplayer`, `zombies`, `spec ops`, `warzone` and `shared` —
  shared being everything that carries no marker either way. Grouping by **mode** builds the
  same buckets in the tree, next to category, package and language.
- Right-click a sound for play, extract, drag out, favorite, tags, and to select the rest
  of its group. Ctrl-click adds to the selection, shift-click takes a run. Every view works
  the same way: the right-click menu and the drag out of the window are on the rows, the
  tiles, the waveforms, the recents and both trees, because a sound found by browsing is no
  different from a sound found by searching.
- A favorited sound carries a small star in front of its name in the compact list, the tree
  and `tree+`. It breathes rather than sits still, each one on its own phase, so a
  favourite is something the eye finds while scrolling a hundred thousand names without
  anything shouting about it. The detailed view leaves it off: its columns are fixed width,
  and a star in front of the name would push every one of them out of line.
- **extract** sends the selection (or everything shown) to the queue, which can be paused,
  cancelled and retried, and writes `manifest.json` alongside the audio when asked.
- **extract library** takes the whole game, filter or no filter, into a folder of its own:
  `[t7] black ops iii - 1.0.0.2`, named for the id, the game and the build it came out of,
  so two patches of the same install never land on top of each other. A run that size is
  written by every core the machine can spare bar two, and the queue window stops listing a
  row per sound: it shows how fast it is going, how much has landed, how long is left, the
  last few names and the failures, which is what a run of a hundred thousand is actually
  watched by. It leaves a `liblog.txt` in the folder saying what it was told to do, what it
  left out, everything that failed and why, and how long it all took — rewritten as it
  goes, so a run that is killed outright still leaves a readable log.
- **leave out** ticks whole buckets off a library run: a dump of everything that skips the
  voice folder is still a dump, and it should not take a search term to get one. The chips
  are the buckets this catalogue actually holds, with their counts, and what is ticked is
  remembered between runs. `voice` is matched both ways — the bucket harmony filed a sound
  under, and the first folder of the sound's own name — because on some titles it is one
  and on some it is the other. Only the library run leaves anything out; `extract shown`
  writes exactly what is on screen.
- **a second export while one is running** joins the line rather than replacing it, and so
  does a third and a tenth: the line takes as many as it is given. The run that is going is
  never interrupted by a click somewhere else. Each run carries its own mount, so the line
  can hold two different games at once. From the queue window any waiting run can be moved
  to the front, taken out, or the whole line held so that nothing new starts when the one
  going ends; **cancel** stops the run that is going and lets the rest carry on, and
  **cancel all** stops the line with it.
- **split view** runs exports side by side instead of one after another. **split view**
  opens another queue window with a line of its own, **next in its own lane** sends the
  next export there instead of to the back of the line, and a run already waiting can be
  lifted out of the line with **split**, which starts it now rather than after everything
  in front of it. As many lanes can be opened as there is work for. The threads are shared
  out between them — two lanes are two halves of the machine, not two machines —
  and a lane with nothing left to do closes with its window. Lane one never closes: it is
  where everything lands by default.
- The clock on a finished run stops when the run does. `took 22m 54s` is what it took,
  rather than a stopwatch nobody remembered to stop.
- **an export writes itself down as it goes**, whether or not harmony is closed politely. A
  note goes on disk the moment a run joins a line, is brought up to date every twenty
  seconds while it writes, and is cleared only when the run reaches the end. A crash, a
  killed process, a pulled plug and a cancelled queue all leave the same thing behind: a
  run that can be picked up. Open the game again and the put-down exports are listed under
  the export panel with how far each of them got, and the console says so on the way up;
  **resume export** runs the same thing into the same folder with `skip existing` on, so
  every file already written is stepped over and the rest carries on. What is never
  written down is which sounds got written — the folder on disk is that list, and it
  cannot fall out of step the way a list could.
- **closing while something is running** asks first, and **pause and quit** stops every
  lane where it is and writes down every run in every one of them, so closing on a queue
  of five exports loses none of them. Each run has a file of its own, named after the game
  and the folder it writes into, so sending the same library to the same folder twice
  lands on the same note rather than leaving orphans behind.
- **folder tree** is one ladder of five, shallowest first: `file only` writes straight into
  the export folder, `sound path` keeps the folders the sound's own name carries, and
  `package`, `category/package` and `language/category` put one more level above that. The
  line under the chips is the path the selected sound would actually be written to, so the
  choice is read off an example rather than guessed from the name of the mode.
- **drag a row out of the window** to hand the sound to anything else: a folder, a chat, a
  DAW. A card says what is being carried while the files are written to a scratch folder,
  then the drag itself begins. Dragging a row that is part of the selection carries the
  whole selection. Escape calls it off, and anything nobody took is deleted.

Formats out: **wav** (decoded PCM), **flac** (lossless), **ogg** (the original Opus packets
remuxed, no re-encode — Opus titles only) and **raw** (the blob as the container holds it,
with the header a sab bank leaves off put back).

Flac is a copy rather than an encode wherever the sound is flac already, which is most of
black ops iii, infinite warfare, black ops 4, ghosts and advanced warfare: re-encoding a
lossless file changes nothing and costs time. Everything else — the pcm, the adpcm, the
opus — is encoded from the decoded samples, and comes out bit for bit what the wav would
have held in about half the space.

A sound harmony could not name is still written somewhere you can find it again. The
filename presets put the package in front of the key — `eng_codhq [a1b2c3d4e5f6]` — so a
folder of extracted keys sorts by where they came from rather than by a hash nobody reads,
and Discord says the same thing the same way, `eng_codhq/a1b2c3d4e5f6`, instead of a bare
number. Load a name list and both go back to being names.

## how it feels to use

Nothing slow happens on the thread that draws the window. Fingerprinting a folder, reading
a cached scan, writing one, loading name lists and the scans themselves all happen on
workers, and the window fills in as they land — a fifty megabyte cache arrives over a few
frames rather than freezing everything for a second. At startup the window goes up first,
behind a veil that says `setting up harmony..` while the last folder is recognised and its
catalogue read.

Leaving a tab does not throw the tab away. The last few games looked at keep their rows,
their tree and the row that was selected on a shelf, so coming back to one is a frame, not
a reload — and if the search box and the view have not changed since, the filtered list
comes back too rather than being built again. Older tabs fall off the shelf and come back
from their cache.

What is open is the file, not its contents. A sab bank is opened, its header read out of
the first block and its tables out of the last ones, and the audio between them is left on
disk until a sound is played — which is the difference between Black Ops II’s hundred and
ninety-nine banks costing nine gigabytes of memory and costing none.

Harmony draws on the gpu where there is one and falls back rather than fails where there is
not: wgpu first (vulkan or dx12), then opengl.

The paths harmony remembers are checked every few seconds on a worker, never on the thread
that draws: a drive that has been unplugged can take seconds to admit it. A game folder
that has gone is quietly forgotten and its tab goes back to having no folder behind it, the
export folder goes back to being asked for, a moved cache or scratch folder falls back to
the default beside the settings, and a put-down export whose folder is gone stops being
offered. Only a plain “not found” counts: a permission, a busy drive or a share that is
reconnecting keeps the path, because forgetting it would lose something you still have.

Rich presence, when it is turned on, says what is actually happening: the sound being
listened to and the game it is from, the game changing the moment a tab does, and an export
while one is running — `extracting 12.0k of 122k`, with the percentage beside the game and
a clock counting the run rather than the session. A sound harmony could not name is shown
under its container, `zmb_tomb.all/_f6a6b431ac13033b`, rather than as a bare number.

## the console

Tilde opens it, from anywhere, including the middle of typing a search. It is not a
terminal and does not pretend to be one: a terminal has one stream and colours words in it,
this has channels, levels and detail.

```
21:04:11  | export  done  lane 1: bo3 - 27,605 sounds finished
                          written 27,601 of 27,605, 4 failed
                          took    22m 54s
```

The clock, then the channel in its own fixed colour, then what kind of line it is, then the
sentence. A line with more to say carries it underneath: folded away unless something went
wrong, and one click either way. Failures carry a faint red stripe so a bad run reads as a
shape rather than as something to be read.

Channels are the point. `app`, `game`, `scan`, `pack`, `export`, `audio`, `names`, `disk`,
`discord` and `debug` — every part of harmony writes to one of them, and a chip at the top
turns each on and off with its count beside it, so a library run pouring out a line a
second can be watched on its own. `debug` is off until it is asked for. Levels filter the
same way, and **find** narrows to lines with a word in them, detail included.

Commands are dotted, so a family of them reads as one thing: `export.library`,
`export.split`, `lane.split`, `lane.close`, `queue`, `resume.list`, `resume.start`,
`skip.add`, `scan.depth`, `game.open`, `crash.dump`, `stat`, `paths`, `open`. Tab completes
and the prompt shows what it would complete with in front of the caret; a few letters are
enough, in order and not necessarily together, so `elib` finds `export.library`. The
arrows walk what has been typed before, and a name that is not a command is pointed at the
one it nearly was. `help` lists the lot with what each takes.

It is a window rather than a lid: drag it anywhere, resize it from any edge, and it comes
back where it was left. It stays off harmony's own title bar, which carries the close button
and is what the window is dragged by, and it slides in and out rather than appearing.

Everything said on the console is also written to `%APPDATA%\harmony\console.log` as it
happens, by a thread of its own so nothing waits on a disk to say something, and rolled
aside at four megabytes. That file is what a crash report takes its tail from.

## when something falls over

Harmony keeps crash reports the way the games keep minidumps: a folder per fall, named for
the moment it happened — `crash-2026-09-16-15-36-16` — under `%APPDATA%\harmony\crashes\`.
Each holds three things:

- `report.txt` — what happened, where, and on which thread; the stack it came from; what
  windows and the machine actually are; what harmony was doing, which game, how many sounds
  and which lanes were writing; the exports that were put down and where to pick them up;
  and the last sixty console lines.
- `console.log` — the whole scrollback as it stood, not only the tail.
- `state.json` — the same state in a form something else can read.

A panic on a worker does not take the window with it: the line goes on the console in red,
the report is written, and harmony carries on. The newest twenty reports are kept. Nothing
in a report leaves the machine — harmony uploads nothing, ever — and `crash.dump` at the
console writes one on purpose, without anything having gone wrong, which is the thing to
send when something is merely behaving oddly.

## sorting

Sounds are sorted by what their names say: a category, and a finer bucket inside it —
birds, glass, doors, efforts, machines. The `tree+` view stacks what is known about a
sound into one tree: which part of the game it belongs to, what kind of sound it is, which
bucket, then the folders in its own name. Nothing in it is guessed; it is the same facts,
arranged so a hundred thousand names can be walked down to a handful.

## caches

Harmony keeps two caches, both in the folder the left card names and both rebuildable at
any time. A scan cache is the catalogue of one game, read back when its tab is opened. A
zone index is what each fastfile turned out to hold: Modern Warfare 3 takes four minutes
and twenty seconds to open the first time and five seconds after that, because three
hundred zones do not have to be inflated again to learn something that has not changed.
Both are stamped against the files they were read from, so a patched game reads again.

## names

The modern games store sounds under hashes, not names. Harmony ships no name lists. Press
**names** to load your own: either `hash,name` pairs or a plain wordlist, which harmony
hashes every way the engines have used. Anything that matches is named, categorised and
searchable; the rest stay as their key, which still plays and still extracts.

The functions harmony tries are the ones the engines and the published databases use:
fnv-1a 64 folded the way the engine folds it (lower case, `\` as `/`), the same function
over the name exactly as written, the 63-bit asset hash of the newer Infinity Ward titles
(offset `0x47F5817A5EF961BA`), the script hash of Modern Warfare II and III (offset
`0x79D6530B0BB9B5D1`, prime `0x10000000233`), fnv-1a 32 for the older Treyarch banks,
and dbj2. A `hash,name` list is filed under the hash as written and under both masks the
containers use, so a list written for one game still answers for another's tables.

A sound harmony could not name is written in its own colour, so the list says at a glance
which names are real and which are keys. Where a container carries no ids at all — the
sound paks are one stream after another with nothing between them — harmony shows where
the sound sits, `soundfile12#00042`, rather than a number it worked out itself and printed
as though the game had given it.

Matching runs on a worker and the results are cached, so a list is read once and the window
keeps drawing while a hundred thousand keys are looked up.

What that does not do is invent a mapping that is not there. Published databases such as
xhashdb are keyed by asset *names*; Modern Warfare III's packages key their sounds by a
number that is not the hash of any name in those lists, and the Black Ops II banks key
theirs by a 32-bit id. Harmony names what actually matches and leaves the rest as keys
rather than guessing.

## what harmony does not do

- It ships no game data, no name lists and no Oodle library. Decompression uses the
  `oo2core_8_win64.dll` that is already in your install.
- It does not touch the game process, and it uploads nothing anywhere.

## command line

The window is the point, but the same reader runs headless, which is how it is tested:

```bash
harmony --root "<game folder>"                 # open the window on a folder
harmony --probe "<game folder>" [deep|full]    # scan and report, and fill the cache
harmony --packages "<game folder>"             # list packages and entry counts
harmony --hits "<game folder>" <package>       # how much of one package is audio
harmony --pull "<game folder>" <n> <out>       # extract n sounds in every format
harmony --dump "<game folder>" <package> [n]   # entry layout of a package
harmony --grab "<game folder>" <package> <key> # one blob to a file
harmony --stream <file>                        # work out a blob's seek table
harmony --sound <file> [out.wav]               # decode one file and measure what came out
harmony --room [folder]                        # free space where harmony writes

harmony --zone <fastfile> [out folder]         # every sound inside one fastfile, as wavs
harmony --inflate <fastfile> <out file>        # the zone behind a fastfile, uncompressed
harmony --casc <game folder> [extension]       # what a battle.net storage holds
harmony --opus-keys <game folder> <package>    # what an encrypted package would need

harmony --hash <name> [name...]                # a name under every hash harmony tries
harmony --names <game key> <depth> <list...>   # match a cached scan against name lists
harmony --names-probe <game key> <depth> <csv> # how much of a scan one list names
harmony --sort <game key> <depth>              # how a cached scan files itself

harmony --pull <game folder> <n> <out> [split] # n sounds out in all four formats, through
                                               # the real queue; "split" gives each format
                                               # a lane of its own instead of a place in
                                               # the line
harmony --crash-report [panic]                 # write a crash report; "panic" falls over
                                               # on purpose to prove the hook
```

## where harmony keeps things

`%APPDATA%\harmony\`: `settings.json` (the folder remembered for each game, output,
favorites, tags, collections, presets, window size) and `scan-<game>-<depth>.json` (the cached catalogue of a scan) and
`zones-<game>.json` (what each fastfile turned out to hold), a `resume` folder holding one
note per export that has been put down and not yet picked up, `console.log` (everything the
console has said, rolled aside at four megabytes) and a `crashes` folder holding the last
twenty crash reports.

All three writing folders can be moved, from **folders** in the right-hand panel:

- **cache** — where the scan caches go. A catalogue of a big install runs to tens of
  megabytes (Modern Warfare III's deep scan is 52 MB), so it does not have to sit on the
  system disk. Moving it carries the caches already written across; a name already taken at
  the far end is left alone.
- **scratch** — where the files a drag out of the window needs are written before the drag
  starts. Swept at startup.
- **export** — where extraction lands, which is the folder the **folder** button sets.

The settings file itself always stays in the config folder, so harmony can always find it.
Each line shows what is free on that disk, and the left card says where the scan that is
open is being cached.

### when the disk is full

Harmony measures the room before it writes, and keeps 256 MB of slack on top of what the
work needs. If there is not enough it does not start: a card says what was being attempted,
where it was writing, what it needs, what is free and how much is missing, and waits —
**try again**, send it **somewhere else**, or **leave it**. Nothing is half-written, and a
scan that could not be cached is still there to browse and extract from.

If a disk fills up while the queue is running, the queue pauses itself at that file rather
than failing every one that is left. What was already written stays; free some room and
press **carry on**, or cancel the rest.

## building

```bash
cargo build --release
```

Needs a Rust toolchain with the 2024 edition. On Windows the build script turns
`images/chinchou.png` into the icon compiled into the exe, so the taskbar, Explorer and the
window all show the same thing. No C toolchain, no cmake: the Opus decoder
is pure Rust.

## working on it

`CLAUDE.md` is the entry point for anyone, or anything, picking the codebase up: the
standing rules, the house style, and a table pointing at the one doc under `docs/` that
covers the area being worked on rather than the lot.

## how it works

`DESIGN.md` has the whole of it, including every format fact measured against a real
install and the evidence for each one. In short: `.xpak` / `.xsub` are KAPI containers
whose entries are block chains of Oodle, LZ4 or stored data; a sound is a seek table
followed by length-prefixed Opus packets, in two shapes; harmony identifies one from its
first 2 KB and reads packages in disk order, which is what makes a scan of a 100 GB
install finish.
