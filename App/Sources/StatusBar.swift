import SwiftUI

struct StatusBar: View {
    @EnvironmentObject var model: AppModel
    @State private var showProgress = false
    @State private var showUnavailable = false
    private var busy: Bool { model.rebuilding || model.searching }

    var body: some View {
        HStack(spacing: 6) {
            ZStack {
                if showProgress { ProgressView().controlSize(.small).scaleEffect(0.6) }
            }.frame(width: 12, height: 12)
            Text(model.rebuilding ? "Indexing… \(model.total.formatted()) objects" : "\(model.total.formatted()) objects")
            if model.inaccessible > 0 || showUnavailable {
                Text("·")
                Button {
                    showUnavailable.toggle()
                } label: {
                    HStack(spacing: 4) {
                        Text("\(model.inaccessible.formatted()) \(model.inaccessible == 1 ? "folder" : "folders") unavailable")
                        Image(systemName: "chevron.up").font(.system(size: 8, weight: .semibold))
                    }
                    .foregroundStyle(.orange)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("unavailableFolders")
                .help("Show unavailable folders and add individual folders to the exclusion list.")
                .popover(isPresented: $showUnavailable, arrowEdge: .bottom) {
                    UnavailableFoldersView().environmentObject(model)
                }
                Button("Retry") { model.retryIndex() }
                    .controlSize(.mini)
                    .disabled(model.retryingIndex || model.applyingRules || model.inaccessible == 0)
                    .help("Retry unavailable folders.")
            }
            Spacer()
            Text("\(model.results.count.formatted())\(model.hasMore ? "+" : "") results")
        }
        .font(.system(size: 11)).foregroundStyle(.secondary)
        .padding(.horizontal, 10).padding(.vertical, 4)
        .task(id: busy) {
            showProgress = false
            guard busy else { return }
            try? await Task.sleep(for: .milliseconds(250))
            guard !Task.isCancelled else { return }
            showProgress = true
        }
    }
}
