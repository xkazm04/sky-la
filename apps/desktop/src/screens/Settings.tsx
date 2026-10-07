import { type CoreKind, commands, type RulePackDto } from "@skyla/ipc";
import {
  type Appearance,
  applyAppearance,
  Badge,
  ContentGroup,
  DataTable,
  Inspector,
  InspectorSection,
  SegmentedControl,
  storeAppearance,
  storedAppearance,
  type TableColumn,
  Toolbar,
} from "@skyla/ui";
import { Monitor, Moon, Sun } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useQuery } from "../data";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { Reasons, TwoLine } from "./common";

interface Setting {
  id: string;
  title: string;
  detail: string;
  value: ReactNode;
  about: string[];
  pack?: RulePackDto | undefined;
}

const columns: TableColumn<Setting>[] = [
  {
    id: "title",
    title: "Setting",
    isRowHeader: true,
    cell: (s) => <TwoLine title={s.title} detail={s.detail} />,
  },
  { id: "value", title: "Value", width: "16rem", cell: (s) => s.value },
];

/** Settings: appearance, security, advisors, data, about. */
export function SettingsScreen({ item, core }: { item: string | null; core: CoreKind }) {
  const [appearance, setAppearance] = useState<Appearance>(storedAppearance);
  useEffect(() => {
    storeAppearance(appearance);
    return applyAppearance(appearance);
  }, [appearance]);
  const info = useQuery("app_info", () => commands.appInfo());
  const pack = useQuery("rule_pack", () => commands.rulePack());

  const settings: Setting[] = [
    {
      id: "appearance",
      title: "Appearance",
      detail: "Light, dark, or follow the system",
      value: (
        <SegmentedControl
          label="Appearance"
          segments={[
            { id: "system", label: "System", icon: Monitor },
            { id: "light", label: "Light", icon: Sun },
            { id: "dark", label: "Dark", icon: Moon },
          ]}
          value={appearance}
          onChange={setAppearance}
        />
      ),
      about: ["Dark is a first-class appearance with its own greys and elevation."],
    },
    {
      id: "encryption",
      title: "Encryption",
      detail: "SQLCipher with a key in your OS keychain",
      value: <Badge tone="warning">Demo · in memory</Badge>,
      about: [
        "Real books live in an encrypted file. The demo entity is built in memory and never written to disk.",
        "Encryption, the recovery key and auto-lock arrive with the unlock flow (WP-30).",
      ],
    },
    {
      id: "recovery",
      title: "Recovery key",
      detail: "Printed once when you create your books",
      value: <Badge tone="neutral">Not needed in the demo</Badge>,
      about: [
        "The recovery key unlocks your books if you forget the passphrase. sky-la never stores it.",
      ],
    },
    {
      id: "advisors",
      title: "Advisors",
      detail: "Your own Claude Code installation",
      value: <Badge tone="neutral">Demo · not connected</Badge>,
      about: [
        "sky-la runs the claude command you installed. Sign in from your terminal with claude auth login; sky-la never sees your credentials.",
        "Every prompt and tool result passes the egress gate and is listed in the egress register.",
      ],
    },
    {
      id: "reference-data",
      title: "Public reference data",
      detail: "ČNB exchange rates and signed rule-pack updates",
      value: <Badge tone="neutral">Off · opt-in</Badge>,
      about: [
        "Off by default. When on, sky-la fetches only signed public data; manual import is always available.",
        "There is no telemetry.",
      ],
    },
    {
      id: "rule-pack",
      title: "Rule pack",
      detail: "Statutory rates, thresholds and deadlines, each with its source",
      value:
        pack.state === "ready" ? (
          <Badge tone={pack.data.review === "draft" ? "warning" : "positive"}>
            {`${pack.data.provenance} · ${pack.data.review}`}
          </Badge>
        ) : (
          "…"
        ),
      about:
        pack.state === "ready"
          ? [
              pack.data.summary,
              pack.data.review === "draft"
                ? "Draft: compiled from the cited provisions, not yet verified line by line by a second person."
                : "Reviewed against the official texts.",
            ]
          : [],
      pack: pack.state === "ready" ? pack.data : undefined,
    },
    {
      id: "about",
      title: "About",
      detail: "Open source: AGPL-3.0 app, Apache-2.0 engine",
      value:
        info.state === "ready" ? `${info.data.name} ${info.data.version} · core via ${core}` : "…",
      about: [
        "The ledger, money and rule-pack crates are Apache-2.0 so other tools can build on them.",
      ],
    },
  ];
  const selected = settings.find((s) => s.id === item) ?? settings[0];
  return (
    <>
      <Toolbar
        title="Settings"
        subtitle="Local-first: your books never leave this machine unless you send them"
      />
      <ContentGroup>
        <DataTable
          label="Settings"
          columns={columns}
          sections={[
            { id: "general", title: "General", rows: settings.slice(0, 1) },
            { id: "security", title: "Security", rows: settings.slice(1, 3) },
            { id: "advisors", title: "Advisors and data", rows: settings.slice(3, 6) },
            { id: "about", title: "About", rows: settings.slice(6) },
          ]}
          selectedId={selected?.id ?? null}
          onSelect={(id) => navigate("settings", id, true)}
        />
      </ContentGroup>
      {selected && (
        <InspectorPane>
          <Inspector label={selected.title} title={selected.title} subtitle={selected.detail}>
            <InspectorSection title="About this setting">
              <Reasons reasons={selected.about} />
            </InspectorSection>
            {selected.pack && (
              <>
                <InspectorSection title={`Values in force · ${selected.pack.values.length}`}>
                  <ul className="overflow-hidden rounded-inner bg-surface shadow-group">
                    {selected.pack.values.map((v) => (
                      <li
                        key={v.key}
                        className="border-hairline border-t px-3 py-2 first:border-t-0"
                      >
                        <div className="flex items-baseline justify-between gap-2">
                          <span className="truncate font-medium">{v.key}</span>
                          <span className="shrink-0">
                            {v.value}
                            {v.kind === "percent" ? " %" : ""}
                          </span>
                        </div>
                        <a
                          href={v.url}
                          className="block text-footnote text-accent-ink underline-offset-2 hover:underline"
                          rel="noreferrer"
                          target="_blank"
                        >
                          {v.citation}
                        </a>
                      </li>
                    ))}
                  </ul>
                </InspectorSection>
                <InspectorSection title="Left out on purpose">
                  <Reasons reasons={selected.pack.omitted} />
                </InspectorSection>
              </>
            )}
          </Inspector>
        </InspectorPane>
      )}
    </>
  );
}
