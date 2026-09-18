# Changelog

All notable changes to Everyseek are recorded here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) (`MAJOR.MINOR.PATCH`).

## 0.4.9 — 2026-09-19

- Clarify the distribution's license boundary by authorship: all copyrightable Everyseek additions and modifications owned by Irurilabs Corp. use Everyseek License 1.0, including work made before this release.
- Preserve MIT for original inherited EverythingMac material and separate terms for third-party dependencies. MIT is not an alternative license for Everyseek Contributions in this distribution.
- Replace the combined legacy notice with the exact upstream MIT text and add a source allocation guide. Update the app's bundled notices, README, and website accordingly.
- This correction does not claim to revoke any separately acquired, valid prior license rights.

## 0.4.8 — 2026-09-19

- Introduce initial source-available terms permitting free personal and internal company use and requiring permission for monetization. The contribution scope is clarified in v0.4.9.
- Retain upstream attribution and dependency licenses; revise their allocation in v0.4.9.
- Bundle project and dependency notices inside the app.
- Update the website, README, captions, and launch film to describe the current licensing accurately.

## 0.4.7 — 2026-09-19

- Put sorting and search options in the built-in View menu, removing the duplicate empty View menu.
- Keep the DMG installer limited to Everyseek.app and the Applications shortcut; license notices remain bundled inside the app.

## 0.4.6 — 2026-09-18

- Show containing folders in the Path column, with middle truncation and complete-path tooltips.
- Move unavailable-folder controls into the status bar. Open the count to inspect individual errors and immediately add a folder to the persisted exclusion list.
- Add the requested system, temporary, mounted-volume and current-user Library default exclusions. Merge missing defaults once for existing settings while preserving custom text and later removals.
- Read unavailable folders through an independent SQLite snapshot; reuse the existing exclusion purge and retry workflow.

## 0.4.5 — 2026-09-18

- Persist the indexed-object count with file mutations; remove full-table recounts at startup and after configuration.
- Migrate size/time indexes to match their complete sort order. Fetch a bounded page before directory joins for empty name/size/time/kind searches, preserving exact matching and pagination.
- Run schema migration and initial count once, then use the existing index on restart. Schema v2 prevents older writers from invalidating the persisted count.
- Limit retained WAL size after reset without changing the checkpoint threshold or FULL durability.
- Verify existing regression tests, real-index query plans, page ordering and startup I/O. See docs/READ-FIX-0.4.5.md.

## 0.4.4 — 2026-09-18

- Preserve the original Exclude editor text across Apply, reopening and restart; share one editable path list with Volumes, including removal.
- Normalize exclusion names/paths to NFC for comparison while preserving original filesystem paths and user text.
- Purge excluded directory subtrees and FTS rows directly from SQLite before reconciliation, even if their parent cannot be read. Publish the changed object count and search revision immediately.
- Keep unrelated failure cooldowns when applying rules; unchanged rules do not queue another full scan. Surface applying/success/error state in Settings.
- Extend the existing compact exclusion test and verify the actual Settings UI Apply/remove/restart flow. See docs/SETTINGS-FIX-0.4.4.md.

## 0.4.3 — 2026-09-18

- Check name/path/hidden exclusions before reading child metadata, so skipped entries cannot turn their parent into an access failure.
- Remove excluded or out-of-scope failure records and retry jobs on startup, configuration and manual retry. Clear unindexed descendant jobs when removing an indexed subtree.
- Honor explicit path exclusions for configured roots. Keep genuine read failures visible and preserve their indexed records.
- Add one compact exclusion regression alongside the existing three tests. See docs/EXCLUSION-FIX-0.4.3.md.

## 0.4.2 — 2026-09-18

- Keep incremental indexing and background result refresh quiet; show delayed progress only for user searches and full reconciliation.
- Reduce the idle Desktop/Documents/Downloads safety sweep from 30 seconds to five minutes. Normal FSEvents updates stay immediate.
- Preserve failed-directory retry deadlines when ordinary events arrive; retry after five minutes, then hourly for persistent failures. Manual Retry remains immediate.
- Keep both /var and /private/var spellings (also /tmp and /etc) in path exclusions so Foundation normalization does not bypass them.
- Reuse the SQLite DB; corrected exclusion rules can trigger one reconciliation. See docs/ACTIVITY-FIX-0.4.2.md.

## 0.4.1 — 2026-09-18

- Reduce excessive disk writes by batching directory transactions, avoiding unchanged updates and reducing WAL checkpoint frequency while retaining FULL durability.
- Avoid refreshing searches for scans that found no record changes.
- Interpret paths pasted into exclusion names as path exclusions, expand ~/, and allow exclusion changes during indexing.
- Fix Swift/Rust VCS exclusion option compatibility.
- Keep the existing SQLite index. See docs/WRITE-FIX-0.4.1.md for before/after disk-write measurements.

