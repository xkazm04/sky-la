import { type BankLineDto, commands, unwrap } from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  Popup,
  SegmentedControl,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { CircleCheck, CircleX } from "lucide-react";
import { useState } from "react";
import { useQuery } from "../data";
import { day, money, signed } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { DemoNote, EmptyInspector, EntryTable, Loaded, TwoLine } from "./common";

type Filter = "all" | "open" | "matched";

const CONFIDENCE_LABEL: Record<string, string> = {
  certain: "Certain",
  likely: "Likely",
  needs_you: "Needs you",
  unlikely: "Unlikely",
};

const confidenceTone: Record<string, Tone> = {
  certain: "positive",
  likely: "info",
  needs_you: "warning",
  unlikely: "neutral",
};

function columnsFor(
  confidenceOf: (line: BankLineDto) => string | undefined,
): TableColumn<BankLineDto>[] {
  return [
    { id: "date", title: "Date", width: "5.5rem", cell: (l) => day(l.date) },
    {
      id: "who",
      title: "Counterparty · reference",
      isRowHeader: true,
      cell: (l) => <TwoLine title={l.counterparty} detail={l.reference} />,
    },
    { id: "amount", title: "Amount", align: "end", width: "8.5rem", cell: (l) => signed(l.amount) },
    {
      id: "status",
      title: "Status",
      width: "10rem",
      cell: (l) => {
        if (l.status === "matched") return <Badge tone="positive">Matched</Badge>;
        const c = confidenceOf(l);
        return (
          <Badge tone={confidenceTone[c ?? ""] ?? "warning"}>
            {CONFIDENCE_LABEL[c ?? ""] ?? "Open"}
          </Badge>
        );
      },
    },
  ];
}

function LineInspector({ line }: { line: BankLineDto }) {
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  const proposal =
    proposals.state === "ready" ? proposals.data.find((p) => p.id === line.proposalId) : undefined;
  return (
    <InspectorPane>
      <Inspector
        label={`Bank line ${line.counterparty}`}
        title={line.counterparty}
        subtitle={`${day(line.date, true)} · ${line.reference}`}
        accessory={<span className="font-semibold text-title">{signed(line.amount)}</span>}
        actions={
          line.status === "open" ? (
            <>
              <Button variant="plain" onPress={() => proposal && navigate("inbox", proposal.id)}>
                Open in Inbox
              </Button>
              <Popup
                label="Accept"
                placement="top end"
                trigger={<Button variant="primary">Accept</Button>}
              >
                <DemoNote>
                  Accepting posts the entry and the settlement through the kernel (WP-18).
                </DemoNote>
              </Popup>
            </>
          ) : undefined
        }
      >
        {line.foreign && (
          <InspectorSection title="Card payment">
            <FactList
              facts={[
                { label: "Original amount", value: money(line.foreign.amount) },
                { label: "Rate", value: line.foreign.rate.replace(".", ",") },
              ]}
            />
          </InspectorSection>
        )}
        {line.matchedTo && (
          <InspectorSection title="Matched">
            <p className="text-body">{line.matchedTo}</p>
          </InspectorSection>
        )}
        {line.candidates.length > 0 && (
          <InspectorSection title="Candidates · scored by rules">
            <div className="space-y-2">
              {line.candidates.map((c) => (
                <div key={c.label} className="rounded-inner bg-surface p-3 shadow-group">
                  <div className="flex items-start gap-2">
                    <div className="min-w-0 flex-1">
                      <p className="font-medium text-body">{c.label}</p>
                      <p className="text-footnote text-ink-secondary">{c.detail}</p>
                    </div>
                    <Badge tone={confidenceTone[c.confidence] ?? "neutral"}>
                      {c.score} · {c.confidence.replace("_", " ")}
                    </Badge>
                  </div>
                  <ul className="mt-2 space-y-1 text-footnote">
                    {c.contributions.map((part) => (
                      <li key={part.reason} className="flex justify-between gap-3">
                        <span className="text-ink-secondary">{part.reason}</span>
                        <span>{part.weight}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </div>
          </InspectorSection>
        )}
        {proposal?.entry && (
          <InspectorSection title="Posting">
            <EntryTable entry={proposal.entry} />
          </InspectorSection>
        )}
      </Inspector>
    </InspectorPane>
  );
}

/** Bank workbench (pattern D): lines, candidates with explained scores, the posting. */
export function BankScreen({ item }: { item: string | null }) {
  const [filter, setFilter] = useState<Filter>("open");
  const statement = useQuery("bank_statement", () => unwrap(commands.bankStatement()));
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  const confidenceOf = (line: BankLineDto) =>
    proposals.state === "ready"
      ? (proposals.data.find((p) => p.id === line.proposalId)?.confidence ?? undefined)
      : undefined;
  return (
    <Loaded query={statement}>
      {(s) => {
        const keep = (l: BankLineDto) => filter === "all" || l.status === filter;
        const selected = s.lines.find((l) => l.id === item) ?? s.lines.find(keep) ?? s.lines[0];
        const open = s.lines.filter((l) => l.status === "open").length;
        return (
          <>
            <Toolbar
              title={s.accountName}
              subtitle={`${s.file} · ${s.format} · ${day(s.from)} – ${day(s.to)} · ${s.lines.length} lines, ${open} open`}
            >
              <SegmentedControl
                label="Show lines"
                segments={[
                  { id: "open", label: `Open ${open}` },
                  { id: "matched", label: "Matched" },
                  { id: "all", label: "All" },
                ]}
                value={filter}
                onChange={setFilter}
              />
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
              {s.tiesOut ? (
                <Badge tone="positive" icon={CircleCheck}>
                  Ties to the bank's reported closing
                </Badge>
              ) : (
                <Badge tone="negative" icon={CircleX}>
                  Bank reports {money(s.reportedClosing)}
                </Badge>
              )}
            </div>
            <ContentGroup>
              <DataTable
                label="Bank lines"
                columns={columnsFor(confidenceOf)}
                sections={[
                  {
                    id: "open",
                    title: "Open",
                    rows: s.lines.filter((l) => l.status === "open" && keep(l)),
                  },
                  {
                    id: "matched",
                    title: "Matched in this import",
                    rows: s.lines.filter((l) => l.status === "matched" && keep(l)),
                  },
                ].filter((x) => x.rows.length > 0)}
                selectedId={selected?.id ?? null}
                onSelect={(id) => navigate("bank", id, true)}
              />
            </ContentGroup>
            {selected ? (
              <LineInspector line={selected} />
            ) : (
              <EmptyInspector label="Bank" text="No line selected." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}
