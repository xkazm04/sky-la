import { type CoreKind, commands, unwrap } from "@skyla/ipc";
import { AppWindow, SourceList, StatusBar, type StatusItem } from "@skyla/ui";
import {
  ChartNoAxesColumn,
  FileText,
  FlaskConical,
  Inbox,
  Landmark,
  LayoutGrid,
  Percent,
  PlugZap,
  ScrollText,
  Send,
  Settings,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { createContext, type ReactNode, useContext, useState } from "react";
import { createPortal } from "react-dom";
import { useQuery } from "../data";
import { navigate, type Screen } from "../router";

const InspectorSlot = createContext<HTMLElement | null>(null);

/** Renders the current screen's inspector into the shell's right pane. */
export function InspectorPane({ children }: { children: ReactNode }) {
  const slot = useContext(InspectorSlot);
  return slot ? createPortal(children, slot) : null;
}

function count(n: number | undefined): number | undefined {
  return n === undefined || n === 0 ? undefined : n;
}

/**
 * The window that stays put while screens change: the source list, the
 * entity, the inspector slot and the status line.
 */
export function Shell({
  core,
  screen,
  children,
}: {
  core: CoreKind;
  screen: Screen;
  children: ReactNode;
}) {
  const [slot, setSlot] = useState<HTMLElement | null>(null);
  const entity = useQuery("entity", () => commands.entity());
  const info = useQuery("app_info", () => commands.appInfo());
  const integrity = useQuery("integrity", () => unwrap(commands.integrity()));
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  const invoices = useQuery("invoices", () => unwrap(commands.invoices()));
  const bank = useQuery("bank_statement", () => unwrap(commands.bankStatement()));
  const register = useQuery("egress_register", () => unwrap(commands.egressRegister()));

  const ready = <T,>(q: { state: string; data?: T }) => (q.state === "ready" ? q.data : undefined);
  const asOf = ready(entity)?.asOf;
  const sections = [
    {
      id: "main",
      items: [
        { id: "overview", label: "Overview", icon: LayoutGrid },
        { id: "inbox", label: "Inbox", icon: Inbox, count: count(ready(proposals)?.length) },
      ],
    },
    {
      id: "books",
      title: "Books",
      items: [
        {
          id: "invoices",
          label: "Invoices",
          icon: FileText,
          count: count(
            ready(invoices)?.filter((i) => ["open", "overdue", "partPaid"].includes(i.status))
              .length,
          ),
        },
        {
          id: "bank",
          label: "Bank",
          icon: Landmark,
          count: count(ready(bank)?.lines.filter((l) => l.status !== "booked").length),
        },
        { id: "statements", label: "Statements", icon: ChartNoAxesColumn },
        { id: "taxes", label: "Taxes", icon: Percent },
      ],
    },
    {
      id: "assist",
      title: "Assist",
      items: [{ id: "advisors", label: "Advisors", icon: Sparkles }],
    },
    {
      id: "app",
      title: "App",
      items: [
        { id: "register", label: "Egress register", icon: ScrollText },
        { id: "settings", label: "Settings", icon: Settings },
      ],
    },
  ];

  const status: StatusItem[] = [{ id: "demo", icon: FlaskConical, label: "Demo data · in memory" }];
  if (integrity.state === "ready") {
    const i = integrity.data;
    status.push(
      i.chainIntact && i.balanced
        ? {
            id: "journal",
            icon: ShieldCheck,
            label: "Journal balanced · chain verified",
            tone: "positive",
          }
        : {
            id: "journal",
            icon: ShieldAlert,
            label: i.firstBreak ?? "Journal doesn't balance",
            tone: "negative",
          },
    );
  }
  const runs = ready(register);
  if (runs && asOf) {
    const today = runs.filter((r) => r.at.startsWith(asOf)).length;
    status.push({
      id: "egress",
      icon: Send,
      label:
        today === 0
          ? "Nothing sent to Claude today"
          : `${today} advisor run${today === 1 ? "" : "s"} sent today`,
    });
  }
  const core_ =
    info.state === "ready"
      ? `${info.data.name} ${info.data.version} · core via ${core}`
      : "Connecting to core…";
  status.push({ id: "core", icon: PlugZap, label: core_ });

  const e = ready(entity);
  return (
    <InspectorSlot.Provider value={slot}>
      <AppWindow
        sidebar={
          <nav aria-label="Sections" className="flex min-h-0 flex-1 flex-col">
            <p className="px-2.5 pb-3 font-semibold text-title">sky-la</p>
            <SourceList
              label="Sections"
              sections={sections}
              selectedId={screen}
              onSelect={(id) => navigate(id as Screen)}
            />
            <div className="mt-auto px-2.5 pt-3">
              <p className="font-medium text-body">{e?.displayName ?? "…"}</p>
              <p className="text-footnote text-ink-secondary">
                {e ? `${e.legalForm} · ${e.bankName}` : ""}
              </p>
            </div>
          </nav>
        }
        inspector={<div ref={setSlot} className="h-full" />}
        status={<StatusBar data-testid="status-line" items={status} />}
      >
        {children}
      </AppWindow>
    </InspectorSlot.Provider>
  );
}
