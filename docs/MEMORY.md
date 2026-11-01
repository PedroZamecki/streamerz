# Streamerz memory investigation

This is the initial baseline investigation. The subsequent measured strategies, applied improvements,
and remaining CPU/storage limits are in [Memory experiments and applied changes](MEMORY-EXPERIMENTS.md).

Measured on November 1, 2026, Linux x86_64, glibc, kernel 7.2.8-9-omarchy.

## Finding

The growth is reproducible without a platform connection, images, or a large retained chat log.
Distinct Community users are the main measured driver. Repeating 10,000 messages from ten users
stays near 27.6 MiB RSS. The same message count from 10,000 distinct users leaves about 109 MiB.
Turning chat history off leaves about 107 MiB. Short chat expiration reclaims allocator pages, but
preserved Community records remain live.

User decision: keep every Community record and its statistics across restarts. Redesign storage and
resident working data; do not solve this by deleting inactive users.

## Method and limits

- Freshly rebuilt `cargo build --profile fast`, optimized development build with line tables.
  Binary SHA-256: `962c644030c334d557fb00f9eb6b5cd28e3697edf432d061a33712f1449233cb`.
- Existing branch `release/1.2.1`, including the pre-existing `src/ui/suggest.rs` working change.
  No application code was changed for this investigation.
- Separate isolated configuration per run. Hidden display `:99`; real settings, tokens and Community
  files were not read or copied. No live integrations, emotes, OBS connection or plugins enabled.
- Synthetic messages enter the existing development WebSocket `InjectChat` request, then the actual
  chat, commands, Community tracking, history and save paths. Each message has 161 ASCII text bytes.
  No matching custom actions are configured. This measures the engine path, not network parsers or
  a realistic mix of badge images, commands and animations.
- WebSocket injection waits for each acknowledgement. This isolates retention under delivered traffic;
  it does not measure overload, socket backpressure, or the maximum throughput of real integrations.
- The 10,000-message cases sample at checkpoints and after 1, 5 and 15 idle seconds. The later 25,000
  and shown-window cases also sample every 200 ms externally. Neither sampling scheme promises the
  exact instantaneous RSS peak. Measurements include actual saves during traffic and normal idle.
- Native RSS samples and allocation tracing are separate runs. Some runs overlap in time; do not use
  their wall-clock durations as isolated throughput benchmarks. Shared-library PSS and USS change
  when another process maps or unmaps the same files. RSS is the primary cross-run comparison here.
- Heaptrack 1.5.0 downloaded and extracted under `/tmp/streamerz-profile-tools`, with no system install,
  new application dependency, background monitor in the product, or production instrumentation.
- These are development-binary measurements, not a claim about current shipped AppImage or Windows
  working set. The hidden Twitch + OBS release-budget check remains outstanding.

## Native process measurements

All memory figures below are MiB (1,048,576 bytes). Every case delivered 10,000 messages.

| Workload | Baseline RSS | Traffic-end RSS | RSS after 15 s idle | PSS after 15 s idle |
|---|---:|---:|---:|---:|
| 10 users, history on | 27.02 | 27.61 | 27.61 | 12.38 |
| 10,000 users, history on | 27.16 | 109.04 | 109.04 | 93.52 |
| 10,000 users, history off | 26.83 | 107.06 | 107.06 | 91.39 |
| 10,000 users, three-second lifetime | 26.95 | 109.39 | 68.49 | 60.26 |

The highest checkpoint in the unique-user case was 124.24 MiB, falling to 109.04 MiB at the next
sample. Traffic-end values are therefore not peak values. History off still reached 121.97 MiB at
its final traffic checkpoint. All four hidden cases recorded zero additional process CPU ticks over
15 seconds after traffic ended, at the kernel's 100 Hz accounting resolution.

A separate 25,000-distinct-user run crossed 200 MiB at the 18,000-message checkpoint.
Its highest observed RSS was 272.59 MiB (checkpoints plus 200 ms sampling),
and it retained 231.14 MiB RSS, 222.86 MiB PSS and
222.52 MiB USS after 15 seconds idle. Swap remained 0 bytes.
This reproduces usage above 200 MB without live integrations or images. It is observed growth,
not an extrapolation from the smaller run.

