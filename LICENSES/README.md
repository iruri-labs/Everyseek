# License allocation

Everyseek is distributed under **[Everyseek License 1.0](../LICENSE)** for all copyrightable original contributions owned by Irurilabs Corp. This includes additions and modifications made before the current release, not just future changes. It is a source-available license, with free personal and internal company use and prior written permission required for monetization.

**MIT is not an alternative license for Everyseek Contributions.** The separate [MIT notice](upstream-everything-mac-MIT.txt) covers inherited original EverythingMac material.

## Upstream reference

- Project: [alesloa/everything-mac](https://github.com/alesloa/everything-mac)
- Copyright: 2026 Alejandro Sloan
- Reference: [v0.3.0 / bb178c419f518f7a32dc7b70bebb4677464ecd93](https://github.com/alesloa/everything-mac/tree/bb178c419f518f7a32dc7b70bebb4677464ecd93)
- The upstream MIT license text is reproduced without changing its grant or attribution.

## Source allocation

| Material | Terms for this distribution |
| --- | --- |
| Original EverythingMac code retained from upstream | MIT, with the original notice |
| Irurilabs Corp.'s copyrightable modifications within upstream files | Everyseek License 1.0 for those modifications; MIT for the inherited original portions |
| Original Everyseek Rust engine in `Core/`, Swift/C bridge additions, and new application/build code | Everyseek License 1.0, except separately identified third-party material |
| Rust dependencies, the Manrope font, and credited music/sound effects | Their respective third-party licenses |

The following source files are unchanged from the upstream reference in v0.4.9:

- `App/Sources/FileIcons.swift`
- `App/Sources/FullDiskAccess.swift`
- `App/Sources/LaunchAtLogin.swift`
- `App/Sources/SearchField.swift`
- `App/Sources/SearchSettingsView.swift`
- `App/Sources/Styling.swift`
- `App/Sources/VolumesSettingsView.swift`

The other retained upstream source/configuration files have Everyseek changes: `App/Sources/AppCommands.swift`, `AppModel.swift`, `ContentView.swift`, `EverythingMacApp.swift`, `ExcludeSettingsView.swift`, `GeneralSettingsView.swift`, `IndexActor.swift`, `ResultActions.swift`, `ResultsTable.swift`, `SettingsView.swift`, `StatusBar.swift`; `App/project.yml`; `Package.swift`; `Sources/IndexCore/ExcludeRules.swift`, `FileRecord.swift`; and `scripts/build-dev.sh`, `build-dmg.sh`, `relaunch.sh`. In those files, the original upstream portions retain MIT and the copyrightable Everyseek changes use Everyseek License 1.0.

Paths and file equality help locate the material; they do not override authorship. A renamed file can still contain upstream code, and a newly created file can still contain separately licensed material. Upstream-derived documentation and other retained upstream assets also retain their original terms. See [NOTICE](../NOTICE) for the complete attribution.

This allocation describes the present distribution and does not revoke any separately acquired, valid prior license rights.
