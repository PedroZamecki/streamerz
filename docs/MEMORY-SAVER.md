# Aggressive Memory Saver measurements

Memory Saver is opt-in in Settings and applies after restart. It does not load or update
Community users or credits, store chat history, load the emote catalog, or download UI images.
The existing Community file is preserved. Commands, actions and connections remain enabled.
First Words still tracks users in session-only disk buckets and is reset on restart. Disk errors
are logged and disable First Words until it is reset; no false first-message events are generated.
Kick random-user actions also keep session identities in disk buckets, without retaining chat messages.
Those identities are cleared on channel change, clean exit, and the next start after an interruption.

Disabling chat history also prevents new user cards and emote catalog loads, and deactivates the
chat composers. This setting is separate from the Memory Saver policy.

## Measured traffic comparison

November 1, 2026, Linux, Intel Core i7-1355U, release-profile development binary. Both runs
started with the same synthetic 25,000-user file. They injected 60,000 messages from distinct
users at a target of 1,000 messages/second and made one additional First Words request per message.
The test verified First Words results and either checked every saved user count (normal mode) or
verified that the existing Community file was unchanged (Memory Saver). `/proc` sampling ran every
200 ms. The normal run opened Community afterward; the Memory Saver run visited Settings instead.

| Measurement | Normal | Memory Saver |
|---|---:|---:|
| Delivered messages per second | 999.70 | 999.95 |
| RSS at start | 31.70 MiB | 21.15 MiB |
| RSS after traffic and 15 seconds idle | 61.19 MiB | 21.21 MiB |
| CPU time during traffic | 27.79 s | 16.82 s |
| CPU percentage of one core | 46.30% | 28.03% |
| p99 acknowledgement including First Words | 13.27 ms | 1.38 ms |
| Logical bytes written during traffic | 9,554,128,700 | 888,890 |

Normal mode showed 99.09 MiB while Community displayed 60,000 rows and 75.00 MiB after hiding.
Its whole-trial sampled peak was 105.14 MiB. Memory Saver's whole-trial sampled peak, including
visiting Settings, was 40.16 MiB. Those UI phases differ and are not a controlled UI comparison.
Neither workload included real service connections, avatars, image downloads, or a large action queue.
First Words' disk storage uses filesystem allocation and page cache, which are not captured by process
RSS or equivalent to physical media writes. The test configuration was on Btrfs, not `/tmp` tmpfs.

The idle 15-second interval recorded zero extra process CPU ticks at 100 Hz resolution in both runs.
Memory Saver does not meet the earlier 1 to 2% of one core CPU target during this workload.

A separate unthrottled Memory Saver burst delivered 25,000 distinct synthetic users and checked
First Words for each in 5.11 seconds (4,887.87 messages/second). It retained 21.21 MiB RSS after
15 seconds idle and preserved the seeded Community file. This test also used a release-profile
binary and no real services or images.

The regular application smoke test passed, including one user-card interaction. It does not exercise
the requested repeated interactions across a large user set.

## Scope still to verify

These are short synthetic traffic tests. They do not demonstrate a years-long 100 MB RSS ceiling:
other owners include action queues, counters, globals, plugins, incoming payloads, platform caches,
and native UI/library allocations. A global cap would need explicit bounds and behavior under overload.
In particular, the requested long human-style test opening and interacting with every user card,
including real image payloads and repeated show/hide cycles, remains outstanding. Do not use these
results as a substitute for it. The original screenshot shows a visible fast-profile build with real
Twitch and Kick connections; the synthetic hidden-process numbers do not explain all of its RSS.

Reproduce with `dev/memory.py --help`. Raw process samples and screenshots for these runs are in
`/tmp/streamerz-saver-final` and `/tmp/streamerz-normal-final`; those paths are temporary and contain
synthetic data only. The durable measurements and binary hashes are in `memory-results.json`.
