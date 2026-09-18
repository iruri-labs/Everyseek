# Performance: a real 1.28-million-item index

Measured September 19, 2026. This is a single-machine observation, not a guarantee for every query or Mac.

## Search results

- **1,278,913 indexed items:** 1,153,005 files and 125,908 folders from an existing everyday-use index.
- **Up to 500 results** per request, ordered by filename ascending.
- **12.25 ms median** across 300 requests: ten query strings, 30 measured requests each.
- Overall **p95: 849.36 ms**, maximum **1,152.94 ms**. A broad one-letter query dominates the slow end of the distribution.
- Apple M4 Pro, 24 GiB unified memory, macOS 26.5.
- Release Rust engine **v0.4.9**, source commit `78faf0efc30c20d51544c16ea94568690eb96b13`.
- Binary SHA-256: `73a91189a06f729431eafb21bf6db23a3427e425462073bfa024f547303def15`.

| Query | Returned rows | Median | p95 |
| --- | ---: | ---: | ---: |
| `readme` | 500 (more matches) | 9.11 ms | 14.96 ms |
| `config` | 500 (more matches) | 25.62 ms | 40.43 ms |
| `report` | 500 (more matches) | 13.66 ms | 17.95 ms |
| `.pdf` | 500 (more matches) | 12.64 ms | 14.46 ms |
| `.png` | 500 (more matches) | 61.38 ms | 66.58 ms |
| `계획` | 240 | 1.86 ms | 3.15 ms |
| `보고서` | 176 | 1.52 ms | 2.20 ms |
| `정산` | 5 | 0.49 ms | 0.78 ms |
| `a` | 500 (more matches) | 850.78 ms | 941.42 ms |
| `zzzxq_no_match_20260919` | 0 | 3.90 ms | 4.61 ms |

The 500-row cap limits the returned page, not all work performed. Exact filename matching runs before sorting and pagination, so a query such as `a` can inspect many candidates even though only 500 rows are returned. The aggregate gives each query the same number of samples; it is not weighted by observed user search frequency.

## What the stopwatch measures

The benchmark uses the release `everything-index` CLI, which calls the same `Engine::request` / search implementation as the app's Rust bridge. Each measurement starts immediately before writing a complete JSON request and ends after reading the complete JSON response. It includes pipe transport, query parsing, SQLite search, result construction and JSON serialization; Python response decoding occurs after the timer stops.

It **does not measure keystroke-to-screen latency**. The app adds a 40 ms input debounce, Swift bridging and table rendering. CLI startup, the index snapshot and warm-up are also outside the timed region. Computer Use separately confirmed that the installed app returned 500+ `readme` results and 240 `계획` results. Automation-tool wall time was not used as search latency.

Search options: case-insensitive filename substring matching, Match Path off, Whole Word off, name ascending, offset 0, limit 500. Strings such as `.pdf` are literal substring searches, not a separate file-type filter.

## Snapshot and cache conditions

The Python script opens the live database read-only and creates a consistent snapshot with SQLite's backup API. Only this temporary snapshot is changed: its stored scan configuration and pending directory jobs are removed to prevent background scanning. Filename rows, metadata, search postings and sort indexes are preserved. The running app's database and preferences are not changed by the script.

One untimed pass over all ten queries warms the cache. Thirty measured rounds follow, each with a shuffled query order and a fixed seed of 20260919. The script checks the engine's object count before and after, the result cap, and stable returned row counts / `hasMore` values. This is a **warm OS-cache** benchmark; there is no cold-cache claim. Other applications and the installed app remained running.

Median is the middle value (the mean of the two middle values for an even sample count). p95 uses the nearest-rank method. The [JSON evidence](benchmark-2026-09-19.json) records all 300 timings, query strings, row counts, options, hardware and source/binary identifiers. It contains no matched filenames or paths.

## Resource footprint

Resource observations are separate from the isolated CLI timing run.

| Metric | Evidence and interpretation |
| --- | --- |
| RAM under 300 MB | Eight `top` samples of the installed Everyseek app showed **243–261 MiB**, approximately **255–274 MB**. This supports the observed footprint, not a hard memory ceiling. |
| CPU 1.7% | **User-reported usage snapshot.** It was not reproduced as a sustained-search average. In our separate verification sample, CPU ranged from **34.4% to 96.7%** after the first `top` interval; indexing was visible during the session. CPU varies with searching, indexing and other activity. |
| Index around 700 MB | Main database **687,067,136 bytes**, WAL **33,554,432 bytes**, shared memory **98,304 bytes**; total **720,719,872 bytes = 720.72 MB**. These are the existing files' logical sizes, including allocated database free pages; no compaction was performed. |

The installed app used for UI and resource observations reported **v0.4.7, build 16**; the benchmark CLI was built from v0.4.9 source. These are explicitly separate observations. The published 1.7% figure comes from the user's own observation, whose sampling interval was not supplied. It should not be read as simultaneous CPU usage during the 300 benchmark requests.

MB means 1,000,000 bytes; MiB means 1,048,576 bytes; GiB means 1,073,741,824 bytes. Database/WAL size can change as the index updates.

## Reproduce

From the repository root, with Rust and Python 3 installed:

```bash
cargo build --manifest-path Core/Cargo.toml --release --locked --bin everything-index
python3 scripts/benchmark-search.py \
  --database "$HOME/Library/Application Support/Everything-Mac/index-v1.sqlite" \
  --output /tmp/everyseek-benchmark.json
```

The installed app may stay open: the CLI receives only a temporary snapshot, never the live database. The temporary directory is removed after the run. The default is 30 rounds per query; `--rounds` changes that count. Use the same options and query list when comparing runs.
