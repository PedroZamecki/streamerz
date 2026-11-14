# Memory experiments and applied changes

Follow-up to [the initial profile](MEMORY.md), November 1, 2026. All data below is measured,
not estimated savings. [Machine-readable results](memory-results.json) contain binary hashes,
workload parameters, process counters, latency, I/O and phase snapshots.

## Applied result

The selected implementation keeps compact JSON records in RAM, expands a record only when needed,
indexes identities, serializes borrowed records directly to a 64 KiB file buffer, and gives the
Community list only its displayed columns. All user records and statistics are retained. The file
is still ordinary `community.json`; no migration or new database dependency was introduced.

Matched unthrottled release-profile runs, each delivering 25,000 distinct users:

| Metric | Original implementation | Current implementation |
|---|---:|---:|
| RSS after 15 s idle | 224.63 MiB | 32.02 MiB |
| Highest sampled RSS | 266.07 MiB | 32.02 MiB |
| Process CPU time during traffic | 198.88 s | 2.73 s |
| Traffic duration | 204.57 s | 4.05 s |
| Delivered messages/second | 122.21 | 6166.87 |
| 99th-percentile acknowledgement | 86.66 ms | 1.71 ms |

Retained RSS fell by 85.75%. Total process CPU time fell by
98.63%. Traffic completed 50.5 times faster.
The current run's idle PSS was 23.78 MiB and USS 23.43 MiB, with zero swap. RSS includes mapped shared
code and libraries; PSS and USS answer different ownership questions. Do not add profiler overhead
to the normal-process numbers.

## Controlled comparison of conflicting strategies

Each row uses the **same release-profile experiment binary**, 10,000 distinct users, a target of 1,000
messages/second, and the same buffered writer. Identity indexing, credits indexing, command filtering
and other refinements are held constant. This isolates representation and save-copy effects; its
control is not the old implementation with linear lookups. The old implementation is tested separately
above. CPU is total process seconds for the complete delivered workload, not a percentage.

| Strategy | Retained RSS (MiB) | CPU seconds | Delivered messages/s | Ack p99 (ms) |
|---|---:|---:|---:|---:|
| Full maps, copied saves | 102.32 | 8.52 | 919 | 35.84 |
| Full maps, borrowed saves | 64.59 | 4.99 | 997 | 13.55 |
| Packed records, copied saves | 65.96 | 7.20 | 978 | 30.20 |
| Packed records, borrowed saves | 29.29 | 3.98 | 999 | 2.68 |
| Deflated records, borrowed saves | 28.30 | 9.82 | 857 | 37.87 |

Packed records alone do not solve the transient full-save copies. Borrowed saves alone still leave
large resident maps. Combining them removes both sources of growth. Record-level Deflate saves
about one more MiB, but uses over twice the CPU time and misses the target rate. It is therefore
available only as a development experiment; production uses packed JSON.

A separate end-to-end target-rate test after the admission refinement delivered about 999
messages/second, used 3.82 process CPU seconds over ten seconds, and retained 26.08 MiB RSS after
10,000 distinct users. Earlier release runs measured 3.86 to 3.92 CPU seconds and retained 26.06
to 27.02 MiB. Native allocator/page variation is visible;
these are measurements, not guaranteed byte-exact budgets.

## Streaming versus buffered writes on the real filesystem

These trials use Btrfs on the project's SSD, not `/tmp`'s tmpfs. Each starts with the same 25,000-user
file and processes 1,000 more messages from ten existing users at a target of 1,000/second. All other
settings are identical. The final synthetic file is fsynced outside the traffic interval before
filesystem allocation is recorded.

| Writer | Retained RSS | CPU seconds | Delivered messages/s | Ack p99 | Longest ack |
|---|---:|---:|---:|---:|---:|
| Buffered snapshot and writer thread | 41.72 MiB | 0.44 | 991.63 | 6.59 ms | 7.87 ms |
| 64 KiB streaming buffer | 31.62 MiB | 0.38 | 988.11 | 11.24 ms | 12.98 ms |

Streaming saves about 10 MiB in this warm-data case and removes concurrent writer ordering races,
but its file writes block the engine. The p99 acknowledgement is consequently worse in this short
SSD test. The production choice prioritizes RAM, with the measured latency cost stated explicitly.
A slower disk or much larger dataset needs its own latency test.