A separate shown-Chat test, 10,000 messages from ten users, started at 38.06 MiB RSS, ended traffic at
47.14 MiB, and reached 48.19 MiB during idle. Clicking its own discovered Hide button released it to
46.63 MiB after two seconds. Hiding a previously used window does not restore its cold-start baseline.
This test has no emote or avatar payloads and does not measure image-cache cleanup.

## Requested heap bytes versus resident pages

Heaptrack records allocation/free events and allocation backtraces. Its Massif export gives requested
live heap bytes over time. Those exclude allocator metadata, unused allocator pages, stacks, mapped
libraries, graphics-server allocations and profiler overhead. RSS from an instrumented run must never
be presented as the product's normal RSS.

| Instrumented workload | End-of-traffic requested heap | Requested heap after 15 s | End-of-traffic RSS | RSS after 15 s |
|---|---:|---:|---:|---:|
| 10,000 unique users, normal lifetime | 36.85 MiB | 36.85 MiB | 130.66 MiB | 115.41 MiB |
| 10,000 unique users, three-second lifetime | 36.46 MiB | 36.22 MiB | 113.61 MiB | 74.26 MiB |

In the expiration run, live requested bytes fell by 253,356 bytes (0.24 MiB), while resident pages
fell by 41,267,200 bytes (39.36 MiB). Expiration's existing `malloc_trim` call can release previously
freed allocator pages as well as pages used by the expired lines. This is evidence of substantial
allocator retention in addition to live Community storage. It does not mean 39 MiB of chat text
expired, or that every byte of RSS minus requested heap is recoverable allocator waste.

Heaptrack's normal-lifetime trace reports 52,133,924 allocation calls, about 4.35 million temporary
allocations, and 88.01 decimal MB peak requested heap. It identifies `save_community` cloning and
`spoke` user-map insertion among the largest allocation stacks. At the merged peak for raw vector
storage, 29.12 decimal MB is attributed to Community clone storage and another 29.12 MB to user-map
insertion storage. Those are components in that report, not independent whole-process peaks to add
across unrelated timestamps. Some Rust generic symbols are merged or imperfectly demangled; the
source caller frames identify the Community operations.

The trace's end-of-process "leaked" total is not proof of a memory leak. Global/process-lifetime
objects and the exit path are included. Diagnose retained ownership while the process is alive first.
Shutdown also saves Community, producing another allocation spike after the last idle sample.

## Where the growth comes from

`src/engine/community.rs` keeps every user in `Vec<Obj>`, where `Obj` is an ordered JSON map.
Each tiny field carries map storage, a separately allocated key and sometimes a separately allocated
string value. Loading expands omitted defaults back into each record. The complete collection stays
in memory even with the window hidden or chat history disabled.

`spoke` scans the user vector to find a record. Credits also scans its growing list to deduplicate
names. New distinct users therefore increase both memory and work per delivered message.

Every 50 changes (also on a change after 60 seconds), `save_community`:

1. Clones the complete user vector and its JSON maps.
2. Removes defaults from the cloned objects.
3. Builds another JSON value with `json!`, including the users and credits.
4. Serializes a complete output string.
5. Starts a fresh writer thread with that string and replaces the entire file.

For 10,000 new users, saving every 50 means 1,005,000 user-record visits for those periodic saves
alone. Full snapshots and scans make total work grow approximately quadratically as the collection
increases. The writer thread performs file I/O; much of the cloning and serialization happens on
the engine thread before the writer starts. Writers also use the same temporary filename, so save
serialization/coalescing deserves a correctness review as well as a performance review. A collision
was not tested or reproduced here.

## What frees memory, and when

