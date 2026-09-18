import Foundation
import CoreServices
import IndexCore

actor IndexActor {
    private var core: RustIndex?
    private var rules = ExcludeRules.defaults
    private var watcher: FileEvents?
    private var eventTask: Task<Void, Never>?
    private var pollTask: Task<Void, Never>?
    private var lastMounts: [String] = []
    private var ticks = 0
    private var onStatus: (@Sendable (IndexStatus) -> Void)?
    private var onError: (@Sendable (String) -> Void)?

    init() {
        if let data = AppPreferences.store.data(forKey: "excludeRules"),
           let saved = try? JSONDecoder().decode(ExcludeRules.self, from: data) { rules = saved }
        if AppPreferences.store.integer(forKey: "defaultExclusionsVersion") < 1 {
            rules = rules.addingFolders(ExcludeRules.defaultFolderPaths)
        }
    }

    static var roots: [String] {
        if let raw = ProcessInfo.processInfo.environment["EVERYTHINGMAC_ROOTS_JSON"],
           let roots = try? JSONDecoder().decode([String].self, from: Data(raw.utf8)), !roots.isEmpty {
            return roots
        }
        return ["/"]
    }
    static func databaseURL() -> URL {
        if let path = ProcessInfo.processInfo.environment["EVERYTHINGMAC_DATA_DIR"] {
            return URL(fileURLWithPath: path).appendingPathComponent("index-v1.sqlite")
        }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Everything-Mac", isDirectory: true).appendingPathComponent("index-v1.sqlite")
    }
    func currentRules() -> ExcludeRules { rules }

    func startUp(onStatus: @escaping @Sendable (IndexStatus) -> Void,
                 onError: @escaping @Sendable (String) -> Void) async throws {
        guard core == nil else { return }
        self.onStatus = onStatus
        self.onError = onError
        let baseline = FSEventsGetCurrentEventId() // BEFORE starting the initial walk
        let url = Self.databaseURL()
        let engine = try await Task.detached(priority: .utility) { try RustIndex(databaseURL: url) }.value
        let effective = effectiveRules()
        try await Task.detached(priority: .utility) {
            try engine.configure(roots: Self.roots, rules: effective, baseline: baseline, rebuild: false)
        }.value
        core = engine
        let (stream, continuation) = AsyncStream<FileEventBatch>.makeStream()
        // One ordered consumer. A later event batch must never advance the
        // persisted cursor before an earlier batch has reached the durable inbox.
        eventTask = Task.detached(priority: .utility) {
            for await batch in stream {
                while !Task.isCancelled {
                    do {
                        try engine.enqueue(paths: batch.paths, cursor: batch.cursor, reset: batch.reset)
                        break
                    } catch {
                        onError(error.localizedDescription)
                        try? await Task.sleep(for: .seconds(2))
                    }
                }
                if Task.isCancelled { break }
            }
        }
        let watcher = FileEvents { continuation.yield($0) }
        self.watcher = watcher
        lastMounts = Self.localMounts()
        do {
            try watcher.start(paths: watchPaths(), since: engine.status().inboxCursor)
        } catch {
            eventTask?.cancel()
            self.watcher = nil
            self.core = nil
            throw error
        }
        // Commit the one-time default merge only after the engine accepted it.
        AppPreferences.store.set(try JSONEncoder().encode(rules), forKey: "excludeRules")
        AppPreferences.store.set(1, forKey: "defaultExclusionsVersion")
        onStatus(try engine.status())
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled else { return }
                await self?.poll()
            }
        }
    }

    private func poll() async {
        guard let core else { return }
        do {
            let status = try core.status()
            onStatus?(status)
            ticks += 1
            if ticks % 30 == 0 {
                let mounts = Self.localMounts()
                if mounts != lastMounts {
                    lastMounts = mounts
                    let effective = effectiveRules()
                    try await Task.detached(priority: .utility) {
                        try core.configure(roots: Self.roots, rules: effective, baseline: 0, rebuild: false)
                        try core.enqueue(paths: ["/Volumes": true], cursor: 0)
                    }.value
                    try watcher?.start(paths: watchPaths(), since: core.status().inboxCursor)
                }
            }
            // FSEvents handles normal changes. Keep only an infrequent idle
            // fallback for FileProvider folders that omit timely events.
            if ticks % 300 == 0 && status.pending == 0 {
                let home = FileManager.default.homeDirectoryForCurrentUser.path
                let paths = Dictionary(uniqueKeysWithValues: ["Desktop", "Documents", "Downloads"].map { (home + "/" + $0, false) })
                try await Task.detached(priority: .utility) { try core.enqueue(paths: paths, cursor: 0) }.value
            }
        } catch { onError?(error.localizedDescription) }
    }

    func search(_ text: String, matchPath: Bool, caseInsensitive: Bool, wholeWord: Bool,
                sort: QueryEngine.SortKey, ascending: Bool, limit: Int, background: Bool = false) async throws -> SearchPage {
        guard let core else { throw CoreError(message: "The index is starting.") }
        return try await Task.detached(priority: background ? .utility : .userInitiated) {
            try core.search(text, matchPath: matchPath, caseInsensitive: caseInsensitive,
                            wholeWord: wholeWord, sort: sort, ascending: ascending, limit: limit)
        }.value
    }
    func cancelSearch() { core?.cancelSearch() }

    func unavailableFolders() async throws -> [UnavailableFolder] {
        guard let core else { throw CoreError(message: "The index is starting.") }
        return try await Task.detached(priority: .utility) { try core.unavailableFolders() }.value
    }

    func rescanAll() async throws {
        guard let core else { return }
        let effective = effectiveRules()
        try await Task.detached(priority: .utility) {
            try core.configure(roots: Self.roots, rules: effective, baseline: 0, rebuild: true)
        }.value
        onStatus?(try core.status())
    }
    func setRules(_ newRules: ExcludeRules) async throws {
        guard let core else { throw CoreError(message: "The index is starting.") }
        let data = try JSONEncoder().encode(newRules)
        let effective = effectiveRules(for: newRules)
        try await Task.detached(priority: .utility) {
            try core.configure(roots: Self.roots, rules: effective, baseline: 0, rebuild: false)
        }.value
        rules = newRules
        AppPreferences.store.set(data, forKey: "excludeRules")
        onStatus?(try core.status())
    }
    func retry() async throws {
        guard let core else { return }
        try await Task.detached(priority: .utility) { try core.retry() }.value
    }
    func flush() async {
        guard let core else { return }
        do { try await Task.detached(priority: .utility) { try core.checkpoint() }.value }
        catch { onError?(error.localizedDescription) }
    }

    private func effectiveRules(for source: ExcludeRules? = nil) -> ExcludeRules {
        var effective = source ?? rules
        effective.pathPrefixes += ["/System/Volumes", "/dev", Self.databaseURL().deletingLastPathComponent().path]
        effective.pathPrefixes += Self.mounts(local: false)
        return effective.resolvingPaths()
    }
    private func watchPaths() -> [String] {
        if Self.roots != ["/"] { return Self.roots }
        return ["/", "/System/Volumes/Data"] + lastMounts.filter { $0.hasPrefix("/Volumes/") }
    }
    private static func localMounts() -> [String] { mounts(local: true) }
    private static func mounts(local: Bool) -> [String] {
        var buffer: UnsafeMutablePointer<statfs>?
        let count = getmntinfo(&buffer, MNT_NOWAIT)
        guard count > 0, let buffer else { return [] }
        var result: [String] = []
        for i in 0..<Int(count) {
            var fs = buffer[i]
            guard ((fs.f_flags & UInt32(MNT_LOCAL)) != 0) == local else { continue }
            let path = withUnsafePointer(to: &fs.f_mntonname) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXPATHLEN)) { String(cString: $0) }
            }
            result.append(path)
        }
        return result.sorted()
    }
}
