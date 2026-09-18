// A result page decoded from Rust. Persistent SQLite IDs remain stable while
// an entry exists; the UI never needs the entire index in memory.
public struct FileRecord: Sendable, Equatable, Decodable {
    public let id: UInt64
    public let name: String
    public let path: String
    public let parent: UInt64
    public let size: UInt64
    public let mtime: Int64
    public let isDir: Bool
    public let volID: UInt64

    private enum CodingKeys: String, CodingKey {
        case id, name, path, parent, size, mtime, isDir
        case volID = "volId"
    }
}
