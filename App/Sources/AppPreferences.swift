import Foundation

enum AppPreferences {
    static var store: UserDefaults {
        if let suite = ProcessInfo.processInfo.environment["EVERYTHINGMAC_DEFAULTS_SUITE"],
           let defaults = UserDefaults(suiteName: suite) { return defaults }

        let defaults = UserDefaults.standard
        let bundleID = "com.everyseek.app"
        let migrationKey = "migration.everyseekPreferences.v1"
        guard Bundle.main.bundleIdentifier == bundleID,
              !defaults.bool(forKey: migrationKey) else { return defaults }

        // Changing the bundle identifier creates a new preferences domain.
        // Import only app-owned settings; keep values already saved by Everyseek.
        let current = defaults.persistentDomain(forName: bundleID) ?? [:]
        let legacy = defaults.persistentDomain(forName: "com.everythingmac.app") ?? [:]
        for (key, value) in legacy
            where current[key] == nil &&
                (key.hasPrefix("pref.") || key == "excludeRules" || key == "defaultExclusionsVersion") {
            defaults.set(value, forKey: key)
        }
        defaults.set(true, forKey: migrationKey)
        return defaults
    }
}