Both variants issue about 100.1 MB of logical writes for a roughly 5.0 MB final file. Streaming lowers
temporary storage, not write amplification. `st_blocks * 512` reports 5,009,408 allocated bytes for
5,005,831 file bytes after the flush. This is filesystem accounting, not a measurement of compressed,
shared physical SSD extents or NAND writes. The mount has `compress=zstd:3`; that does not make file
length, `st_blocks` and physical media bytes interchangeable.

## Sustained 60-second test

On that same Btrfs filesystem, with 25,000 users already loaded, the final program processes 60,000
messages from ten returning users. Its saved file is checked by identity and message count afterward.

- Delivered rate: 999.78 messages/second over 60.02 seconds.
- RSS: 31.64 MiB at baseline, every 1,000-message checkpoint, and after 15 seconds idle.
  The maximum in external samples and the kernel high-water counter was also 31.64 MiB.
- Process CPU: 22.25 seconds, or 37.08% of one core.
- Acknowledgements: p99 10.50 ms, maximum 14.35 ms.
- Persisted: all 25,000 users and 85,000 total messages, with each user's count verified.
- Logical process writes: 6.007 GB for a 5.006 MB final file.
- Idle: zero additional process CPU ticks during the following 15 seconds, at 100 Hz resolution.

**The 1 to 2% of one-core CPU target is not met.** The sustained result is 37.08%. Simple normalization
across this machine's 12 logical CPUs is about 3.09%, which is a different denominator. There is no
claim of a new CPU throttle or a hard global RAM cap. The test includes development WebSocket request
parsing and acknowledgements; real platform decoders and action workloads still need independent tests.

The measured processing/storage bounds are a 64 KiB snapshot output buffer, one expanded record at a
time on the mutation path, and the existing chat retention limits. Full snapshot work still grows with
the number of historical users. No save frequency was reduced and no events or user records were dropped.
A bounded total working set and bounded persistence work need indexed cold storage/incremental writes,
with a tested recovery and migration design. Merely increasing the interval would weaken the current
save guarantees and is not the implemented solution.

## Community UI and reclamation

A controlled UI comparison loads the same 25,000-user seed, adds 1,000 returning-user messages, then
opens Community, leaves it, reopens it, and hides the window. Both release-profile variants already
have packed engine records and hidden-page cleanup; the differing strategy is full user-map transfer
versus projection to list columns.

| UI representation | RSS while Community shows | RSS after leaving | RSS after hiding |
|---|---:|---:|---:|
| Full maps sent to the list | 104.97 MiB | 48.36 MiB | 48.39 MiB |
| Only list columns sent | 58.21 MiB | 48.46 MiB | 49.36 MiB |

The list now keeps identities and rendered rows, rather than full JSON maps. A card fetches its full
record by indexed identity. Leaving the page or hiding the window drops list storage and returns
unused allocator pages. Returning to the page refills it with the same count and preserves sorting,
filtering and card behavior. The smoke test checks release/refill, and screenshots cover 700x600 and
1200x800. Native UI/font/backend state remains after the first show, so hide does not restore cold RSS.

A prior matched fast-build test of the earlier UI cleanup had 10,000 saved users: hidden RSS after
visiting Community fell from 77.23 to 50.30 MiB across the refinement. The stronger list-column projection
above additionally avoids constructing an entire expanded collection for display.

Chat expiration was tested separately: with a three-second lifetime, the refined fast build fell from
33.15 to 32.62 MiB within five seconds after traffic stopped. This smaller reclaimed amount is expected
once large temporary save copies are removed. Persistent users are still live and are not expired
with chat lines.

## What is freed in the selected implementation

| Memory owner | When ownership ends | What can remain resident |
|---|---|---|
| Expanded Community record being changed | The updated compact record replaces it before `spoke` returns | Packed record and identity index stay live |
| Previous packed record | Replacement by `Users::put` | Allocator pages can be reused without reducing RSS |
| Snapshot output buffer | Save finishes or returns an error | Live records remain; no full snapshot string exists in the default path |
| Community list columns transferred to the UI | Rendering that refresh finishes | Rendered rows and identities remain while the page shows |
| Community rendered users and identities | Leaving Community or hiding the window | Engine records, fonts and native backend state remain |
| Queued old image work | Generation check before decode, including after acquiring a decoder permit | An already active decode runs to completion |
| Old decoded image result arriving after hide | Publication rejects it and drops its pixel buffer, then calls `trim_memory` | Other live decode buffers and partially occupied allocator pages remain |
| Ready decoded images | Hide calls `forget` and clears the queue | Files on disk remain |
| Retained chat lines | Count limit, configured lifetime or explicit clear | Persistent Community statistics are independent |

