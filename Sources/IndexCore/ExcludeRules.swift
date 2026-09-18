import Foundation

public struct ExcludeRules: Sendable, Codable, Equatable {
    public var names: Set<String>
    public var pathPrefixes: [String]
    /// Original editor text; normalization is only for the engine's search copy.
    public var folderText: String?
    public var excludeHidden: Bool
    // When on, regenerable developer build/dependency directories are skipped on top
    // of the user's own `names`. Unambiguous names (see `devFolderNames`) are skipped
    // anywhere; generic English-word names (see `markerScopedDevFolderNames`) are only
    // skipped inside a directory that also holds a project marker, so a personal
    // ~/Documents/build isn't silently hidden by a tool whose whole job is recall.
    public var excludeDevFolders: Bool
    // Version-control internals (.git/.hg/.svn). Separate toggle from dev folders so a
    // user can re-index a build dir to find one file WITHOUT also pulling every churny
    // .git blob on disk back into the index.
    public var excludeVCSFolders: Bool
    // The Trash (~/.Trash and per-volume /.Trashes). A "deleted" file still turning up
    // in search confuses users — the point of deleting is for it to be gone — so Trash
    // is skipped by default; toggle off to search inside it.
    public var excludeTrash: Bool
    // File-name wildcard patterns (Everything's "Exclude files"). Matched against the
    // FILE name only (never directories) during the scan, so e.g. "*.tmp" or "*.log"
    // never enter the index. Empty by default — when empty the per-file check is a
    // single isEmpty branch, so the scan hot path pays nothing.
    public var excludeFilePatterns: [String]

    public init(names: Set<String> = [], pathPrefixes: [String] = [],
                excludeHidden: Bool = false, excludeDevFolders: Bool = true,
                excludeVCSFolders: Bool = true, excludeTrash: Bool = true,
                excludeFilePatterns: [String] = [], folderText: String? = nil) {
        self.names = names
        self.pathPrefixes = pathPrefixes
        self.folderText = folderText
        self.excludeHidden = excludeHidden
        self.excludeDevFolders = excludeDevFolders
        self.excludeVCSFolders = excludeVCSFolders
        self.excludeTrash = excludeTrash
        self.excludeFilePatterns = excludeFilePatterns
    }

    // Settings saved before excludeDevFolders/excludeVCSFolders existed lack those
    // keys. Decode each as ON when absent so upgrading enables skipping WITHOUT
    // discarding the user's custom names (a hard decode failure would reset rules to
    // defaults). The three original fields are still hard-decoded — identical to the
    // synthesized conformance this replaces, so no pre-existing blob decodes worse.
    private enum CodingKeys: String, CodingKey {
        case names, pathPrefixes, folderText, excludeHidden, excludeDevFolders, excludeVCSFolders, excludeTrash, excludeFilePatterns
    }
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        names = try c.decode(Set<String>.self, forKey: .names)
        pathPrefixes = try c.decode([String].self, forKey: .pathPrefixes)
        folderText = try c.decodeIfPresent(String.self, forKey: .folderText)
        excludeHidden = try c.decode(Bool.self, forKey: .excludeHidden)
        excludeDevFolders = try c.decodeIfPresent(Bool.self, forKey: .excludeDevFolders) ?? true
        excludeVCSFolders = try c.decodeIfPresent(Bool.self, forKey: .excludeVCSFolders) ?? true
        excludeTrash = try c.decodeIfPresent(Bool.self, forKey: .excludeTrash) ?? true
        excludeFilePatterns = try c.decodeIfPresent([String].self, forKey: .excludeFilePatterns) ?? []
    }

    /// Also accepts paths pasted into the folder-name editor in older builds.
    public func resolvingPaths() -> Self {
        var result = self
        for name in names where name.hasPrefix("/") || name.hasPrefix("~/") {
            result.names.remove(name)
            result.pathPrefixes.append(name)
        }
        result.pathPrefixes = Array(Set(result.pathPrefixes.flatMap { raw -> [String] in
            let path = URL(fileURLWithPath: (raw as NSString).expandingTildeInPath).standardizedFileURL.path
            // Foundation shortens /private/var to /var; the scanner sees the
            // physical path. Keep both spellings without resolving filesystem links.
            if ["/var", "/tmp", "/etc"].contains(where: { path == $0 || path.hasPrefix($0 + "/") }) {
                return [path, "/private" + path]
            }
            return [path]
        })).sorted()
        return result
    }

    /// Keep the visible list in the requested order, including macOS aliases.
    public static var defaultFolderPaths: [String] {
        [
            "/Library", "/System", "/Volumes",
            "/private/tmp", "/private/var", "/private/var/folders",
            "/tmp", "/var", "/var/folders", "/Users/Deleted Users",
            FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library").path
        ]
    }

    /// Append without rewriting the user's spelling, ordering or blank lines.
    public func addingFolders(_ paths: [String]) -> Self {
        var result = self
        var text = folderText ?? (names.sorted() + pathPrefixes).joined(separator: "\n")
        func key(_ path: String) -> String {
            (path as NSString).expandingTildeInPath.precomposedStringWithCanonicalMapping
        }
        var known = Set((pathPrefixes + Array(names)).map(key))
        for path in paths where known.insert(key(path)).inserted {
            result.pathPrefixes.append(path)
            if !text.isEmpty && !text.hasSuffix("\n") { text += "\n" }
            text += path
        }
        result.folderText = text
        return result
    }

    public static var defaults: ExcludeRules {
        let paths = defaultFolderPaths
        return ExcludeRules(pathPrefixes: paths, folderText: paths.joined(separator: "\n"))
    }

}
