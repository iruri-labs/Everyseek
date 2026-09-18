import Foundation
import CoreServices

struct FileEventBatch: Sendable {
    var paths: [String: Bool]
    var cursor: UInt64
    var reset: Bool
}

// Keep the C callback outside start() to avoid a Swift 6.3 region-isolation
// compiler crash when lowering an inline callback alongside its context.
private let fileEventsCallback: FSEventStreamCallback = { _, info, count, rawPaths, flags, ids in
    guard let info else { return }
    let watcher = Unmanaged<FileEvents>.fromOpaque(info).takeUnretainedValue()
    let paths = unsafeBitCast(rawPaths, to: NSArray.self)
    var batch = FileEventBatch(paths: [:], cursor: 0, reset: false)
    let deepFlags = UInt32(kFSEventStreamEventFlagMustScanSubDirs | kFSEventStreamEventFlagUserDropped |
                          kFSEventStreamEventFlagKernelDropped | kFSEventStreamEventFlagRootChanged |
                          kFSEventStreamEventFlagMount | kFSEventStreamEventFlagUnmount)
    for i in 0..<count {
        batch.cursor = max(batch.cursor, ids[i])
        if flags[i] & UInt32(kFSEventStreamEventFlagEventIdsWrapped) != 0 { batch.reset = true }
        if flags[i] & UInt32(kFSEventStreamEventFlagHistoryDone) != 0 { continue }
        guard let raw = paths[i] as? String else { continue }
        let path = FileEvents.canonicalPath(raw)
        batch.paths[path] = (batch.paths[path] ?? false) || (flags[i] & deepFlags != 0)
    }
    watcher.deliver(batch)
}

// Platform adapter only. Rust owns reconciliation and persistence.
// start/stop are serialized by IndexActor; callbacks use only immutable receive.
final class FileEvents: @unchecked Sendable {
    private var stream: FSEventStreamRef?
    private let queue = DispatchQueue(label: "com.irurilabs.everythingmac.events")
    private let receive: @Sendable (FileEventBatch) -> Void

    init(receive: @escaping @Sendable (FileEventBatch) -> Void) { self.receive = receive }
    deinit { stop() }
    fileprivate func deliver(_ batch: FileEventBatch) { receive(batch) }

    func start(paths: [String], since cursor: UInt64) throws {
        stop()
        var context = FSEventStreamContext(version: 0, info: Unmanaged.passUnretained(self).toOpaque(),
                                          retain: nil, release: nil, copyDescription: nil)
        let flags = UInt32(kFSEventStreamCreateFlagUseCFTypes | kFSEventStreamCreateFlagNoDefer |
                           kFSEventStreamCreateFlagWatchRoot)
        guard let newStream = FSEventStreamCreate(nil, fileEventsCallback, &context, paths as CFArray, cursor, 0.5, flags) else {
            throw NSError(domain: "EverythingMac", code: 1, userInfo: [NSLocalizedDescriptionKey: "Could not create the file change monitor."])
        }
        stream = newStream
        FSEventStreamSetDispatchQueue(newStream, queue)
        guard FSEventStreamStart(newStream) else {
            stop()
            throw NSError(domain: "EverythingMac", code: 2, userInfo: [NSLocalizedDescriptionKey: "Could not start the file change monitor."])
        }
    }

    func stop() {
        guard let stream else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        queue.sync {}
        FSEventStreamRelease(stream)
        self.stream = nil
    }

    fileprivate static func canonicalPath(_ path: String) -> String {
        let alias = "/System/Volumes/Data"
        if path == alias { return "/" }
        if path.hasPrefix(alias + "/") { return String(path.dropFirst(alias.count)) }
        return path == "/" ? path : "/" + path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    }
}