| Owner | Current bound | Cleanup trigger and limitation |
|---|---|---|
| Engine chat lines | Default 400 per platform, 1,000 total | Oldest lines dropped as new ones arrive; limit reductions drop excess. Capacity shrinks only when sufficiently oversized. |
| Chat lifetime | Off by default | Existing scheduled sweeps drop expired timestamped lines. Sweep batching is at least one second, or one tenth of the lifetime; expiration is not an exact millisecond deadline. Unknown timestamps do not age out. |
| Chat retention disabled | No stored lines | Clears and shrinks engine lines, calls trim. Community tracking continues. The path is reached per message too, so allocator trimming frequency needs CPU review. |
| Chat seen-user facts | Up to configured total during insertion | Least-recent user removed on admission at the limit. Line expiration does not sweep these facts; changing a limit does not immediately trim all existing users. |
| Community users | No resident size cap | Channel-management delete/move paths or process exit. Neither chat expiration nor hide evicts cold persistent users. |
| First Words | One key per distinct speaker | Explicit reset, stream/channel reset paths, exit. Must preserve correct first-message semantics if moved out of RAM. |
| Credits | Growing distinct-user/event lists | Explicit reset; stale credits discarded on load after 12 hours. This is not an idle 12-hour eviction timer. |
| UI chat lines/layout | Copies in each pane | Hide releases panes and shrinks buffers; hidden panes refill from the engine when shown. Fonts and other UI state can remain resident. |
| Action history | 100 records | Oldest record replaced; clear drops records, but `clear` alone need not release backing capacity. Large argument objects can still make 100 records expensive. |
| User-card facts | 200 entries | Ten-minute TTL controls reuse on lookup; expired entries are not proactively swept. At 200, insertion clears the collection. |
| Decoded images | 8 MiB accounted pixels | LRU eviction; hide clears decoded cache and ready queue. This limit is not a whole-image-pipeline resident budget. |
| Image jobs/results | Two active decoders, six active downloads | Completion releases work. Semaphores bound active work, not all waiting tasks or completed `READY` bytes. Work is not cancelled by clearing the ready queue on hide. |
| Asked image URLs | No count/byte cap | Process exit. Successful and failed requests remain remembered for the run. |
| Emote catalogs | Whole catalog plus lookup maps | Catalog replacement/configuration and exit. Catalog strings and lookup maps duplicate data; hiding does not remove them. |
| Chat profile-picture URLs | About 400 entries | Admission clears the whole map at its limit; saved JSON remains on disk. |
| Command cooldowns | No pruning found | Expired entries return zero remaining time but are still stored. Needs its own workload test. |
| Command counters | Per command and per user that invokes it | Persistent or temporary maps, incremented for matching commands. No automatic bound was found; not exercised by this workload. |
| Local/remote signals, queued actions | No general bound found | Consumed/cleared by their workflows. Unconsumed signals or slow actions can grow independently of chat limits. Not exercised here. |
| Allocator free pages | Depends on glibc arenas/fragmentation | Some frees return pages automatically; existing trim calls try to release whole unused pages. Live objects and partially occupied pages cannot simply be trimmed away. |

The image cache has two further limits to verify: a single oversized picture can survive the
`len() > 1` eviction condition, and PNG/GIF/WebP dimensions are not governed by a common decoded-byte
budget. Animations are capped at 150 frames, which is not a byte cap. These are source findings,
not measured causes of this synthetic run's growth.

## Disk use

The 10,000-user synthetic `community.json` contains all 10,000 records in 1,975,651 bytes.
Gzip of those exact bytes is 111,651 bytes, a 94.35% reduction. This is an offline compression
measurement, not a new application format or a guarantee for real names and arbitrary user data.
Repeated synthetic content compresses especially well. At 25,000 users, the file was 5,005,651 bytes,
or 277,467 bytes gzipped, with all 25,000 records present. Ten users use 2,131 bytes, or 313 gzipped.

Images are kept in their downloaded PNG/JPEG/WebP/GIF formats. Recompressing them blindly is unlikely
to be a useful first change. The current 50 MiB disk image target excludes JSON catalogs and is checked
at the first miss and periodically after successful downloads, so it is not a hard whole-cache disk
limit. Thousands of small files also occupy filesystem blocks: compare allocated bytes with file
length, not just the sum of encoded image sizes. Backups already use gzip.

Compressing the whole Community file alone would not fix RAM: it would still be expanded into the
same JSON objects. Recompressing a full growing snapshot every 50 messages may make CPU and write
amplification worse. Persistence needs a working-set design as well as smaller encoding.

## Recommended implementation sequence

1. Replace repeated full save clones with a borrowed serialization view, then stream the encoding
   and coalesce writes so only one save is in flight. Preserve atomic writes, last-state durability,
   unknown fields, backup compatibility and crash behavior. This removes transient duplication but
   does not by itself solve the full resident Community collection.
