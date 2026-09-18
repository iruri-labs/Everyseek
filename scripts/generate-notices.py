#!/usr/bin/env python3
"""Bundle upstream notices for the locked macOS dependency graph."""
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--manifest-path", str(root / "Core/Cargo.toml"),
    "--locked", "--offline", "--format-version", "1",
    "--filter-platform", "aarch64-apple-darwin",
]))
included = {node["id"] for node in metadata["resolve"]["nodes"]}
parts = ["Everyseek licensing and third-party notices\n"]
for notice in ("LICENSE", "NOTICE", "LICENSES/upstream-everything-mac-MIT.txt", "LICENSES/README.md"):
    parts.extend([f"\n{notice}\n", (root / notice).read_text()])
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["id"] not in included or package["name"] == "everything-core":
        continue
    directory = pathlib.Path(package["manifest_path"]).parent
    notices = sorted(p for p in directory.iterdir() if p.is_file()
                     and p.name.lower().startswith(("license", "copying", "notice")))
    if not notices:
        raise SystemExit("Missing license notice: " + package["name"])
    parts.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\n"
                 f"License: {package['license']}\nSource: {package.get('repository') or ''}\n")
    for notice in notices:
        parts.extend([notice.name + "\n", notice.read_text()])
output = pathlib.Path(sys.argv[1])
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text("\n".join(parts))
