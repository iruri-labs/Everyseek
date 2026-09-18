import SwiftUI
import IndexCore
import Combine

@MainActor
final class AppModel: ObservableObject {
    @Published var query = ""
    @Published var results: [FileRecord] = []
    @Published var total = 0
    @Published var sortKey: QueryEngine.SortKey = .name
    @Published var ascending = true
    @Published var matchPath = false
    @Published var caseSensitive = false
    @Published var wholeWord = false
    @Published var resultLimit = 5000
    @Published var rules: ExcludeRules = .defaults
    @Published var applyingRules = false
    @Published var rulesApplyMessage: String?
    @Published var scanning = false
    @Published var rebuilding = false
    @Published var searching = false
    @Published var hasMore = false
    @Published var inaccessible = 0
    @Published var unavailableFolders: [UnavailableFolder] = []
    @Published var loadingUnavailable = false
    @Published var unavailableError: String?
    @Published var retryingIndex = false
    @Published var errorMessage: String?
    @Published var selectedID: UInt64?
    @Published var focusSearchSignal = 0

    var selected: FileRecord? { selectedID.flatMap { id in results.first { $0.id == id } } }
    let index = IndexActor()
    private var task: Task<Void, Never>?
    private var searchSeq = 0
    private var started = false
    private var revision: UInt64?
    private var pendingRefresh = false
    private var searchInFlight = false

    func bootstrap() {
        guard !started else { return }
        started = true
        loadPrefs()
        scanning = true
        rebuilding = true
        Task {
            do {
                try await index.startUp(onStatus: { [weak self] status in
                    Task { @MainActor in self?.updateStatus(status) }
                }, onError: { [weak self] message in
                    Task { @MainActor in self?.errorMessage = message }
                })
                rules = await index.currentRules()
            } catch {
                scanning = false
                rebuilding = false
                errorMessage = error.localizedDescription
                started = false
            }
        }
    }

    private func updateStatus(_ status: IndexStatus) {
        total = status.total
        scanning = status.pending > 0
        rebuilding = status.rebuilding
        inaccessible = status.inaccessible
        if let error = status.error { errorMessage = error }
        if revision != status.revision {
            let firstSearch = revision == nil
            revision = status.revision
            // Do not cancel a user's active search for background metadata churn.
            if searchInFlight { pendingRefresh = true } else { scheduleSearch(background: !firstSearch) }
        }
    }

    func queryChanged() { savePrefs(); scheduleSearch() }
    func searchOptionsChanged() { savePrefs(); scheduleSearch() }
    func setSort(_ key: QueryEngine.SortKey, ascending asc: Bool) {
        sortKey = key; ascending = asc; savePrefs(); scheduleSearch()
    }

    private func scheduleSearch(background: Bool = false) {
        searchSeq &+= 1
        task?.cancel()
        searchInFlight = true
        searching = !background
        task = Task {
            await index.cancelSearch()
            try? await Task.sleep(for: .milliseconds(background ? 500 : 40))
            guard !Task.isCancelled else { return }
            await runSearch(background: background)
        }
    }

    private func runSearch(background: Bool) async {
        searchSeq &+= 1
        let sequence = searchSeq
        searchInFlight = true
        searching = !background
        pendingRefresh = false
        do {
            let page = try await index.search(query, matchPath: matchPath, caseInsensitive: !caseSensitive,
                                              wholeWord: wholeWord, sort: sortKey, ascending: ascending,
                                              limit: resultLimit, background: background)
            guard sequence == searchSeq, !Task.isCancelled else { return }
            results = page.rows
            hasMore = page.hasMore
            searchInFlight = false
            searching = false
            if pendingRefresh { pendingRefresh = false; scheduleSearch(background: true) }
        } catch {
            guard sequence == searchSeq, !Task.isCancelled else { return }
            searchInFlight = false
            searching = false
            errorMessage = error.localizedDescription
        }
    }

    func focusSearch() { focusSearchSignal &+= 1 }

    // MARK: - Search preference persistence (UserDefaults)

    func loadPrefs() {
        let d = AppPreferences.store
        matchPath = d.bool(forKey: "pref.matchPath")
        caseSensitive = d.bool(forKey: "pref.caseSensitive")
        wholeWord = d.bool(forKey: "pref.wholeWord")
        let lim = d.integer(forKey: "pref.resultLimit")
        resultLimit = lim > 0 ? lim : 5000
        if let sk = d.string(forKey: "pref.sortKey") { sortKey = Self.sortKey(from: sk) }
        if d.object(forKey: "pref.ascending") != nil { ascending = d.bool(forKey: "pref.ascending") }
    }

    func savePrefs() {
        let d = AppPreferences.store
        d.set(matchPath, forKey: "pref.matchPath")
        d.set(caseSensitive, forKey: "pref.caseSensitive")
        d.set(wholeWord, forKey: "pref.wholeWord")
        d.set(resultLimit, forKey: "pref.resultLimit")
        d.set(Self.sortKeyName(sortKey), forKey: "pref.sortKey")
        d.set(ascending, forKey: "pref.ascending")
    }

    // Preserve the existing preference keys across the engine migration.
    static func sortKeyName(_ k: QueryEngine.SortKey) -> String {
        switch k {
        case .name:  return "name"
        case .path:  return "path"
        case .size:  return "size"
        case .mtime: return "mtime"
        case .kind:  return "kind"
        }
    }
    static func sortKey(from s: String) -> QueryEngine.SortKey {
        switch s {
        case "path":  return .path
        case "size":  return .size
        case "mtime": return .mtime
        case "kind":  return .kind
        default:      return .name
        }
    }


    func rebuildIndex() {
        guard !scanning else { return }
        errorMessage = nil
        Task {
            do { try await index.rescanAll() }
            catch { errorMessage = error.localizedDescription }
        }
    }

    func applyRules(_ newRules: ExcludeRules) {
        guard !applyingRules else { return }
        applyingRules = true
        rulesApplyMessage = nil
        errorMessage = nil
        Task {
            defer { applyingRules = false }
            do {
                try await index.setRules(newRules)
                rules = await index.currentRules()
                scheduleSearch(background: true)
                rulesApplyMessage = "Exclusions applied."
                await refreshUnavailableFolders()
            } catch { errorMessage = error.localizedDescription }
        }
    }

    func refreshUnavailableFolders() async {
        guard !loadingUnavailable else { return }
        loadingUnavailable = true
        defer { loadingUnavailable = false }
        do {
            let folders = try await index.unavailableFolders()
            guard !Task.isCancelled else { return }
            unavailableFolders = folders
            unavailableError = nil
        } catch {
            if !Task.isCancelled { unavailableError = error.localizedDescription }
        }
    }

    func excludeUnavailableFolder(_ folder: UnavailableFolder) {
        applyRules(rules.addingFolders([folder.path]))
    }

    func retryIndex() {
        guard !retryingIndex else { return }
        errorMessage = nil
        if !started { bootstrap(); return }
        retryingIndex = true
        Task {
            defer { retryingIndex = false }
            do {
                try await index.retry()
                await refreshUnavailableFolders()
            } catch { errorMessage = error.localizedDescription }
        }
    }
}
