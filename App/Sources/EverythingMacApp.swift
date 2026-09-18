import SwiftUI
import AppKit

@main
struct EverythingMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()
    var body: some Scene {
        WindowGroup("Everyseek") {
            ContentView().environmentObject(model)
                .frame(minWidth: 800, minHeight: 500)
                .onAppear {
                    appDelegate.model = model
                    model.bootstrap()
                }
        }
        .commands { AppCommands(model: model) }
        Settings {
            SettingsView().environmentObject(model)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    weak var model: AppModel?

    func applicationWillTerminate(_ notification: Notification) {
        guard let index = model?.index else { return }
        let sem = DispatchSemaphore(value: 0)
        Task.detached {
            await index.flush()
            sem.signal()
        }
        // Checkpoint is best effort. Each committed directory and event inbox
        // is already durable in WAL; unfinished jobs/events resume next launch.
        _ = sem.wait(timeout: .now() + 5)
    }
}
