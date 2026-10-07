import { type CoreKind, commands } from "@skyla/ipc";
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
            { id: "advisors", title: "Advisors and data", rows: settings.slice(3, 5) },
            { id: "about", title: "About", rows: settings.slice(5) },
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
          </Inspector>
        </InspectorPane>
      )}
    </>
  );
}
