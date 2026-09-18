// swift-tools-version: 6.0
import PackageDescription
import Foundation

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let package = Package(
    name: "IndexCore",
    platforms: [.macOS(.v14)],
    products: [.library(name: "IndexCore", targets: ["IndexCore"])],
    targets: [
        .target(name: "CEverythingCore", publicHeadersPath: "include",
                linkerSettings: [.unsafeFlags(["-L" + root + "/.build/rust"]),
                                 .linkedLibrary("everything_core"), .linkedLibrary("iconv")]),
        .target(name: "IndexCore", dependencies: ["CEverythingCore"])
    ]
)
