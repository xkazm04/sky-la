import {
  type BackupsDto,
  type CoreKind,
  commands,
  type EgressPolicyDto,
  type RefDataDto,
  type RulePackDto,
  type UpdateStatusDto,
  unwrap,
} from "@skyla/ipc";
import {
  type Appearance,
  applyAppearance,
  Badge,
  Button,
  Checkbox,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  SegmentedControl,
  Select,
  storeAppearance,
  storedAppearance,
  type TableColumn,
  TextField,
  Toolbar,
} from "@skyla/ui";
import { FileDown, FileUp, Monitor, Moon, Sun } from "lucide-react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { downloadBase64 } from "../download";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { connectionBadge } from "./AdvisorConnection";
import { Reasons, TwoLine } from "./common";

interface Setting {
  id: string;
  title: string;
  detail: string;
  value: ReactNode;
  about: string[];
  pack?: RulePackDto | undefined;
  refdata?: RefDataDto | undefined;
  policies?: EgressPolicyDto[] | undefined;
  backups?: BackupsDto | undefined;
  exportable?: boolean;
  updates?: UpdateStatusDto | undefined;
}

/** Which section of the list each setting sits in. */
const GROUP: Record<string, string> = {
  appearance: "general",
  encryption: "security",
  backups: "security",
  recovery: "security",
  advisors: "data",
  "advisor-sharing": "data",
  "reference-data": "data",
  export: "data",
  updates: "about",
};

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
  const refdata = useQuery("reference_data", () => unwrap(commands.referenceData()));
  const ref = refdata.state === "ready" ? refdata.data : undefined;
  const status = useQuery("advisor_status", () => commands.advisorStatus());
  const badge = connectionBadge(status.state === "ready" ? status.data : undefined);
  const policies = useQuery("egress_policies", () => commands.egressPolicies());
  const pol = policies.state === "ready" ? policies.data : undefined;
  const updates = useQuery("update_status", () => commands.updateStatus());
  const up = updates.state === "ready" ? updates.data : undefined;
  const backupList = useQuery("backups", () => unwrap(commands.backups()));
  const bk = backupList.state === "ready" ? backupList.data : undefined;

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
      value:
        bk && !bk.demo ? (
          <Badge tone="positive">Encrypted on this computer</Badge>
        ) : (
          <Badge tone="warning">Demo · in memory</Badge>
        ),
      about: [
        "Your books live in one encrypted file, opened with your passphrase or, if you chose, the key kept in your system keychain.",
        "The demo is built in memory and never written to disk.",
      ],
    },
    {
      id: "backups",
      title: "Backups",
      detail: "Encrypted copies of your books, made every day",
      value: bk?.demo ? (
        <Badge tone="neutral">Demo · in memory</Badge>
      ) : bk ? (
        <span className="text-ink-secondary">
          {bk.backups.length > 0 ? `Last ${bk.backups[0]?.createdAt.slice(0, 10)}` : "None yet"}
        </span>
      ) : (
        ""
      ),
      about: [
        "Each backup is encrypted with the same key as your books, so your passphrase and your recovery key both open it.",
        "Beside each one, a manifest records its content hash and the journal's hash-chain head, so a rewritten history can't pass as a backup.",
        "Check the newest backup now and then: sky-la restores it to a scratch folder and confirms it matches.",
      ],
      backups: bk,
    },
    {
      id: "recovery",
      title: "Recovery key",
      detail: "Printed once when you create your books",
      value:
        bk && !bk.demo ? (
          <span className="text-ink-secondary">Shown once at setup</span>
        ) : (
          <Badge tone="neutral">Not needed in the demo</Badge>
        ),
      about: [
        "The recovery key unlocks your books if you forget the passphrase. sky-la never stores it.",
        "Using it sets a new passphrase and replaces the key with a new one, shown once.",
      ],
    },
    {
      id: "advisors",
      title: "Advisors",
      detail: "Your own Claude Code installation",
      value: <Badge tone={badge.tone}>{badge.label}</Badge>,
      about: [
        "sky-la runs the claude command you installed. Sign in from your terminal with claude auth login; sky-la never sees your credentials.",
        "Every prompt and tool result passes the egress gate and is listed in the egress register.",
      ],
    },
    {
      id: "advisor-sharing",
      title: "What advisors may send",
      detail: "Per task: what it may share, and whether it runs",
      value: pol ? (
        <span className="text-ink-secondary">
          {pol.filter((p) => p.policy === "never").length > 0
            ? `${pol.filter((p) => p.policy !== "never").length} of ${pol.length} tasks allowed`
            : `Ask before each run · ${pol.length} tasks`}
        </span>
      ) : (
        ""
      ),
      about: [
        "IBANs, account numbers, personal ID numbers and card numbers are never sent, whatever a task may share.",
        "Customer and supplier names are replaced by stable pseudonyms (Customer A, Vendor B); sky-la puts the real names back in the answer on this machine.",
        "Fields outside a task's scope are dropped before anything leaves.",
      ],
      policies: pol,
    },
    {
      id: "reference-data",
      title: "Public reference data",
      detail: "ČNB exchange rates and signed rule-pack updates",
      value: ref?.fetchEnabled ? (
        <Badge tone="info">{`On · ${ref.fetchHost}`}</Badge>
      ) : (
        <Badge tone="neutral">Off · opt-in</Badge>
      ),
      about: [
        "Off by default. When on, sky-la asks only the ČNB for its published rates; importing the files by hand always works.",
        "Exchange rates value foreign payments; the repo-rate history sets statutory late interest.",
        "There is no telemetry.",
      ],
      refdata: ref,
    },
    {
      id: "export",
      title: "Export everything",
      detail: "The journal, chart and documents in open formats",
      value: <span className="text-ink-secondary">Zip · JSON, CSV, ISDOC, PDF</span>,
      about: [
        "Your books are yours: the export holds every posted entry with its hash-chain link, one CSV row per posting, the chart of accounts, and each issued document as ISDOC and PDF.",
        "A manifest lists the chain head and a SHA-256 of every file, so anyone can check nothing was changed after export.",
        "The same books always give the same file, byte for byte.",
      ],
      exportable: true,
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
      id: "updates",
      title: "Updates",
      detail: "Ask the project's releases whether a newer version is out",
      value: up?.enabled ? (
        <Badge tone="info">{`On · ${up.source}`}</Badge>
      ) : (
        <Badge tone="neutral">Off · opt-in</Badge>
      ),
      about: [
        "Off by default. When on, sky-la asks only the project's release page on GitHub, and only when you press Check now.",
        "It believes a release only when the project's release key signed it, and it never downloads or installs anything: you get the version and the page to download it from.",
        "Nothing about you or your books is sent; there is no telemetry.",
      ],
      updates: up,
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
  const inGroup = (group: string) => settings.filter((s) => (GROUP[s.id] ?? "about") === group);
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
            { id: "general", title: "General", rows: inGroup("general") },
            { id: "security", title: "Security", rows: inGroup("security") },
            { id: "advisors", title: "Advisors and data", rows: inGroup("data") },
            { id: "about", title: "About", rows: inGroup("about") },
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
            {selected.refdata && <ReferenceData data={selected.refdata} />}
            {selected.policies && <AdvisorSharing policies={selected.policies} />}
            {selected.backups && <Backups data={selected.backups} />}
            {selected.exportable && <ExportBooks demo={bk?.demo ?? true} />}
            {selected.updates && <UpdateCheck status={selected.updates} />}
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

/** The opt-in update check: a switch, a button, and what it found. */
function UpdateCheck({ status }: { status: UpdateStatusDto }) {
  const [result, setResult] = useState<{ ok: boolean; lines: string[] } | null>(null);
  const run = async (write: () => Promise<string>) => {
    setResult(null);
    try {
      const text = await write();
      invalidateAll();
      setResult({ ok: true, lines: [text] });
    } catch (e) {
      setResult({ ok: false, lines: problems(e) });
    }
  };
  return (
    <>
      <InspectorSection title={`This version · ${status.current}`}>
        <Checkbox
          isSelected={status.enabled}
          onChange={(enabled) =>
            void run(async () => {
              await commands.setUpdateCheck(enabled);
              return enabled
                ? `The update check is on; sky-la may ask ${status.source} when you check.`
                : "The update check is off.";
            })
          }
        >
          Allow checking {status.source} for a newer version
        </Checkbox>
        {status.trustedKeys === 0 && (
          <p className="mt-2 text-footnote text-ink-secondary">
            This build trusts no release key yet, so it won't check; download new versions from the
            project page yourself.
          </p>
        )}
      </InspectorSection>
      {status.available && (
        <InspectorSection title={`Version ${status.available.version} is out`}>
          <p className="text-body">{status.available.notes}</p>
          <p className="mt-1 select-all text-footnote text-ink-secondary">
            {status.available.page}
          </p>
        </InspectorSection>
      )}
      <div className="mt-3 flex gap-2">
        <Button
          isDisabled={!status.enabled}
          onPress={() =>
            void run(async () => {
              const s = await unwrap(commands.checkForUpdate());
              return s.available
                ? `Version ${s.available.version} is available.`
                : `${s.current} is the newest version.`;
            })
          }
        >
          Check now
        </Button>
      </div>
      {result && (
        <div
          role={result.ok ? "status" : "alert"}
          className={`mt-2 text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
        >
          {result.lines.map((l) => (
            <p key={l}>{l}</p>
          ))}
        </div>
      )}
    </>
  );
}

/** The full export, saved as a zip. */
function ExportBooks({ demo }: { demo: boolean }) {
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [pass, setPass] = useState("");
  const run = async () => {
    setBusy(true);
    try {
      const e = await unwrap(commands.exportBooks(demo ? null : pass));
      setPass("");
      downloadBase64(e.fileName, e.contentBase64, "application/zip");
      setResult({
        ok: true,
        text: `Saved ${e.fileName}: ${e.files} files, ${Math.round(e.bytes / 1024)} kB.`,
      });
    } catch (err) {
      setResult({ ok: false, text: problems(err).join(" ") });
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <form
        aria-label="Export"
        className="mt-3 flex items-end gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        {!demo && (
          <TextField
            className="flex-1"
            label="Your passphrase, to confirm"
            type="password"
            value={pass}
            onChange={setPass}
            autoComplete="current-password"
          />
        )}
        <Button type="submit" icon={FileDown} isDisabled={busy || (!demo && pass === "")}>
          Export…
        </Button>
      </form>
      {result && (
        <p
          role={result.ok ? "status" : "alert"}
          className={`mt-2 text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
        >
          {result.text}
        </p>
      )}
    </>
  );
}

/** Backups: the list, a backup now, and the restore drill. */
function Backups({ data }: { data: BackupsDto }) {
  const [result, setResult] = useState<{ ok: boolean; lines: string[] } | null>(null);
  if (data.demo) {
    return (
      <InspectorSection title="This demo">
        <p className="text-body text-ink-secondary">
          The demo keeps its books in memory, so there's nothing to back up. Your own books are
          backed up every day to a Backups folder beside them.
        </p>
      </InspectorSection>
    );
  }
  const run = async (action: () => Promise<string>) => {
    try {
      const line = await action();
      invalidateAll();
      setResult({ ok: true, lines: [line] });
    } catch (e) {
      setResult({ ok: false, lines: problems(e) });
    }
  };
  return (
    <>
      <InspectorSection title={`Backups · every ${data.everyDays} day(s), ${data.keep} kept`}>
        <FactList
          facts={[
            { label: "Folder", value: data.folder ?? "—" },
            ...data.backups.map((b) => ({
              label: b.createdAt.replace("T", " ").replace("Z", " UTC"),
              value: `${Math.round(b.bytes / 1024)} kB`,
            })),
          ]}
        />
      </InspectorSection>
      <div className="mt-3 flex gap-2">
        <Button
          onPress={() =>
            void run(async () => `Backed up at ${(await unwrap(commands.backupNow())).createdAt}.`)
          }
        >
          Back up now
        </Button>
        <Button
          onPress={() =>
            void run(async () => {
              const d = await unwrap(commands.restoreDrill());
              return d.passed
                ? "The newest backup restores and matches its manifest."
                : "The newest backup doesn't match its manifest. Make a new one and keep the old for now.";
            })
          }
        >
          Check the newest backup
        </Button>
      </div>
      {result && (
        <p
          role={result.ok ? "status" : "alert"}
          className={`mt-2 text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
        >
          {result.lines.join(" ")}
        </p>
      )}
    </>
  );
}

const POLICIES = [
  { id: "ask", label: "Ask before each run" },
  { id: "always", label: "Run without asking" },
  { id: "never", label: "Never run" },
] as const;

/** Each advisor task: what it may share and whether it runs. The core keeps the policy. */
function AdvisorSharing({ policies }: { policies: EgressPolicyDto[] }) {
  const [error, setError] = useState<string[]>([]);
  const change = async (task: string, policy: string) => {
    try {
      await unwrap(commands.setEgressPolicy(task, policy));
      setError([]);
      invalidateAll();
    } catch (e) {
      setError(problems(e));
    }
  };
  return (
    <>
      {policies.map((p) => (
        <InspectorSection key={p.task} title={p.label}>
          <div className="flex flex-col gap-2">
            <Select
              label={`${p.label}: when it runs`}
              labelHidden
              options={POLICIES}
              value={p.policy}
              onChange={(v) => void change(p.task, v)}
            />
            <p className="text-footnote text-ink-secondary">
              {p.advisor} may send: {p.scope.join("; ")}.
            </p>
          </div>
        </InspectorSection>
      ))}
      {error.length > 0 && (
        <p role="alert" className="mt-2 text-footnote text-negative-ink">
          {error.join(" ")}
        </p>
      )}
    </>
  );
}

/** Reference data: the fetch switch, imports by hand, and where each set came from. */
function ReferenceData({ data }: { data: RefDataDto }) {
  const [result, setResult] = useState<{ ok: boolean; lines: string[] } | null>(null);
  const fx = useRef<HTMLInputElement>(null);
  const repo = useRef<HTMLInputElement>(null);
  const run = async (write: () => Promise<string>) => {
    setResult(null);
    try {
      const text = await write();
      invalidateAll();
      setResult({ ok: true, lines: [text] });
    } catch (e) {
      setResult({ ok: false, lines: problems(e) });
    }
  };
  const importFile = (kind: "cnb_fx" | "cnb_repo", file: File) =>
    void run(async () => {
      const text = await file.text();
      await unwrap(commands.importReferenceData(kind, file.name, text));
      return `Imported ${file.name}.`;
    });
  const picker = (kind: "cnb_fx" | "cnb_repo", label: string, input: typeof fx) => (
    <input
      ref={input}
      type="file"
      accept=".txt,.csv"
      aria-label={label}
      className="hidden"
      onChange={(e) => {
        const f = e.target.files?.[0];
        if (f) importFile(kind, f);
        e.target.value = "";
      }}
    />
  );
  return (
    <>
      <InspectorSection title="Fetching">
        <Checkbox
          isSelected={data.fetchEnabled}
          onChange={(enabled) =>
            void run(async () => {
              await unwrap(commands.setReferenceFetch(enabled));
              return enabled
                ? `Fetching is on; sky-la may ask ${data.fetchHost} for its published rates.`
                : "Fetching is off.";
            })
          }
        >
          Fetch the ČNB's published rates from {data.fetchHost}
        </Checkbox>
      </InspectorSection>
      <InspectorSection title="Loaded">
        <FactList
          facts={[
            { label: "Exchange rates", value: data.euro ?? "none yet" },
            {
              label: "Repo rate",
              value:
                data.repoNow ?? (data.repoChanges > 0 ? `${data.repoChanges} changes` : "none yet"),
            },
            {
              label: "Pack signing keys",
              value:
                data.trustedKeys > 0
                  ? `${data.trustedKeys} trusted`
                  : "none yet: updates can't be installed",
            },
          ]}
        />
        <div className="mt-2 flex flex-wrap gap-2">
          {picker("cnb_fx", "ČNB rates file", fx)}
          {picker("cnb_repo", "Repo-rate history file", repo)}
          <Button icon={FileUp} onPress={() => fx.current?.click()}>
            Import ČNB rates…
          </Button>
          <Button icon={FileUp} onPress={() => repo.current?.click()}>
            Import repo history…
          </Button>
        </div>
        {result && (
          <div
            role={result.ok ? "status" : "alert"}
            className={`mt-2 text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
          >
            {result.lines.map((l) => (
              <p key={l}>{l}</p>
            ))}
          </div>
        )}
      </InspectorSection>
      {data.sources.length > 0 && (
        <InspectorSection title={`Where it came from · ${data.sources.length}`}>
          <ul className="space-y-1 text-footnote">
            {data.sources.map((s) => (
              <li key={`${s.kind}-${s.origin}-${s.summary}`}>
                <span className="font-medium text-ink">{s.summary}</span>
                <span className="block text-ink-secondary">{s.origin}</span>
              </li>
            ))}
          </ul>
        </InspectorSection>
      )}
    </>
  );
}
