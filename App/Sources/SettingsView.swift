import SwiftUI
import IndexCore

struct SettingsView: View {
    @EnvironmentObject var model: AppModel
    @StateObject private var edit = SettingsModel()

    var body: some View {
        VStack(spacing: 0) {
            TabView {
                GeneralSettingsView()
                    .tabItem { Label("General", systemImage: "gearshape") }
                SearchSettingsView()
                    .tabItem { Label("Search", systemImage: "magnifyingglass") }
                ExcludeSettingsView(edit: edit, apply: apply)
                    .tabItem { Label("Exclude", systemImage: "nosign") }
                VolumesSettingsView(edit: edit, apply: apply)
                    .tabItem { Label("Volumes", systemImage: "externaldrive") }
            }
            .disabled(model.applyingRules)
            if model.applyingRules {
                HStack { ProgressView().controlSize(.small); Text("Applying exclusions…") }
                    .font(.caption).padding(.bottom, 10)
            } else if let error = model.errorMessage {
                Text(error).font(.caption).foregroundStyle(.orange).padding(.bottom, 10)
            } else if let message = model.rulesApplyMessage {
                Text(message).font(.caption).foregroundStyle(.secondary).padding(.bottom, 10)
            }
        }
        .frame(width: 540, height: 480)
        .onAppear { edit.load(from: model.rules) }
        .onChange(of: model.rules) { edit.load(from: model.rules) }
    }

    private func apply() { model.applyRules(edit.makeRules()) }
}
