#!/usr/bin/env python3
"""The WP-32 security audit, run on every `just ci`.

1. CSP: no remote sources, no inline or eval scripts, IPC the only connect
   target; the global Tauri object off and the prototype frozen.
2. Capabilities: the webview gets `core:default` and nothing else, no
   remote origins; no Tauri plugin in the shell or the webview packages.
3. Network paths (CLAUDE.md): the only HTTP client is ureq, linked by
   skyla-app alone and called from its reference-data module alone; the
   only sockets are the advisor tool host's loopback listener and the MCP
   shim's loopback client; the only spawned process is the user's own
   `claude`. The webview code makes no network calls of its own.

Exits non-zero with a message per violation. See docs/design/SECURITY_REVIEW.md.
"""

import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TAURI = ROOT / "apps/desktop/src-tauri"

HTTP_CLIENTS = {
    "ureq", "reqwest", "hyper", "isahc", "curl", "surf", "attohttpc",
    "minreq", "ehttp", "ureq-proto", "h2", "awc", "tiny_http", "rouille",
}
# Which workspace crate may link which client, and why.
HTTP_ALLOWED = {("skyla-app", "ureq"), ("ureq", "ureq-proto")}

# Rust call sites that reach the network or spawn processes: file → why.
RUST_ALLOWED = {
    r"ureq::": {"crates/skyla-app/src/core/refdata.rs": "opt-in ČNB reference data"},
    r"TcpListener::bind": {"crates/skyla-app/src/core/toolhost.rs": "advisor tool host, loopback"},
    r"TcpStream::connect": {"crates/skyla-mcp/src/lib.rs": "MCP shim to the tool host, loopback"},
    r"UdpSocket": {},
    r"Command::new": {"crates/skyla-advisor/src/cli.rs": "the user's own claude binary"},
}

WEB_FORBIDDEN = [
    r"\bfetch\s*\(", r"XMLHttpRequest", r"new\s+WebSocket", r"EventSource",
    r"sendBeacon", r"importScripts", r"navigator\.serviceWorker",
]


def audit_csp(errors: list[str]) -> None:
    conf = json.loads((TAURI / "tauri.conf.json").read_text())
    app = conf.get("app", {})
    sec = app.get("security", {})
    csp = sec.get("csp")
    if not isinstance(csp, dict):
        errors.append("tauri.conf.json: the CSP must be set as directives")
        return
    want = {
        "default-src": "'self'",
        "script-src": "'self'",
        "object-src": "'none'",
        "base-uri": "'none'",
        "frame-ancestors": "'none'",
        "form-action": "'none'",
        "connect-src": "ipc: http://ipc.localhost",
    }
    for directive, value in want.items():
        if csp.get(directive) != value:
            errors.append(f"CSP {directive} must be {value!r}, is {csp.get(directive)!r}")
    for directive, value in csp.items():
        tokens = value.split()
        for t in tokens:
            remote = re.match(r"^(https?:|wss?:|\*|data:|blob:)", t)
            allowed = (
                (directive == "connect-src" and t in ("ipc:", "http://ipc.localhost"))
                or (directive == "img-src" and t == "data:")
            )
            if remote and not allowed:
                errors.append(f"CSP {directive} allows {t}")
        if "'unsafe-eval'" in tokens or "'wasm-unsafe-eval'" in tokens:
            errors.append(f"CSP {directive} allows eval")
        if directive != "style-src" and "'unsafe-inline'" in tokens:
            errors.append(f"CSP {directive} allows inline code")
    if app.get("withGlobalTauri") is not False:
        errors.append("withGlobalTauri must be false")
    if sec.get("freezePrototype") is not True:
        errors.append("freezePrototype must be true")
    for risky in ("dangerousDisableAssetCspModification", "devCsp"):
        if risky in sec:
            errors.append(f"tauri.conf.json sets {risky}")
    if sec.get("assetProtocol", {}).get("enable"):
        errors.append("the asset protocol must stay off")
    if "plugins" in conf and conf["plugins"]:
        errors.append(f"tauri.conf.json configures plugins: {sorted(conf['plugins'])}")


