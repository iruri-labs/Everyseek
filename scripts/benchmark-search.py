#!/usr/bin/env python3
"""Measure the release CLI against an isolated snapshot; never publish file results."""
import argparse
import datetime
import hashlib
import json
import math
import pathlib
import platform
import random
import sqlite3
import statistics
import subprocess
import tempfile
import time

QUERIES = ["readme", "config", "report", ".pdf", ".png", "계획", "보고서", "정산", "a", "zzzxq_no_match_20260919"]


def summary(samples):
    ordered = sorted(samples)
    return {
        "median_ms": round(statistics.median(samples), 3),
        "p95_ms": round(ordered[math.ceil(len(ordered) * 0.95) - 1], 3),
        "min_ms": round(ordered[0], 3),
        "max_ms": round(ordered[-1], 3),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--database", type=pathlib.Path, required=True)
    parser.add_argument("--binary", type=pathlib.Path, default=pathlib.Path("Core/target/release/everything-index"))
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--rounds", type=int, default=30)
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("--rounds must be positive")
    binary = args.binary.resolve()
    database = args.database.resolve()
    source_sizes = {
        suffix or "database": pathlib.Path(str(database) + suffix).stat().st_size
        for suffix in ("", "-wal", "-shm")
        if pathlib.Path(str(database) + suffix).exists()
    }
    measurements = {query: {"samples_ms": []} for query in QUERIES}
    with tempfile.TemporaryDirectory(prefix="everyseek-benchmark-") as temporary:
        snapshot = pathlib.Path(temporary) / "index.sqlite"
        source = sqlite3.connect(database.as_uri() + "?mode=ro", uri=True)
        copy = sqlite3.connect(snapshot)
        try:
            source.backup(copy)
            counts = dict(copy.execute("SELECT is_dir, count(*) FROM files GROUP BY is_dir"))
            # Only the temporary copy changes: disable background scanning while
            # retaining every filename, search posting, sort index and file row.
            copy.execute("DELETE FROM meta WHERE key='config'")
            copy.execute("DELETE FROM pending_dirs")
            copy.commit()
        finally:
            copy.close()
            source.close()
        process = subprocess.Popen(
            [str(binary), str(snapshot)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        def request(payload):
            encoded = (json.dumps(payload, ensure_ascii=False) + "\n").encode()
            started = time.perf_counter_ns()
            process.stdin.write(encoded)
            process.stdin.flush()
            response = process.stdout.readline()
            elapsed = (time.perf_counter_ns() - started) / 1_000_000
            if not response:
                raise RuntimeError("CLI exited without a response")
            decoded = json.loads(response)
            if "error" in decoded:
                raise RuntimeError(decoded["error"])
            return decoded["value"], elapsed
        try:
            status, _ = request({"op": "status"})
            if status["total"] != sum(counts.values()):
                raise RuntimeError("Snapshot count differs from engine status")
            options = {
                "op": "search", "matchPath": False, "caseInsensitive": True,
                "wholeWord": False, "sort": "name", "ascending": True,
                "limit": 500, "offset": 0,
            }
            # One full warm-up pass; no cold-cache claim.
            for query in QUERIES:
                request({**options, "text": query})
            rng = random.Random(20260919)
            for _ in range(args.rounds):
                order = QUERIES.copy()
                rng.shuffle(order)
                for query in order:
                    page, elapsed = request({**options, "text": query})
                    item = measurements[query]
                    count = len(page["rows"])
                    if count > 500:
                        raise RuntimeError("Result limit exceeded")
                    if "rows" in item and (item["rows"], item["has_more"]) != (count, page["hasMore"]):
                        raise RuntimeError("Snapshot results changed during measurement")
                    item.update(rows=count, has_more=page["hasMore"])
                    item["samples_ms"].append(round(elapsed, 6))
            final_status, _ = request({"op": "status"})
            if final_status["total"] != status["total"]:
                raise RuntimeError("Snapshot changed during measurement")
        finally:
            process.stdin.close()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=10)
            process.stdout.close()
            process.stderr.close()
    all_samples = [value for item in measurements.values() for value in item["samples_ms"]]
    for item in measurements.values():
        item.update(summary(item["samples_ms"]))
    report = {
        "measured_at": datetime.datetime.now().astimezone().isoformat(),
        "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "machine": {
            "chip": subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip(),
            "memory_bytes": int(subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True)),
            "macos": platform.mac_ver()[0],
        },
        "scope": "Release Rust CLI request write through complete JSON response read; excludes UI debounce/rendering and Python JSON decoding",
        "cache": "Warm OS cache; one untimed pass, then shuffled query order in each round",
        "indexed_objects": sum(counts.values()),
        "files": counts.get(0, 0),
        "folders": counts.get(1, 0),
        "source_index_bytes": source_sizes,
        "source_index_total_bytes": sum(source_sizes.values()),
        "options": options,
        "rounds_per_query": args.rounds,
        "sample_count": len(all_samples),
        "aggregate": summary(all_samples),
        "queries": measurements,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: value for key, value in report.items() if key != "queries"}, indent=2))
    for query, item in measurements.items():
        print(json.dumps({"query": query, **{k: v for k, v in item.items() if k != "samples_ms"}}, ensure_ascii=False))


if __name__ == "__main__":
    main()
