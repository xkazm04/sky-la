# Desktop app and UI

A Tauri v2 shell around the Rust core, with a React 19 + TypeScript + Tailwind v4 webview in design direction A ("Tahoe"). The webview holds no secrets and enforces no rules: it collects what's typed, shows what the core returns, and formats it.

**Code:** `apps/desktop/src-tauri` (shell), `apps/desktop/src` (screens, shell, router), `packages/ipc` (generated bindings, mock transport), `packages/ui` (tokens and primitives), `packages/fixtures` (demo data and IPC recordings) · **Tests:** `apps/desktop/e2e`, `apps/desktop/src-tauri/tests/ipc.rs`, vitest suites in `packages/*` and `apps/desktop/src`

## Shell and IPC

- 74 Tauri commands. Each is a thin wrapper over a `skyla_app::Core` method and returns typed DTOs.
- tauri-specta generates `packages/ipc/src/bindings.ts`. The webview talks to the core only through these `commands`, and a test fails if the bindings are stale.
- `Session` holds the open books. Each command takes `Books` and is refused with `locked` while the books are locked. Errors come back as a typed `IpcFailure` (`code`, `message`).
- A navigation guard keeps the window on the app's own pages. The CSP and capabilities are audited in `just ci`.

## Running headlessly: the mock transport

- Outside Tauri (`just dev-web`, the e2e suite), `connectCore()` mocks the IPC layer and replays `packages/fixtures/data/ipc-recordings.json`.
- The recordings are the real core's answers to canonical requests and to **scenarios**: scripted writes that move the mock into a new state, so flows like *approve in the inbox* or *undo a booking* replay faithfully.
- `just recordings` re-records them. A test replays every recording over the real Tauri IPC and requires identical answers.
- Desktop builds leave the recordings and the mock module out. The webview bundle is 676 kB (204 kB gzipped).

## Screens

| Screen | What it does |
|---|---|
| Overview | Key figures, receivables and payables, what's coming up |
| Inbox | Proposals to approve, deadlines, advice ([Inbox](inbox.md)) |
| Invoices | List, editor (new and edit), issue, export, recurring templates, reminders ([Invoicing](invoicing.md)) |
| Purchases | Received invoices, record one ([Purchases](purchases.md)) |
| Bank | The reconciliation workbench ([Bank](bank.md)) |
| Statements | P&L, balance sheet, trial balance, cash basis, journal; drill-down to entries and "Explain this" |
| Taxes | DPH, kontrolní hlášení, § 7 scenarios, insurance, the obligations calendar ([Taxes](taxes.md)) |
| Advisors | Connection status, findings, tasks ([Advisors](advisors.md)) |
| Egress register | Every advisor run and exactly what was shared |
| Settings | Appearance, business details, encryption (Lock now), backups, recovery, advisors and sharing, reference data, export, updates |
| Setup, Unlock, Recover | The books' lifecycle before they're open ([Storage](storage-and-security.md)) |

Routing is a typed hash router with deep links per selection (`#/invoices/2026-102`, `#/bank/s1-3`). The shell stays mounted, so the source list keeps focus, and each screen portals its inspector into the right pane.

## Design system (direction A, Tahoe)

- Tokens are in `packages/ui/src/tokens.css`, light and dark. Dark is re-stepped greys and elevation, not an inversion. There are glass panels with opaque fallbacks for no-blur and reduced transparency.
- Primitives are built on React Aria: Button, SegmentedControl, SearchField, Popup, Menu, DataTable (with in-list sections), Inspector, SourceList, StatusBar, Badge, Kbd, TextField, Select, Checkbox, plus AppWindow, Toolbar and ContentGroup.
- The gallery lives at `#/gallery`.
- Unit tests check that both token sets name the same tokens and that text meets WCAG AA on its surfaces.
- No display serifs, no decorative monospace and no bordered card grids (CLAUDE.md).

## End-to-end testing

- Playwright walks every screen in both appearances with **axe** (WCAG 2.2 AA) and pixel baselines (`apps/desktop/e2e/baseline`; `just e2e-update` after an intended change).
- It runs the flows on the recorded core:
  - first run, unlock and recover;
  - invoice create, issue and export;
  - edit a draft, recurring invoices, reminders;
  - bank import, accept, split, rule and undo;
  - inbox approval and dismissal;
  - purchases, import and export, the advisors, the register.
- A test fails on any page or console error.