2. Use compact typed resident records for known user fields, retaining unknown fields only where
   necessary. Stop allocating the same keys and default strings in every record. Add indexed user
   lookup rather than scanning all records for every message.
3. Put complete historical records in indexed disk storage with a bounded resident working set.
   Load Community UI pages and user cards on demand; retain exact counts/statistics and First Words
   behavior. Dirty records must be persisted before eviction. Select the format and migration plan
   before implementation; no database dependency is approved or introduced by this investigation.
4. Give caches both byte and entry budgets. Bound pending image work and ready results, reject or
   downscale oversized decodes, and cancel or discard obsolete completions by generation when hidden.
   Include references held by rendered rows and temporary decode copies in the memory accounting.
5. Reuse existing activity/expiration opportunities to retire expired cooldowns and cold facts.
   Do not add a permanent cleanup poll. Define queue/signal overflow behavior before dropping work.
6. Benchmark chunk compression with existing `flate2` against uncompressed indexed records. Compare
   actual allocated disk space, write volume, random read latency, CPU and peak RSS. Smaller encoded
   bytes alone are not sufficient grounds for a format change.
7. Repeat native and traced profiles after each isolated change, plus long repeated-user and
   distinct-user runs. Verify startup with large saved Community data, restart integrity, shown/hidden
   transitions, TTL expiry with no new messages, queue drain, images and cancelled in-flight jobs.
   Finish with current release builds on Linux and Windows and live Twitch + OBS hidden-budget tests.

## Reproducing the measurements

```
cargo build --profile fast
python3 dev/memory.py --out /tmp/streamerz-memory-new --messages 10000
python3 dev/memory.py --out /tmp/streamerz-memory-large --messages 25000 --cases unique
python3 dev/memory.py --out /tmp/streamerz-memory-ui --messages 10000 --cases repeat --shown
python3 dev/memory.py --out /tmp/streamerz-memory-trace --cases unique ttl \
  --heaptrack-lib /path/to/libheaptrack_preload.so
```

Run a hidden X display at `:99` first. The harness removes `WAYLAND_DISPLAY` and targets that display.
It never kills other Streamerz processes. Use different `--port` values for concurrent experiments.
Its source settings and the app's own development copy are separate from real settings. It preserves
only synthetic metrics, logs, a screenshot for shown tests, and optional traces in the output folder.

After the traced process exits, convert its raw stream using the matching Heaptrack version:

```
heaptrack_interpret < unique/heaptrack.raw | zstd -o unique/heaptrack.zst
heaptrack_print -f unique/heaptrack.zst -a 0 -T 0 -M unique/massif.txt > unique/summary.txt
```

Raw traces can be gigabytes because they record every allocation/free. They are profiling output,
not product cache usage. Their interpreted compressed files in this experiment are under 1 MiB each.
The report summaries and Massif timelines remain in `/tmp/streamerz-memory-heap/{unique,ttl}/`.
Native samples are in `/tmp/streamerz-memory-baseline/`, `/tmp/streamerz-memory-25k/` and
`/tmp/streamerz-memory-shown/`. These `/tmp` artifacts are not durable across reboot.

## Measurement references

- [Kernel process memory documentation](https://cdn.kernel.org/doc/html/latest/filesystems/proc.html):
  RSS is resident pages; PSS proportionally attributes shared mappings. USS here is private clean
  plus private dirty pages. Swap is recorded separately. No single scalar includes all app-attributable
  GPU/server/kernel resources.
- [Heaptrack](https://github.com/KDE/heaptrack): allocation backtraces, temporary allocation counts,
  requested heap tracking and Massif export.
- [glibc memory allocation documentation](https://www.gnu.org/software/libc/manual/html_node/Memory-Allocation.html):
  object allocation, release and allocator behavior.

## Validation

`cargo build --profile fast`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`python3 -m py_compile dev/memory.py` and `git diff --check` passed. The harness completed all four
10,000-message retention cases, both allocation-traced cases, the 25,000-user case and the shown/hidden
case. No runtime behavior, UI design, persistence format or cache policy was changed.
