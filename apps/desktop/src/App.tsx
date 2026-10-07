import { demoEntity } from "@skyla/fixtures";
import type { AppInfo, Transport } from "@skyla/ipc";
import { useEffect, useState } from "react";

const sections = ["Overview", "Inbox", "Invoices", "Bank", "Statements", "Taxes", "Advisors"];

/**
 * WP-00 placeholder shell in direction A's three-pane structure. It proves
 * the build, the styling pipeline and an IPC round trip. Real screens arrive
 * in WP-10.
 */
export function App({ transport }: { transport: Transport }) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    transport.invoke("app_info").then(setInfo, (e: unknown) => setError(String(e)));
  }, [transport]);

  return (
    <div className="flex h-full gap-2 p-2">
      <nav
        aria-label="Sections"
        className="flex w-56 shrink-0 flex-col rounded-[18px] border border-black/[0.08] bg-white/80 px-2.5 py-3.5 shadow-[0_12px_36px_rgba(20,24,40,0.08)] backdrop-blur-2xl"
      >
        <p className="px-2.5 pb-4 text-[15px] font-semibold tracking-tight">sky-la</p>
        <ul className="flex flex-col gap-px">
          {sections.map((label) => (
            <li
              key={label}
              className="flex h-7 items-center rounded-[9px] px-2.5 text-[13px] text-ink-secondary"
            >
              {label}
            </li>
          ))}
        </ul>
        <p className="mt-auto px-2.5 text-[11.5px] text-ink-secondary">
          {demoEntity.displayName} · {demoEntity.legalForm}
        </p>
      </nav>

      <main className="flex min-w-0 flex-1 flex-col">
        <div className="flex-1 rounded-[14px] bg-white p-6 shadow-[0_0_0_0.5px_rgba(0,0,0,0.07)]">
          <h1 className="text-[15px] font-semibold tracking-tight">Nothing here yet</h1>
          <p className="mt-1 max-w-prose text-[13px] text-ink-secondary">
            This is the WP-00 scaffold. The ledger, invoicing and bank screens come in later work
            packets, as listed in docs/plan/IMPLEMENTATION_PLAN.md.
          </p>
        </div>
        <footer
          data-testid="status-line"
          className="flex h-[30px] items-center gap-3 px-1.5 text-[11.5px] text-ink-secondary"
        >
          {error ? (
            <span role="alert">IPC error: {error}</span>
          ) : info ? (
            <span>
              {info.name} {info.version} · core via {info.transport}
            </span>
          ) : (
            <span>Connecting to core…</span>
          )}
        </footer>
      </main>
    </div>
  );
}