Rust drops establish when allocations become free for reuse. `/proc` establishes when process pages
actually leave RAM. These are different events: there is no claim that every drop immediately lowers
RSS. The UI phase measurements and focused allocation traces check both sides where tested. The
initial report contains the broader ownership inventory and measured expiration timeline.

## Allocation tracing and remaining costs

The combined release-profile trace records 1,714,997 allocation calls, 332,365 temporary allocations
and 7.02 decimal MB peak requested heap for 10,000 distinct users. The earlier pre-admission trace
recorded 2,074,952 calls and 6.52 MB peak heap. Fewer allocation calls did not lower every heap metric;
the measured peak rose by 0.50 MB while native retained RSS fell in the paired admission test. The initial fast-build trace recorded 52,133,924 calls and 88.01 decimal
MB peak heap. These traces use different optimization profiles, so they are supporting observations,
not the controlled factorial comparison above. Instrumented RSS includes the profiler and is excluded
from native RAM comparisons. The exit-time report's "leaked" count includes intentionally live data
and is not itself a leak diagnosis.

A separate `perf` CPU sample of the packed-record intermediate identified seen-user eviction scans,
chat-limit traversal, allocation/free operations, credits duplicate checks and JSON formatting among
remaining sampled work. It captured 719 samples with zero reported losses. Stack unwinding was limited;
its self samples are not a complete attribution of engine CPU. The captured data remains in the
local experiment directory.

Changes applied beyond record packing:

- Identity lookup uses platform/channel/id indexes, without copying platform and channel strings into
  every index entry. Legacy records without a channel keep their original first-match behavior.
- Credits use a sorted index into insertion-ordered names. This preserves wire order and case-sensitive
  deduplication without a second copy of every name. Lookups are logarithmic; index insertion can still
  move integers, so it is not constant-time work.
- Unmatched chat commands are no longer cloned on every message. Matching commands remain snapshots,
  and the config borrow is released before running them. Captures are recomputed for the command
  being executed so scratch captures do not accumulate across all matches.
- The normal save path has no writer thread and no full output string. Saves are ordered and replace
  the previous file only after successful serialization and flush. Existing power-loss guarantees are
  unchanged; no new fsync policy was silently introduced.
- Loading uses a buffered reader and creates packed records incrementally. It does not first build
  a complete expanded user-map collection or a full input string.

The on-disk JSON length is deliberately unchanged. Whole-file gzip remains much smaller (for the
60,000-message run, 5,005,841 bytes becomes 281,011 bytes offline), but that does not fix RAM or write
amplification. A new disk format must be measured with recovery, random reads, compaction and backups.

Remaining untested or unresolved budgets: large arbitrary user payloads; an unbounded historical
record count (packed records still remain resident); image decode/pending-result bytes; command
counters/cooldowns; unconsumed signals/action queues; real Twitch + OBS hidden operation; Windows;
and shipped AppImage behavior. None is claimed solved by this synthetic chat workload.

## Final admission and lifecycle refinements

The controlled matrix above predates the last small refinements, which were tested independently
and then in the combined build used for the headline and final sustained figures.

A new user's complete record is now built before it is packed, rather than packing an incomplete
record, decoding it, and packing it again. Constructed objects are moved instead of cloned. A paired
25,000-user test measured 33.46 MiB before versus 32.09 MiB after. CPU time in that pair was 2.39
versus 2.56 seconds: the RAM saving is measured, but a CPU improvement is not claimed. The selected
result prioritizes lower RAM. Combined repetitions measured 31.99 to 32.02 MiB and 2.58 to 2.73 CPU seconds. The headline uses
the last build, including the final command-capture scratch refinement.

The previous build failed a new smoke regression: a queued chat refresh refilled Community rows
after hide. Visibility guards now prevent deferred chat and image refreshes from rebuilding hidden
presentation state. Decoder work carries a generation; hiding invalidates queued old decode work and
rejects late results. Rejected pixel buffers are dropped before asking the allocator to return unused
pages. No timer or cancellation worker was added. A focused test exercises two 4 MiB pixel payloads,
checking stale rejection, acceptance of current results and clearing of ready pixels. These are
ownership/lifecycle checks, not a claim that the complete image pipeline has a byte budget. A focused
Heaptrack run of this test measured 4.40 decimal MB peak heap and 122.43 KB retained at exit, with no
retained ready pixels. This trace is separate from native whole-process measurements.

