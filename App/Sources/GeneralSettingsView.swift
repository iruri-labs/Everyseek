import SwiftUI
import IndexCore

struct GeneralSettingsView: View {
    @EnvironmentObject var model: AppModel
    @State private var launchAtLogin = LaunchAtLogin.isEnabled

    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Open Everyseek at login", isOn: $launchAtLogin)
                    // set() reflects the real resulting state — if registration fails
                    // the toggle snaps back instead of lying.
                    .onChange(of: launchAtLogin) { launchAtLogin = LaunchAtLogin.set(launchAtLogin) }
            }
            Section("Index") {
                LabeledContent("Objects indexed", value: model.total.formatted())
                if let s = Self.cacheStats() {
                    LabeledContent("Index size",
                                   value: ByteCountFormatter.string(fromByteCount: s.size, countStyle: .file))
                    LabeledContent("Last updated",
                                   value: s.modified.formatted(date: .abbreviated, time: .shortened))
                } else {
                    LabeledContent("Index", value: "not created yet")
                }
                Button(model.scanning ? "Rebuilding…" : "Rebuild Index Now") { model.rebuildIndex() }
                    .disabled(model.scanning)
            }
        }
        .formStyle(.grouped)
        .padding(20)
    }

    // Include the write-ahead log: recent committed changes may still live there.
    static func cacheStats() -> (size: Int64, modified: Date)? {
        let path = IndexActor.databaseURL().path
        var size: Int64 = 0
        var modified = Date(timeIntervalSince1970: 0)
        var found = false
        for suffix in ["", "-wal", "-shm"] {
            guard let attributes = try? FileManager.default.attributesOfItem(atPath: path + suffix) else { continue }
            found = true
            size += (attributes[.size] as? NSNumber)?.int64Value ?? 0
            modified = max(modified, (attributes[.modificationDate] as? Date) ?? modified)
        }
        return found ? (size, modified) : nil
    }
}
