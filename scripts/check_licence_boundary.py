#!/usr/bin/env python3
"""Enforce the licence boundary from DECISIONS.md D-016.

1. Every workspace crate and every package.json declares a licence.
2. The Apache-2.0 engine crates never depend, directly or transitively,
   on an AGPL workspace crate.

Exits non-zero with a message per violation.
"""

import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
APACHE_ENGINE = {"skyla-money", "skyla-ledger", "skyla-rules"}


def main() -> int:
    meta = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=ROOT, check=True, capture_output=True, text=True,
        ).stdout
    )
    members = set(meta["workspace_members"])
    packages = {p["id"]: p for p in meta["packages"]}
    errors: list[str] = []

    for pid in members:
        pkg = packages[pid]
        licence = pkg.get("license")
        if not licence:
            errors.append(f"{pkg['name']}: Cargo.toml has no `license` field")
        elif pkg["name"] in APACHE_ENGINE and licence != "Apache-2.0":
            errors.append(f"{pkg['name']}: must be Apache-2.0, found {licence}")
        elif pkg["name"] not in APACHE_ENGINE and licence != "AGPL-3.0-or-later":
            errors.append(f"{pkg['name']}: must be AGPL-3.0-or-later, found {licence}")

    agpl_members = {pid for pid in members if packages[pid].get("license") == "AGPL-3.0-or-later"}
    graph = {node["id"]: [d["pkg"] for d in node["deps"]] for node in meta["resolve"]["nodes"]}
    for pid in members:
        name = packages[pid]["name"]
        if name not in APACHE_ENGINE:
            continue
        stack, seen = [pid], set()
        while stack:
            current = stack.pop()
            for dep in graph.get(current, []):
                if dep in seen:
                    continue
                seen.add(dep)
                if dep in agpl_members:
                    errors.append(f"{name} (Apache-2.0) depends on AGPL crate {packages[dep]['name']}")
                stack.append(dep)

    for manifest in ROOT.glob("**/package.json"):
        if "node_modules" in manifest.parts:
            continue
        if not json.loads(manifest.read_text()).get("license"):
            errors.append(f"{manifest.relative_to(ROOT)}: no `license` field")

    for crate in sorted(APACHE_ENGINE):
        if not (ROOT / "crates" / crate / "LICENSE").is_file():
            errors.append(f"crates/{crate}/LICENSE (Apache-2.0 text) is missing")
    if not (ROOT / "LICENSE").is_file():
        errors.append("LICENSE (AGPL-3.0 text) is missing at the repository root")

    for error in errors:
        print(f"licence-boundary: {error}", file=sys.stderr)
    if not errors:
        print(f"licence-boundary: ok ({len(members)} crates, engine crates clean)")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