The final combined minute test also opens Community afterward, verifies all 25,000 rows, checks
700x600 and 1200x800 captures, leaves and reopens the page, and hides the window. Whole-trial peak RSS,
including those UI operations, is 65.81 MiB; pre-UI traffic and idle RSS is 31.64 MiB. The machine-readable
summary's final/peak fields include UI for this trial, so use its explicit idle phase for steady-load RAM.
This last repetition measured 56.89 MiB while Community showed, 48.53 MiB after leaving and 53.29
MiB after reopening and hiding. The list count was zero after cleanup. This variation reinforces that
releasing list ownership does not promise an identical RSS or a return to the cold-start baseline.

## Validation

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass.
- Current source builds with `cargo build --release`.
- 243 non-UI tests pass, with 4 pre-existing ignored tests. In separate processes, 42 UI tests pass
  and one pre-existing UI test remains ignored. That is 285 passing tests and 5 ignored tests overall.
  A combined run stalled on shared cross-test state; GUI process isolation is the successful
  verification method, not a claim that the single-process combined suite completed.
- New tests cover record encodings, unknown nested fields, legacy channel identity ordering, update
  and restart round trips, malformed optional list fields, credits order/case, and failed atomic saves.
- The application smoke run visits every page, opens cards and menus, and checks cleanup/refill.
- Every native trial verifies saved identities and per-user message counts. The restart trials seed
  a fresh process from the preceding synthetic output.
- No runtime dependencies, worker threads, timers or cleanup polls were added. `raw_value` is an
  additional feature on the existing `serde_json` dependency. Development-only switches preserve
  competing strategies for repeatable experiments; production ignores them.

## Reproduction

Baseline source: `272aa8d3e5336d4bdebf1062befc2755eaef6db4`, with the pre-existing Suggest fix left in
place. Baseline and candidate binaries were copied outside `target` so subsequent builds could not
replace the executable under test. All workload settings were synthetic and isolated; no real tokens,
settings or Community data were copied. Native trials run sequentially without compilation or heap
profilers competing during their traffic interval. The machine uses its normal power management,
without CPU affinity or a forced frequency; report variation rather than assuming identical clocks.

Use a release-profile development binary (do not set `STREAMERZ_VERSION`) to enable the existing
injection tools and experimental switches. These measurements are not a Windows or shipped AppImage
benchmark. Run Xvfb at `:99`, then:

```sh
cargo build --release
python3 dev/memory.py --binary target/release/Streamerz --out /tmp/memory-burst \
  --messages 25000 --cases unique
python3 dev/memory.py --binary target/release/Streamerz --out /tmp/memory-paced \
  --messages 10000 --cases unique --rate 1000
```

For the controlled matrix, always use `--env STREAMERZ_MEMORY_WRITE=buffered` and add these settings:

| Variant | Additional `--env` values |
|---|---|
| Full maps, copied saves | `STREAMERZ_MEMORY_RECORDS=map`, `STREAMERZ_MEMORY_SAVE=legacy` |
| Full maps, borrowed saves | `STREAMERZ_MEMORY_RECORDS=map` |
| Packed records, copied saves | `STREAMERZ_MEMORY_SAVE=legacy` |
| Packed records, borrowed saves | None |
| Deflated records, borrowed saves | `STREAMERZ_MEMORY_RECORDS=deflate` |

Repeat with no switches for the default streaming writer. Restart, sustained SSD and UI tests:

```sh
mkdir -p target/memory-disk
TMPDIR="$PWD/target/memory-disk" python3 dev/memory.py --binary target/release/Streamerz \
  --out /tmp/memory-sustained --messages 60000 --cases repeat --rate 1000 \
  --seed /tmp/memory-burst/unique/community.json --sync-storage
python3 dev/memory.py --binary target/release/Streamerz --out /tmp/memory-ui \
  --messages 1000 --cases repeat --seed /tmp/memory-burst/unique/community.json --visit-community
```

Choose message counts divisible by 50 so the final normal periodic save contains the whole workload;
the harness rejects other counts instead of mistaking expected unsaved changes for lost data.

The harness samples `/proc` externally every 200 ms plus phase checkpoints, recording RSS, PSS, USS,
swap, CPU ticks, threads, high-water RSS and process I/O. Acknowledgements measure end-to-end test
request latency, including periodic saves. Sampling can miss instantaneous spikes; the kernel high-water
counter is recorded separately. A single short trial is not a percentile guarantee for every disk.

`/tmp/streamerz-memory-experiments/` contains logs, phase samples, timelines, synthetic saved files,
screenshots and traces. Those files are temporary; the machine-readable result summary in this repo
is durable and contains no per-user data. The version is left open; nothing was pushed or released.
