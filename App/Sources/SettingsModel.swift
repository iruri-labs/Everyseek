import SwiftUI
import IndexCore

@MainActor final class SettingsModel: ObservableObject {
    @Published var namesText = ""
    @Published var filePatternsText = ""
    @Published var excludeHidden = false
    @Published var excludeDevFolders = true
    @Published var excludeVCSFolders = true
    @Published var excludeTrash = true

    private static func lines(_ text: String) -> [String] {
        text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }
    private static func isPath(_ text: String) -> Bool {
        text.hasPrefix("/") || text.hasPrefix("~/")
    }

    // Both tabs edit one list. Removing a path in either tab really removes it.
    var pathPrefixes: [String] {
        get { Self.lines(namesText).filter(Self.isPath) }
        set {
            let previous = Set(pathPrefixes)
            let wanted = Set(newValue)
            let kept = namesText.components(separatedBy: "\n").filter {
                let line = $0.trimmingCharacters(in: .whitespacesAndNewlines)
                return !Self.isPath(line) || wanted.contains(line)
            }
            namesText = (kept + newValue.filter { !previous.contains($0) }).joined(separator: "\n")
        }
    }

    func load(from rules: ExcludeRules) {
        namesText = rules.folderText ?? (rules.names.sorted() + rules.pathPrefixes).joined(separator: "\n")
        filePatternsText = rules.excludeFilePatterns.joined(separator: "\n")
        excludeHidden = rules.excludeHidden
        excludeDevFolders = rules.excludeDevFolders
        excludeVCSFolders = rules.excludeVCSFolders
        excludeTrash = rules.excludeTrash
    }

    func makeRules() -> ExcludeRules {
        let entries = Self.lines(namesText)
        return ExcludeRules(names: Set(entries.filter { !Self.isPath($0) }),
                            pathPrefixes: entries.filter(Self.isPath),
                            excludeHidden: excludeHidden,
                            excludeDevFolders: excludeDevFolders,
                            excludeVCSFolders: excludeVCSFolders,
                            excludeTrash: excludeTrash,
                            excludeFilePatterns: Self.lines(filePatternsText),
                            folderText: namesText)
    }
}
