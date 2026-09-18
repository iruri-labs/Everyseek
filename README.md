<p align="center">
  <img src="logo/everyseek.png" alt="Everyseek app icon" width="96" height="96">
</p>

<h1 align="center">Everyseek</h1>
<p align="center"><strong>Your files. Within reach.</strong></p>
<p align="center">Lightweight file search for Mac, with an index of its own.</p>

<p align="center">
  <a href="https://github.com/iruri-labs/Everyseek/releases/download/v0.4.9/Everyseek-0.4.9-universal.dmg"><strong>Download for Mac</strong></a>
  &nbsp; · &nbsp;
  <a href="https://iruri-labs.com/Everyseek/">Website</a>
  &nbsp; · &nbsp;
  <a href="https://iruri-labs.com/Everyseek/#film">Watch the 22-second intro</a>
</p>

[![Everyseek — Your files. Within reach. Watch the introduction.](site/assets/launch-poster.jpg)](https://iruri-labs.com/Everyseek/#film)

<p align="center">macOS 14+ &nbsp; · &nbsp; Apple Silicon + Intel &nbsp; · &nbsp; Developer ID signed &amp; Apple notarized</p>

## A small utility for a big filesystem

Everyseek builds a dedicated local filename index, independent of Spotlight. Open it, type a name, and see matching files with their containing folders.

| An index of its own                                                                  | Built for everyday use                                                                 |
| ------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| **Independent of Spotlight** — search uses Everyseek's own Rust + SQLite index.      | **Native Mac interface** — filenames, folders, size, kind and familiar Finder actions. |
| **Saved between launches** — reuse the existing index while unfinished work resumes. | **Korean filename search** — ordinary syllable matching across NFC/NFD forms.          |
| **Incremental updates** — file changes update the index without starting over.       | **You choose the scope** — editable exclusions and per-folder retry controls.          |

Everyseek focuses on **filenames and paths**, not document contents. The index stays on your Mac. Access depends on macOS permissions and your chosen exclusions.

## A million files. A remarkably small footprint.

**1.28 million indexed files and folders. 12 ms median search. Under 300 MB of observed RAM.** Everyseek keeps a large filesystem within reach while leaving room for the rest of your work.

| Search benchmark | Result |
| --- | --- |
| Index searched | **1,278,913 items** — 1,153,005 files + 125,908 folders |
| Results per request | **Up to 500**, sorted by name |
| Median response | **12.25 ms** across 300 requests / 10 query strings |
| Example searches | `readme` **9.11 ms** · `.pdf` **12.64 ms** · `계획` **1.86 ms** |

| Small resource footprint | Observed value |
| --- | --- |
| RAM | **Under 300 MB** — 255–274 MB in the running-app sample |
| CPU | **1.7%** in a user-reported usage snapshot |
| Complete on-disk index | **About 700 MB** — 721 MB including SQLite WAL and shared-memory files |

Measured on **Apple M4 Pro / 24 GiB / macOS 26.5**, September 19, 2026. Search numbers measure the release Rust engine through its CLI, with a warm cache and a 500-result cap; they exclude the app's 40 ms input debounce and screen rendering. Query selectivity matters: the broad single-letter query `a` took **851 ms median**, and overall p95 was **849 ms**. RAM and CPU are separate usage observations, not search-time limits; CPU rose during indexing in the verification session.

[Full method, every query and resource context](docs/PERFORMANCE.md) · [Raw timing samples](docs/benchmark-2026-09-19.json) · [Reproduce the benchmark](scripts/benchmark-search.py)

## Download and install

1. [**Download the notarized DMG**](https://github.com/iruri-labs/Everyseek/releases/download/v0.4.9/Everyseek-0.4.9-universal.dmg).
2. Open it and drag **Everyseek** into **Applications**.
3. Launch Everyseek. Allow Full Disk Access in **System Settings → Privacy & Security → Full Disk Access** when prompted to index protected folders.
4. Let the initial index finish. Later launches reuse your saved index.

Requires **macOS 14 or later**. One universal download supports **Apple Silicon and Intel**.

[Release notes, ZIP and SHA-256 checksums](https://github.com/iruri-labs/Everyseek/releases/latest) · [Report an issue](https://github.com/iruri-labs/Everyseek/issues)

## Build and technical notes

<details>
<summary>Architecture, build instructions, diagnostics, and upgrade behavior</summary>

## Architecture

- **SwiftUI/AppKit**: search input, result table, Finder/open/trash actions and permissions UI.
- **Swift FSEvents adapter**: ordered change batches with persistent event cursors.
- **Rust static library**: directory reconciliation, exclusions, search and cancellation through a small C ABI.
- **SQLite WAL**: file metadata, original names/paths, persistent scan queue, event checkpoints and filename inverted index in one database. No server or runtime installation.

Filename indexing uses NFC-normalized, lowercase Unicode scalar **unigrams + bigrams**, encoded as ASCII terms in FTS5. This supports one- and two-character Korean searches without FTS5 trigram's short-query limitation. Candidate intersections are verified against the real normalized filename **before sorting and pagination**. Original paths remain unchanged. Initial-consonant expansion (초성), body search, fuzzy correction and separator-ignoring matching are not enabled.

Startup reads a persisted object count rather than recounting the files table. File-count deltas commit atomically with each scan batch, root change and exclusion purge. Schema v2 seeds this count once and replaces the old size/time indexes with indexes matching the full sort order. Empty searches for name, size, modification time and kind fetch only the requested page plus one row before resolving directory paths; full-path sorting and nonselective filtered searches may still inspect the full candidate set.

The writer groups up to 128 directories per transaction, stopping between directories after 100 ms or 8,000 SQLite row changes. Child jobs, file/FTS changes and job completion commit together. The writer uses a 64 MiB page cache, a 32 MiB automatic WAL-checkpoint threshold and a 32 MiB retained-WAL limit after reset (active readers may temporarily prevent shrinkage); FULL synchronous durability remains enabled. Unchanged directory rows are not rewritten, and unchanged scans do not refresh the UI. FSEvents cursor advancement and queued changes commit together. On restart, searches use the existing DB immediately while unfinished jobs resume and FSEvents replays from the durable inbox cursor. Dropped events schedule recursive reconciliation. Permission failures retain records and retry; only confirmed absence/removal deletes indexed entries. Searches read independent WAL snapshots and can be cancelled.

Default scope is the local filesystem, excluding `/Library`, `/System`, `/Volumes`, `/private/tmp`, `/private/var`, `/private/var/folders`, `/tmp`, `/var`, `/var/folders`, `/Users/Deleted Users` and the current user's `Library` folder. These visible, editable defaults are merged into existing settings once; later user removals are respected. Remove `/Volumes` from the exclusion list to include mounted local volumes. Network volumes, system data aliases, device files and the index's own directory are excluded. Symlinks are indexed without traversal. Full-path queries and wildcard queries with no literal characters require a scan. FileProvider remote inventories are not guaranteed.

Incremental changes refresh results quietly; progress is reserved for full reconciliation and user searches lasting more than 250 ms. FSEvents remains the normal update source, with an idle five-minute shallow safety sweep of Desktop/Documents/Downloads. Inaccessible directories retry after five minutes, then hourly while failures persist. Ordinary events preserve this cooldown; manual Retry overrides it. Path exclusions include both macOS /private aliases and their short spellings.

The Path column shows only the containing folder, with middle truncation and a full-path tooltip. Click the orange unavailable-folder count in the bottom status bar to inspect paths and errors, then use **Add to excluded folder list** for an individual folder. The adjacent **Retry** button retries access failures. Excluding a folder persists the setting and removes its index entries and retry jobs; it does not delete files from disk.

Source layout:

| Path                              | Role                                           |
| --------------------------------- | ---------------------------------------------- |
| App/Sources                       | Native UI and FSEvents adapter                 |
| Sources/IndexCore/RustIndex.swift | Swift JSON/C ABI bridge                        |
| Sources/CEverythingCore           | C header and SwiftPM module                    |
| Core/src                          | Rust engine, SQLite, reconciliation and search |
| Core/tests/engine.rs              | Four compact regression tests                  |
| scripts                           | Build, signing and packaging                   |

## Build

Requires Xcode with Swift 6, Rust 1.93 or newer, and XcodeGen (`brew install xcodegen`). Tested with Xcode 26.5 / Swift 6.3.2 and Rust 1.93.1.

```bash
./scripts/build-dev.sh
open .build/xcode/Build/Products/Release/Everyseek.app
```

The script builds Rust, generates the Xcode project and links a self-contained Release app. Default signing is ad hoc. It does not replace the installed application; set `INSTALL=1` to copy it to /Applications.

For stable team signing:

```bash
DEVELOPER_ID='Developer ID Application: Your Company (TEAMID)' ./scripts/build-dev.sh
```

For universal distribution, install both Rust targets once:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
DEVELOPER_ID='Developer ID Application: Your Company (TEAMID)' ./scripts/build-dmg.sh
```

Outputs versioned DMG, ZIP and SHA-256 checksums under `dist/`. Without `NOTARY_PROFILE`, filenames explicitly include `unnotarized`. To notarize/staple later, supply a stored notarytool Keychain profile:

```bash
DEVELOPER_ID='Developer ID Application: Your Company (TEAMID)' \
NOTARY_PROFILE='team-profile' ./scripts/build-dmg.sh
```

Never put signing passwords or API keys in the repository. The generated Xcode project is ignored; edit `App/project.yml`.

## Validation and diagnostics

```bash
cargo test --manifest-path Core/Cargo.toml --locked
cargo clippy --manifest-path Core/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path Core/Cargo.toml --release --locked --bin everything-index
Core/target/release/everything-index /tmp/example-index.sqlite --scan /path/to/folder
```

Without `--scan`, the CLI reads newline-delimited JSON requests on stdin, e.g. `{"op":"search","text":"계획","limit":100}` and `{"op":"status"}`. Do not open the app's DB concurrently with the CLI: one writer per database is enforced.

For an isolated native-app check, set `EVERYTHINGMAC_ROOTS_JSON='["/absolute/test/folder"]'`, `EVERYTHINGMAC_DATA_DIR=/absolute/test/database-directory` and `EVERYTHINGMAC_DEFAULTS_SUITE=com.everyseek.qa.example` when launching the executable. This separates test roots, database and preferences. Leave these unset for ordinary use.

The tests cover NFC/NFD and exact candidate filtering, metadata/move/delete/restart/event-loss recovery, permission failures, and exclusion/error cleanup.

## Data and upgrade

Everyseek uses the bundle identifier `com.everyseek.app`. On its first normal launch, it copies existing search preferences and exclusions from `com.everythingmac.app` once, preserving any settings already saved under the new identifier. The index stays at its existing location below. macOS privacy permissions and login-item registration may need to be granted again for the new app identity.

The new index is `~/Library/Application Support/Everything-Mac/index-v1.sqlite`. It is local, stores filenames/metadata only, and is not encrypted. The legacy `index.idx` is left intact; the first 0.4 launch builds a fresh SQLite index. Later launches resume incrementally. The first 0.4.5 launch migrates the database atomically to schema v2 without rescanning the filesystem; older app versions cannot open schema v2. Keep an old-version DB backup if rollback is required. Ordinary launches do not rebuild indexes or compact the database. Rebuild reconciles the existing DB without clearing the visible result set up front. Errors are surfaced rather than silently deleting the database.

Applying changed exclusions removes matching directory subtrees and FTS entries directly from SQLite before the remaining scope is reconciled. Exclusion text is stored separately from NFC-normalized engine rules; both settings tabs edit the same list.

</details>

## License

Everyseek uses the [Everyseek License 1.0](LICENSE). **Personal use and internal company use are free, including at for-profit businesses.** Monetizing the software or covered original/modified code requires **prior written permission from Irurilabs Corp.**

| Use                                                                                                                                               | Permission                                      |
| ------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| Search files for personal use or everyday work at a company                                                                                       | Free                                            |
| Deploy to employees or modify it for your company's internal operations                                                                           | Free; internal modifications may stay private   |
| Share original or modified copies without charge or monetization                                                                                  | Allowed with the required license and notices   |
| Sell the app or a modified version; include covered code in a paid product; offer paid hosting, SaaS, API access, or other monetized distribution | Separate written commercial permission required |

Your own files and work products are not covered merely because you find or manage them with Everyseek. Earning a salary or selling unrelated work does not require a commercial license.

[Request commercial permission](https://github.com/iruri-labs/Everyseek/issues). The complete [LICENSE](LICENSE) controls; this table is a summary. This is **source-available software**, not an OSI-approved open-source license.

**The license boundary follows authorship.** All copyrightable Everyseek additions and modifications owned by Irurilabs Corp. in this distribution use Everyseek License 1.0, regardless of when they were written. Original EverythingMac material inherited from upstream remains under its [MIT license](LICENSES/upstream-everything-mac-MIT.txt). In a mixed file, MIT covers the inherited original portions and Everyseek License 1.0 covers our copyrightable changes; MIT is not an alternative license for our contributions. See [NOTICE](NOTICE) and the [allocation guide](LICENSES/README.md). This notice does not revoke separately acquired, valid prior rights.

Copyright (c) 2026 Irurilabs Corp. Portions of this project are derived from [alesloa/everything-mac](https://github.com/alesloa/everything-mac), copyright (c) 2026 Alejandro Sloan. The original SwiftUI/AppKit foundation and upstream copyright notice are retained.

Rust dependency versions are pinned in `Core/Cargo.lock`; their upstream licenses remain applicable. Distribution builds include the project and dependency license notices.
