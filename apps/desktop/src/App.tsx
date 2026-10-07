import { demoEntity } from "@skyla/fixtures";
import { type AppInfo, type CoreKind, commands, type IntegrityDto, unwrap } from "@skyla/ipc";
import { AppWindow, ContentGroup, SourceList, StatusBar, Toolbar } from "@skyla/ui";
import {
  ChartNoAxesColumn,
  FileText,
  FlaskConical,
  Inbox,
  Landmark,
  LayoutGrid,
  Percent,
  PlugZap,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Gallery } from "./gallery/Gallery";
import { useRoute } from "./route";

const sections = [
  {
    id: "main",
    items: [
      { id: "overview", label: "Overview", icon: LayoutGrid },
      { id: "inbox", label: "Inbox", icon: Inbox },
      { id: "invoices", label: "Invoices", icon: FileText },
      { id: "bank", label: "Bank", icon: Landmark },
      { id: "statements", label: "Statements", icon: ChartNoAxesColumn },
      { id: "taxes", label: "Taxes", icon: Percent },
      { id: "advisors", label: "Advisors", icon: Sparkles },
    ],
  },
];

/**
 * The shell in direction A's chrome. Screens arrive in WP-10; `#/gallery`
 * shows the design system (WP-08).
 */
export function App({ core }: { core: CoreKind }) {
  const route = useRoute();
  const [section, setSection] = useState("overview");
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [integrity, setIntegrity] = useState<IntegrityDto | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    commands.appInfo().then(setInfo, (e: unknown) => setError(String(e)));
    unwrap(commands.integrity()).then(setIntegrity, (e: unknown) => setError(String(e)));
  }, []);

  if (route === "gallery") return <Gallery />;

  const status = error
    ? `IPC error: ${error}`
    : info
      ? `${info.name} ${info.version} · core via ${core}`
      : "Connecting to core…";

  return (
    <AppWindow
      sidebar={
        <nav aria-label="Sections" className="flex min-h-0 flex-1 flex-col">
          <p className="px-2.5 pb-3 font-semibold text-title">sky-la</p>
          <SourceList
            label="Sections"
            sections={sections}
            selectedId={section}
            onSelect={setSection}
          />
          <p className="mt-auto px-2.5 text-footnote text-ink-secondary">
            {demoEntity.displayName} · {demoEntity.legalForm}
          </p>
        </nav>
      }
      status={
        <StatusBar
          data-testid="status-line"
          items={[
            { id: "demo", icon: FlaskConical, label: "Demo data · in memory" },
            ...(integrity
              ? [
                  integrity.chainIntact && integrity.balanced
                    ? {
                        id: "journal",
                        icon: ShieldCheck,
                        label: `Journal balanced · chain verified (${integrity.entriesChecked} entries)`,
                        tone: "positive" as const,
                      }
                    : {
                        id: "journal",
                        icon: ShieldAlert,
                        label: integrity.firstBreak ?? "Journal doesn't balance",
                        tone: "negative" as const,
                      },
                ]
              : []),
            { id: "core", icon: PlugZap, label: status, tone: error ? "negative" : "neutral" },
          ]}
          role={error ? "alert" : undefined}
        />
      }
    >
      <Toolbar title="Nothing here yet" subtitle="Screens arrive in WP-10" />
      <ContentGroup className="p-6">
        <p className="max-w-prose text-ink-secondary">
          The ledger kernel is complete (M1). The design system is in the gallery at #/gallery; the
          screens follow in WP-10, as listed in docs/plan/IMPLEMENTATION_PLAN.md.
        </p>
      </ContentGroup>
    </AppWindow>
  );
}