## 0.4.0 — Team build — 2026-09-18

- Preserve the native UI and replace the Swift in-memory core with Rust + SQLite WAL.
- Add NFC-normalized filename unigram/bigram inverted search, exact candidate verification and cancellable reads. No initial-consonant expansion.
- Persist directory jobs and FSEvents checkpoints atomically; reconcile creates, metadata, moves and deletes incrementally and resume unfinished work after restart.
- Surface permission/index errors with retry; retain records on access failure.
- Add universal Developer ID signing, optional notarization, checksums, bundled notices and team installation instructions.
- Leave the legacy binary cache intact and build the new SQLite index once.
- Add three compact regression tests. See docs/VALIDATION.md for measured checks and limitations.

## [0.3.0] - 2026-06-24

### Added

- Menu bar commands. The File, Edit, View, and Help menus now drive search focus (⌘F), sorting, and index rebuild, and a new File ▸ Export writes the current results to a tab-separated file.
- Reworked Settings, split into General, Search, Exclude, and Volumes tabs — with live search preferences, index stats, launch-at-login, a rebuild button, and per-volume and per-folder exclusion controls.
- Match Case and Match Whole Word search options, plus a configurable result limit (it was a fixed 5,000 rows). These, along with the sort order, now persist across launches.
- Exclude the Trash by default (with a toggle), custom excluded folder names, and excluded file patterns such as `*.tmp` or `Thumbs.db`.

### Fixed

- New files in your home folder — Desktop, Documents, Downloads, and the rest — now appear instantly instead of lagging minutes behind. The app watches the macOS Data volume directly, and sweeps the iCloud-backed Desktop/Documents/Downloads folders that don't emit timely filesystem events.
- The window no longer locks up after a minute or so of use. A coalesced "rescan everything under here" filesystem event used to trigger a synchronous walk of the entire disk on the same thread as search, pegging a CPU core and freezing the UI. Deep rescans are now incremental and yield to your searches, and a whole-volume overflow no longer re-walks the whole disk.
- The same folder no longer appears twice — once under `/Users/…` and again under `/System/Volumes/Data/…`. Live indexing was missing an exclusion and could index the Data volume's internal firmlink alias as a duplicate. That's fixed, and any index already carrying these duplicates is detected and rebuilt clean automatically on the next launch.

## [0.2.2] - 2026-06-23

### Fixed

- High CPU during normal use. The live-update watcher re-read a whole directory on every filesystem event, and its per-entry "is this new?" check was a linear scan — so the diff was quadratic, and a big busy folder (like a browser cache) pegged a CPU core. The diff is now linear, and directories whose entries didn't change are skipped entirely (a directory-mtime check), so idle CPU drops to near zero while new, renamed, and deleted files are still picked up.

## [0.2.1] - 2026-06-23

### Fixed

- Indexing no longer hangs when a network share is mounted. The scan used to descend into SMB/NFS shares under `/Volumes` and stall on slow network reads (one mounted share with millions of files froze it indefinitely). Network volumes are now skipped — local volumes only.

## [0.2.0] - 2026-06-23

### Added

- Skip developer folders by default. `node_modules`, `Pods`, `DerivedData`, `.gradle`, `.cargo`, `__pycache__`, `.venv` and similar are left out of the index. Generic names like `build`, `dist`, and `target` are skipped only inside an actual project (a folder that also holds a `Cargo.toml`, `package.json`, `.git`, etc.), so a personal folder you happen to name "build" still shows up in search. Toggle in Settings.
- Skip version-control folders (`.git`, `.hg`, `.svn`) by default, on a separate toggle.

### Fixed

- Runaway CPU that climbed the longer the app stayed open. It no longer re-searches the whole index on every background file change — only when something it indexes actually changes.
- Live updates are more reliable: new files and bulk changes (a `git clone`, an `npm install`) are picked up without thrashing, and deep coalesced filesystem events are no longer missed.
- The on-disk index cache is rebuilt when the exclusion rules that shaped it change, instead of serving a stale index.
- The app now reports its real version (it was hardcoded to `1.0`).

## [0.1.0] - 2026-06-05

### Added

- First public release. Instant filename and folder search across every mounted volume, updating as you type. FSEvents-backed live index, a binary on-disk cache for instant restarts, and right-click actions (Open, Open With, Reveal in Finder, Copy Path/Name, Move to Trash).

[0.3.0]: https://github.com/alesloa/everything-mac/releases/tag/v0.3.0
[0.2.2]: https://github.com/alesloa/everything-mac/releases/tag/v0.2.2
[0.2.1]: https://github.com/alesloa/everything-mac/releases/tag/v0.2.1
[0.2.0]: https://github.com/alesloa/everything-mac/releases/tag/v0.2.0
[0.1.0]: https://github.com/alesloa/everything-mac/releases/tag/v0.1.0
