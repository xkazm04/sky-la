import {
  type BankLineDto,
  type BankStatementDto,
  commands,
  type MatchCandidateDto,
  unwrap,
} from "@skyla/ipc";
import {
  Badge,
  Button,
  Checkbox,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  Popup,
  SegmentedControl,
  Select,
  type TableColumn,
  TextField,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { CircleCheck, FileUp, Plus, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { fileToBase64 } from "../download";
import { day, money, signed } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { EmptyInspector, Loaded, TwoLine } from "./common";

type Filter = "attention" | "booked" | "all";

const STATUS: Record<string, { tone: Tone; label: string }> = {
  certain: { tone: "positive", label: "Certain" },
  needs_you: { tone: "warning", label: "Needs you" },
  booked: { tone: "neutral", label: "Booked" },
};

const columns: TableColumn<BankLineDto>[] = [
  { id: "date", title: "Date", width: "5.5rem", cell: (l) => day(l.date) },
  {
    id: "who",
    title: "Counterparty · reference",
    isRowHeader: true,
    cell: (l) => (
      <TwoLine title={l.counterparty} detail={l.bookedAs ?? l.proposal ?? l.reference} />
    ),
  },
  { id: "amount", title: "Amount", align: "end", width: "8.5rem", cell: (l) => signed(l.amount) },
  {
    id: "status",
    title: "Status",
    width: "8.5rem",
    cell: (l) => {
      const s = STATUS[l.status] ?? { tone: "warning" as Tone, label: "Needs you" };
      return <Badge tone={s.tone}>{s.label}</Badge>;
    },
  },
];

/** Runs a write, refreshes everything the core may have changed, and reports. */
function useWrite() {
  const [result, setResult] = useState<{ ok: boolean; lines: string[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (write: () => Promise<string>) => {
    setBusy(true);
    setResult(null);
    try {
      const text = await write();
      invalidateAll();
      setResult({ ok: true, lines: [text] });
    } catch (e) {
      setResult({ ok: false, lines: problems(e) });
    } finally {
      setBusy(false);
    }
  };
  return { result, busy, run };
}

function Result({ result }: { result: { ok: boolean; lines: string[] } | null }) {
  if (!result) return null;
  return (
    <div
      role={result.ok ? "status" : "alert"}
      className={`mb-2 px-2 text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
    >
      {result.lines.map((l) => (
        <p key={l}>{l}</p>
      ))}
    </div>
  );
}

function Candidate({ c }: { c: MatchCandidateDto }) {
  return (
    <div className="rounded-inner bg-surface p-3 shadow-group">
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="font-medium text-body">{c.label}</p>
          <p className="text-footnote text-ink-secondary">{c.detail}</p>
        </div>
        <Badge tone={c.confidence === "likely" ? "info" : "neutral"}>Score {c.score}</Badge>
      </div>
      <ul className="mt-2 space-y-1 text-footnote">
        {c.contributions.map((part) => (
          <li key={part.reason} className="flex justify-between gap-3">
            <span className="text-ink-secondary">{part.reason}</span>
            <span className="tabular-nums">{part.weight}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

interface Row {
  key: number;
  account: string | null;
  vatCode: string;
  amount: string;
}

let rowKey = 1;

/** Books a line to one account, or splits it across several; the core
 * computes the VAT from each row's gross. */
function SplitEditor({
  line,
  s,
  onBook,
}: {
  line: BankLineDto;
  s: BankStatementDto;
  onBook: (rows: Row[]) => void;
}) {
  const [rows, setRows] = useState<Row[]>(() => [
    { key: rowKey++, account: null, vatCode: "IN21", amount: "" },
  ]);
  const accounts = s.accounts.map((a) => ({ id: a.code, label: `${a.code} ${a.name}` }));
  const vat = [
    ...s.vatCodes.map((v) => ({ id: v.code, label: `${v.ratePercent} %`, detail: v.name })),
    { id: "", label: "No VAT" },
  ];
  const update = (key: number, patch: Partial<Row>) =>
    setRows((all) => all.map((r) => (r.key === key ? { ...r, ...patch } : r)));
  return (
    <div className="w-[26rem] text-body">
      <p className="font-semibold">
        Split {money({ ...line.amount, minor: Math.abs(line.amount.minor) })}
      </p>
      <p className="mt-0.5 text-footnote text-ink-secondary">
        Gross amounts per row; the VAT is split out by the core.
      </p>
      <div className="mt-3 grid grid-cols-[minmax(0,1fr)_5.5rem_6.5rem_1.75rem] items-center gap-2">
        {rows.map((r, i) => (
          <div key={r.key} className="contents">
            <Select
              label={`Row ${i + 1} account`}
              labelHidden
              options={accounts}
              value={r.account}
              onChange={(account) => update(r.key, { account })}
              placeholder="Account…"
            />
            <Select
              label={`Row ${i + 1} VAT`}
              labelHidden
              options={vat}
              value={r.vatCode}
              onChange={(vatCode) => update(r.key, { vatCode })}
            />
            <TextField
              label={`Row ${i + 1} amount`}
              labelHidden
              numeric
              placeholder="1 200,00"
              value={r.amount}
              onChange={(amount) => update(r.key, { amount })}
            />
            <Button
              variant="plain"
              icon={Trash2}
              aria-label={`Remove row ${i + 1}`}
              isDisabled={rows.length === 1}
              onPress={() => setRows((all) => all.filter((x) => x.key !== r.key))}
            />
          </div>
        ))}
      </div>
      <div className="mt-3 flex items-center justify-between">
        <Button
          variant="plain"
          icon={Plus}
          className="-ml-2"
          onPress={() =>
            setRows((all) => [
              ...all,
              { key: rowKey++, account: null, vatCode: "IN21", amount: "" },
            ])
          }
        >
          Add row
        </Button>
        <Button variant="primary" onPress={() => onBook(rows)}>
          {rows.length > 1 ? "Book split" : "Book"}
        </Button>
      </div>
    </div>
  );
}

/** A rule from this line: the payee's account (or name) → an account. */
function RuleEditor({
  s,
  onCreate,
}: {
  s: BankStatementDto;
  onCreate: (r: { name: string; account: string | null; vatCode: string; auto: boolean }) => void;
}) {
  const [name, setName] = useState("");
  const [account, setAccount] = useState<string | null>(null);
  const [vatCode, setVatCode] = useState("");
  const [auto, setAuto] = useState(true);
  return (
    <div className="flex w-80 flex-col gap-3 text-body">
      <p className="font-semibold">Create a rule from this line</p>
      <TextField label="Rule name" value={name} onChange={setName} placeholder="Office rent" />
      <Select
        label="Book to"
        options={s.accounts.map((a) => ({ id: a.code, label: `${a.code} ${a.name}` }))}
        value={account}
        onChange={setAccount}
        placeholder="Account…"
      />
      <Select
        label="VAT"
        options={[
          { id: "", label: "No VAT" },
          ...s.vatCodes.map((v) => ({ id: v.code, label: `${v.ratePercent} %`, detail: v.name })),
        ]}
        value={vatCode}
        onChange={setVatCode}
      />
      <Checkbox isSelected={auto} onChange={setAuto}>
        Accept matching lines with the certain ones
      </Checkbox>
      <div className="flex justify-end">
        <Button variant="primary" onPress={() => onCreate({ name, account, vatCode, auto })}>
          Create rule and book
        </Button>
      </div>
    </div>
  );
}

function LineInspector({
  line,
  s,
  write,
}: {
  line: BankLineDto;
  s: BankStatementDto;
  write: ReturnType<typeof useWrite>;
}) {
  const out = line.amount.minor < 0;
  return (
    <InspectorPane>
      <Inspector
        label={`Bank line ${line.counterparty}`}
        title={line.counterparty}
        subtitle={`${day(line.date, true)}${line.reference ? ` · ${line.reference}` : ""}`}
        accessory={<span className="font-semibold text-title">{signed(line.amount)}</span>}
        actions={
          line.status === "needs_you" ? (
            <>
              {line.proposalId && (
                <Button variant="plain" onPress={() => navigate("inbox", line.proposalId)}>
                  Open in Inbox
                </Button>
              )}
              {out && (
                <Popup
                  label="Create rule"
                  placement="top end"
                  trigger={<Button variant="plain">Create rule…</Button>}
                >
                  <RuleEditor
                    s={s}
                    onCreate={(r) =>
                      void write.run(async () => {
                        await unwrap(
                          commands.createBankRule(line.id, {
                            name: r.name,
                            account: r.account ?? "",
                            vatCode: r.vatCode || null,
                            autoAccept: r.auto,
                          }),
                        );
                        return `Rule "${r.name}" created; ${line.counterparty} is booked.`;
                      })
                    }
                  />
                </Popup>
              )}
              {out && (
                <Popup
                  label="Book line"
                  placement="top end"
                  trigger={<Button variant="primary">Book…</Button>}
                >
                  <SplitEditor
                    line={line}
                    s={s}
                    onBook={(rows) =>
                      void write.run(async () => {
                        await unwrap(
                          commands.bookBankLine(
                            line.id,
                            rows.map((r) => ({
                              entryId: null,
                              account: r.account,
                              vatCode: r.vatCode || null,
                              amount: r.amount,
                            })),
                          ),
                        );
                        return rows.length > 1
                          ? `${line.counterparty} is booked across ${rows.length} accounts.`
                          : `${line.counterparty} is booked.`;
                      })
                    }
                  />
                </Popup>
              )}
            </>
          ) : line.status === "certain" ? (
            <Button
              variant="primary"
              isDisabled={write.busy}
              onPress={() =>
                void write.run(async () => {
                  await unwrap(commands.acceptBankLine(line.id));
                  return `${line.counterparty} is booked: ${line.proposal ?? "accepted"}.`;
                })
              }
            >
              Accept
            </Button>
          ) : line.status === "booked" ? (
            <Popup
              label="Undo booking"
              placement="top end"
              trigger={<Button variant="plain">Undo booking…</Button>}
            >
              {(close) => (
                <div className="flex w-72 flex-col gap-3 text-body">
                  <p>
                    Posts a reversal of entry #{line.entryId}. The booking stays in the journal,
                    cancelled by the reversal, and the line waits for a decision again.
                  </p>
                  <div className="flex justify-end">
                    <Button
                      variant="destructive"
                      isDisabled={write.busy}
                      onPress={() => {
                        close();
                        void write.run(async () => {
                          await unwrap(commands.unbookBankLine(line.id));
                          return `The booking of ${line.counterparty} is reversed.`;
                        });
                      }}
                    >
                      Post reversal
                    </Button>
                  </div>
                </div>
              )}
            </Popup>
          ) : undefined
        }
      >
        {line.status === "booked" && (
          <InspectorSection title="Booked">
            <FactList
              facts={[
                { label: "As", value: line.bookedAs ?? "—" },
                { label: "Journal entry", value: line.entryId ? `#${line.entryId}` : "—" },
              ]}
            />
          </InspectorSection>
        )}
        {line.status !== "booked" && line.proposal && (
          <InspectorSection title="Proposal">
            <p className="text-body font-medium">{line.proposal}</p>
            <p className="mt-0.5 text-footnote text-ink-secondary">
              {line.status === "certain"
                ? "Certain: accepted with the others when you press Accept."
                : `Held: ${line.heldBecause ?? "a person decides"}.`}
            </p>
          </InspectorSection>
        )}
        {line.status === "needs_you" && !line.proposal && (
          <InspectorSection title="Why it waits">
            <p className="text-body">
              {line.heldBecause ?? "Nothing in the books fits this line yet."}
            </p>
          </InspectorSection>
        )}
        {line.status !== "booked" && line.candidates.length > 0 && (
          <InspectorSection title="Candidates · scored by named signals">
            <div className="space-y-2">
              {line.candidates.map((c) => (
                <Candidate key={c.label} c={c} />
              ))}
            </div>
          </InspectorSection>
        )}
        {line.counterpartyAccount && (
          <p className="mt-3 text-footnote text-ink-secondary">
            Counter-account {line.counterpartyAccount}
          </p>
        )}
      </Inspector>
    </InspectorPane>
  );
}

/** Bank workbench (pattern D): import, tie-out, explained matches, splits and rules. */
export function BankScreen({ item }: { item: string | null }) {
  const [filter, setFilter] = useState<Filter>("attention");
  const statement = useQuery("bank_statement", () => unwrap(commands.bankStatement()));
  const write = useWrite();
  const fileInput = useRef<HTMLInputElement>(null);

  const importFile = (file: File) =>
    void write.run(async () => {
      const s = await unwrap(commands.importBankStatement(file.name, await fileToBase64(file)));
      const last = s.imports.at(-1);
      return `Imported ${file.name}: ${last?.lines ?? 0} new lines; it ties out to ${money(s.closing)}.`;
    });

  return (
    <Loaded query={statement}>
      {(s) => {
        const certain = s.lines.filter((l) => l.status === "certain");
        const needs = s.lines.filter((l) => l.status === "needs_you");
        const booked = s.lines.filter((l) => l.status === "booked");
        const keep = (l: BankLineDto) =>
          filter === "all" || (filter === "booked" ? l.status === "booked" : l.status !== "booked");
        const selected = s.lines.find((l) => l.id === item) ?? s.lines.find(keep) ?? s.lines[0];
        return (
          <>
            <Toolbar
              title={s.accountName}
              subtitle={`${s.file} · ${s.format} · ${day(s.from)} – ${day(s.to)} · ${needs.length} need you, ${certain.length} certain`}
            >
              <SegmentedControl
                label="Show lines"
                segments={[
                  { id: "attention", label: `To do ${certain.length + needs.length}` },
                  { id: "booked", label: "Booked" },
                  { id: "all", label: "All" },
                ]}
                value={filter}
                onChange={setFilter}
              />
              <input
                ref={fileInput}
                type="file"
                accept=".xml,.sta,.gpc,.csv,.txt"
                aria-label="Statement file"
                className="hidden"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) importFile(f);
                  e.target.value = "";
                }}
              />
              <Button
                icon={FileUp}
                isDisabled={write.busy}
                onPress={() => fileInput.current?.click()}
              >
                Import…
              </Button>
              <Button
                variant="primary"
                isDisabled={write.busy || certain.length === 0}
                onPress={() =>
                  void write.run(async () => {
                    const after = await unwrap(commands.acceptCertainBankLines());
                    const n = certain.length;
                    return `Accepted ${n} certain ${n === 1 ? "line" : "lines"}; ${after.lines.filter((l) => l.status === "needs_you").length} still need you.`;
                  })
                }
              >
                Accept {certain.length} certain
              </Button>
            </Toolbar>
            <div
              data-testid="tie-out"
              className="mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 px-2 text-footnote text-ink-secondary"
            >
              <span>
                Opening <strong className="font-medium text-ink">{money(s.opening)}</strong>
              </span>
              <span>+{money(s.credits, { symbol: false })}</span>
              <span>{money(s.debits, { symbol: false })}</span>
              <span>
                = Closing <strong className="font-medium text-ink">{money(s.closing)}</strong>
              </span>
              <Badge tone="positive" icon={CircleCheck}>
                Ties to the bank's closing
              </Badge>
              {s.rules.length > 0 && (
                <span>
                  · {s.rules.length} {s.rules.length === 1 ? "rule" : "rules"}:{" "}
                  {s.rules.map((r) => r.name).join(", ")}
                </span>
              )}
            </div>
            <Result result={write.result} />
            <ContentGroup>
              <DataTable
                label="Bank lines"
                columns={columns}
                sections={[
                  {
                    id: "certain",
                    title: "Certain",
                    rows: certain.filter(keep),
                  },
                  { id: "needs", title: "Needs you", rows: needs.filter(keep) },
                  { id: "booked", title: "Booked", rows: booked.filter(keep) },
                ].filter((x) => x.rows.length > 0)}
                selectedId={selected?.id ?? null}
                onSelect={(id) => navigate("bank", id, true)}
              />
            </ContentGroup>
            {selected ? (
              <LineInspector line={selected} s={s} write={write} />
            ) : (
              <EmptyInspector label="Bank" text="No line selected." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}
