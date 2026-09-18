import SwiftUI

struct UnavailableFoldersView: View {
    @EnvironmentObject var model: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Unavailable folders").font(.headline)
                Spacer()
                Button { dismiss() } label: { Image(systemName: "xmark") }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Close unavailable folders")
            }
            Text("Existing results are kept until access returns. Excluding a folder removes its indexed results and stops retries.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Divider()
            if model.unavailableFolders.isEmpty {
                if model.loadingUnavailable {
                    HStack { Spacer(); ProgressView().controlSize(.small); Spacer() }
                        .frame(height: 72)
                } else if model.unavailableError == nil {
                    Label("No unavailable folders.", systemImage: "checkmark.circle")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 72)
                }
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        ForEach(model.unavailableFolders) { folder in
                            VStack(alignment: .leading, spacing: 6) {
                                Label {
                                    Text(folder.path)
                                        .font(.system(size: 12, design: .monospaced))
                                        .textSelection(.enabled)
                                        .fixedSize(horizontal: false, vertical: true)
                                } icon: {
                                    Image(systemName: "folder.badge.questionmark")
                                        .foregroundStyle(.orange)
                                }
                                Text(folder.message)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(2)
                                    .help(folder.message)
                                HStack {
                                    Spacer()
                                    Button("Add to excluded folder list") {
                                        model.excludeUnavailableFolder(folder)
                                    }
                                    .controlSize(.small)
                                    .disabled(model.applyingRules)
                                    .accessibilityLabel("Exclude \(folder.path)")
                                }
                            }
                            if folder.id != model.unavailableFolders.last?.id { Divider() }
                        }
                    }
                    .padding(.trailing, 4)
                }
                .frame(height: min(320, CGFloat(model.unavailableFolders.count) * 108))
            }
            if let error = model.unavailableError ?? model.errorMessage {
                Text(error).font(.caption).foregroundStyle(.orange).textSelection(.enabled)
            }
            if model.applyingRules {
                HStack(spacing: 6) {
                    ProgressView().controlSize(.small)
                    Text("Applying exclusion…").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .padding(16)
        .frame(width: 600)
        .task {
            while !Task.isCancelled {
                await model.refreshUnavailableFolders()
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }
}
