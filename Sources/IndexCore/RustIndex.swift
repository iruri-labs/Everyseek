import Foundation
import CEverythingCore

public struct IndexStatus: Decodable, Sendable {
    public let total: Int
    public let pending: Int
    public let rebuilding: Bool
    public let inaccessible: Int
    public let revision: UInt64
    public let inboxCursor: UInt64
    public let appliedCursor: UInt64
    public let error: String?
    public let currentPath: String
}

public struct UnavailableFolder: Decodable, Sendable, Identifiable {
    public let path: String
    public let message: String
    public var id: String { path }
}

public struct SearchPage: Decodable, Sendable {
    public let rows: [FileRecord]
    public let hasMore: Bool
}

public enum QueryEngine {
    public enum SortKey: String, Sendable { case name, path, size, mtime, kind }
}

public struct CoreError: LocalizedError, Sendable {
    public let message: String
    public init(message: String) { self.message = message }
    public var errorDescription: String? { message }
}

// The handle is immutable and the Rust API synchronizes writes internally.
// Each call owns its response allocation. Strong references held by detached
// tasks keep the handle alive until the last in-flight operation returns.
public final class RustIndex: @unchecked Sendable {
    private let handle: OpaquePointer

    public init(databaseURL: URL) throws {
        var error: UnsafeMutablePointer<CChar>?
        let pointer = databaseURL.path.withCString { em_open($0, &error) }
        defer { if let error { em_string_free(error) } }
        guard let pointer else {
            let raw = error.map { Data(String(cString: $0).utf8) }
            let message = raw.flatMap { try? JSONDecoder().decode(ErrorEnvelope.self, from: $0).error }
                ?? "Could not open the file index."
            throw CoreError(message: message)
        }
        handle = pointer
    }

    deinit { em_close(handle) }
    public func cancelSearch() { em_cancel_search(handle) }

    private struct ErrorEnvelope: Decodable { let error: String? }
    private struct Envelope<T: Decodable>: Decodable { let value: T?; let error: String? }
    private struct Acknowledgement: Decodable { let ok: Bool }

    private func call<T: Decodable>(_ request: [String: Any], as: T.Type) throws -> T {
        let bytes = try JSONSerialization.data(withJSONObject: request)
        let json = String(decoding: bytes, as: UTF8.self)
        guard let response = json.withCString({ em_request(handle, $0) }) else {
            throw CoreError(message: "The index returned an empty response.")
        }
        defer { em_string_free(response) }
        let envelope = try JSONDecoder().decode(Envelope<T>.self, from: Data(String(cString: response).utf8))
        if let error = envelope.error { throw CoreError(message: error) }
        guard let value = envelope.value else { throw CoreError(message: "Invalid index response.") }
        return value
    }

    public func status() throws -> IndexStatus { try call(["op": "status"], as: IndexStatus.self) }

    public func unavailableFolders() throws -> [UnavailableFolder] {
        try call(["op": "unavailable"], as: [UnavailableFolder].self)
    }

    public func configure(roots: [String], rules: ExcludeRules, baseline: UInt64, rebuild: Bool) throws {
        let encoded = try JSONEncoder().encode(rules)
        let rulesObject = try JSONSerialization.jsonObject(with: encoded)
        let _: Acknowledgement = try call(["op": "configure", "config": ["roots": roots, "rules": rulesObject],
                                           "baseline": baseline, "rebuild": rebuild], as: Acknowledgement.self)
    }

    public func enqueue(paths: [String: Bool], cursor: UInt64, reset: Bool = false) throws {
        let changes = paths.map { ["path": $0.key, "recursive": $0.value] as [String: Any] }
        let _: Acknowledgement = try call(["op": "events", "changes": changes, "cursor": cursor,
                                           "reset": reset], as: Acknowledgement.self)
    }

    public func search(_ text: String, matchPath: Bool, caseInsensitive: Bool,
                       wholeWord: Bool, sort: QueryEngine.SortKey, ascending: Bool,
                       limit: Int, offset: Int = 0) throws -> SearchPage {
        try call(["op": "search", "text": text, "matchPath": matchPath, "caseInsensitive": caseInsensitive,
                  "wholeWord": wholeWord, "sort": sort.rawValue, "ascending": ascending,
                  "limit": limit, "offset": offset], as: SearchPage.self)
    }

    public func retry() throws { let _: Acknowledgement = try call(["op": "retry"], as: Acknowledgement.self) }
    public func checkpoint() throws { let _: Acknowledgement = try call(["op": "checkpoint"], as: Acknowledgement.self) }
}