def audit_capabilities(errors: list[str]) -> None:
    files = sorted((TAURI / "capabilities").glob("*.json"))
    if not files:
        errors.append("no capabilities file")
    for f in files:
        cap = json.loads(f.read_text())
        perms = cap.get("permissions", [])
        extra = [p for p in perms if p != "core:default"]
        if extra:
            errors.append(f"{f.name}: grants more than core:default: {extra}")
        if "remote" in cap:
            errors.append(f"{f.name}: grants IPC to remote origins")
        if cap.get("windows") != ["main"]:
            errors.append(f"{f.name}: applies to {cap.get('windows')}, expected ['main']")
    cargo = (TAURI / "Cargo.toml").read_text()
    for m in re.finditer(r"^(tauri-plugin-[\w-]+)\s*=", cargo, re.M):
        errors.append(f"src-tauri links {m.group(1)}")
    for pkg in [ROOT / "package.json", *ROOT.glob("apps/*/package.json"), *ROOT.glob("packages/*/package.json")]:
        data = json.loads(pkg.read_text())
        for section in ("dependencies", "devDependencies"):
            for name in data.get(section, {}):
                if name.startswith("@tauri-apps/plugin-"):
                    errors.append(f"{pkg.relative_to(ROOT)} depends on {name}")


# The desktop targets sky-la ships. (Tauri links reqwest on Android and
# iOS only, for its mobile dev-server proxy; those targets aren't built.)
DESKTOP_TARGETS = ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-pc-windows-msvc"]


def audit_http_clients(errors: list[str]) -> None:
    found: set[str] = set()
    for target in DESKTOP_TARGETS:
        meta = json.loads(
            subprocess.run(
                ["cargo", "metadata", "--format-version", "1", "--locked",
                 "--filter-platform", target, "--manifest-path", str(TAURI / "Cargo.toml")],
                cwd=ROOT, check=True, capture_output=True, text=True,
            ).stdout
        )
        names = {p["id"]: p["name"] for p in meta["packages"]}
        for node in meta["resolve"]["nodes"]:
            parent = names[node["id"]]
            for dep in node["deps"]:
                child = names[dep["pkg"]]
                normal = any(k.get("kind") in (None, "build") for k in dep["dep_kinds"])
                if child in HTTP_CLIENTS and normal and (parent, child) not in HTTP_ALLOWED:
                    found.add(f"{parent} links the HTTP crate {child} on {target}")
    errors.extend(sorted(found))


def audit_sources(errors: list[str]) -> None:
    rust = [
        p for p in ROOT.glob("crates/*/src/**/*.rs")
    ] + list(TAURI.glob("src/**/*.rs"))
    for pattern, allowed in RUST_ALLOWED.items():
        rx = re.compile(pattern)
        for path in rust:
            rel = path.relative_to(ROOT).as_posix()
            text = path.read_text()
            # Test modules may open sockets and spawn helpers.
            body = text.split("#[cfg(test)]")[0]
            if rx.search(body) and rel not in allowed:
                errors.append(f"{rel}: {pattern} outside its allowed place")
    web = [
        p for base in ("apps/desktop/src", "packages/ipc/src", "packages/ui/src", "packages/fixtures/src")
        for p in (ROOT / base).glob("**/*.ts*")
        if ".test." not in p.name and "bindings.ts" != p.name
    ]
    for path in web:
        text = path.read_text()
        for pattern in WEB_FORBIDDEN:
            if re.search(pattern, text):
                errors.append(f"{path.relative_to(ROOT).as_posix()}: the webview calls {pattern}")


def main() -> int:
    errors: list[str] = []
    audit_csp(errors)
    audit_capabilities(errors)
    audit_http_clients(errors)
    audit_sources(errors)
    for e in errors:
        print(f"security: {e}", file=sys.stderr)
    if errors:
        return 1
    print("security audit: CSP, capabilities, network paths and process spawns ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
