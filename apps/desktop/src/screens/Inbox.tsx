import { commands, type ProposalDto, unwrap } from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  SegmentedControl,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { CheckCheck } from "lucide-react";
import { useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { day, signed } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { EmptyInspector, EntryTable, Loaded, Reasons, TwoLine } from "./common";

type Filter = "all" | "posting" | "deadline" | "advice";

const confidence: Record<string, { tone: Tone; label: string }> = {
  certain: { tone: "positive", label: "Certain" },
  likely: { tone: "info", label: "Likely" },
  needs_you: { tone: "warning", label: "Needs you" },
};

type Row = ProposalDto & { id: string };

const columns: TableColumn<Row>[] = [
  {
    id: "item",
    title: "Item",
    isRowHeader: true,
    cell: (p) => <TwoLine title={p.title} detail={p.detail} />,
  },
  {
    id: "amount",
    title: "Amount",
    align: "end",
    width: "8.5rem",
    cell: (p) => (p.amount ? signed(p.amount) : p.dueOn ? `due ${day(p.dueOn)}` : ""),
  },
  {
    id: "confidence",
    title: "Status",
    width: "7.5rem",
    cell: (p) => {
      if (p.kind === "deadline") return <Badge tone="info">Deadline</Badge>;
      if (p.kind === "advice") return <Badge tone="accent">Advice</Badge>;
      const c = confidence[p.confidence ?? ""] ?? { tone: "neutral" as const, label: "Proposal" };
      return <Badge tone={c.tone}>{c.label}</Badge>;
    },
  },
];

/** Where an item without an entry is dealt with. */
function elsewhere(p: ProposalDto): { label: string; go: () => void } | null {
  if (p.kind === "posting" && p.bankLineId) {
    const line = p.bankLineId;
    return { label: "Book in Bank", go: () => navigate("bank", line) };
  }
  if (p.kind === "advice") {
    return p.source.startsWith("Tax")
      ? { label: "Open in Taxes", go: () => navigate("taxes", "scenarios") }
      : { label: "Open in Advisors", go: () => navigate("advisors") };
  }
  if (p.kind === "deadline") return { label: "Open in Taxes", go: () => navigate("taxes") };
  return null;
}

function ProposalInspector({
  proposal,
  busy,
  onApprove,
  onDismiss,
}: {
  proposal: ProposalDto;
  busy: boolean;
  onApprove: (ids: string[]) => void;
  onDismiss: (id: string) => void;
}) {
  const isPosting = proposal.kind === "posting";
  const canPost = isPosting && proposal.entry !== null;
  const other = elsewhere(proposal);
  return (
    <InspectorPane>
      <Inspector
        label={proposal.title}
        title={proposal.title}
        subtitle={proposal.source}
        actions={
          canPost ? (
            <>
              {proposal.bankLineId && (
                <Button
                  variant="plain"
                  isDisabled={busy}
                  onPress={() => navigate("bank", proposal.bankLineId)}
                >
                  Book differently
                </Button>
              )}
              <Button variant="primary" isDisabled={busy} onPress={() => onApprove([proposal.id])}>
                Approve and post
              </Button>
            </>
          ) : other ? (
            <>
              {proposal.kind === "advice" && (
                <Button variant="plain" isDisabled={busy} onPress={() => onDismiss(proposal.id)}>
                  Dismiss
                </Button>
              )}
              <Button variant="primary" onPress={other.go}>
                {other.label}
              </Button>
            </>
          ) : undefined
        }
      >
        {proposal.amount && (
          <InspectorSection title="Bank line">
            <FactList
              facts={[
                { label: "Amount", value: signed(proposal.amount) },
                { label: "Booked", value: day(proposal.entry?.date, true) },
                {
                  label: "Line",
                  value: (
                    <Button
                      variant="plain"
                      size="small"
                      onPress={() => navigate("bank", proposal.bankLineId)}
                    >
                      Open in Bank
                    </Button>
                  ),
                },
              ]}
            />
          </InspectorSection>
        )}
        {proposal.dueOn && (
          <InspectorSection title="Due">
            <FactList facts={[{ label: "Due", value: day(proposal.dueOn, true) }]} />
          </InspectorSection>
        )}
        {proposal.entry && (
          <InspectorSection title="Proposed journal entry">
            <EntryTable entry={proposal.entry} />
            <p className="mt-1.5 text-footnote text-ink-secondary">{proposal.entry.memo}</p>
          </InspectorSection>
        )}
        <InspectorSection title={isPosting ? "Why this posting" : "About this"}>
          <Reasons reasons={proposal.reasons} />
        </InspectorSection>
        {canPost && (
          <p className="mt-4 text-footnote text-ink-secondary">
            Nothing posts until you approve. The kernel checks the entry again before it posts.
          </p>
        )}
      </Inspector>
    </InspectorPane>
  );
}

/** Inbox (pattern B): decisions first. */
export function InboxScreen({ item }: { item: string | null }) {
  const [filter, setFilter] = useState<Filter>("all");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; lines: string[] } | null>(null);
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  /** Runs a decision, refreshes everything it may have moved, and reports. */
  const decide = async (write: () => Promise<unknown>, done: string) => {
    setBusy(true);
    setResult(null);
    try {
      await write();
      invalidateAll();
      setResult({ ok: true, lines: [done] });
      navigate("inbox", null, true);
    } catch (e) {
      setResult({ ok: false, lines: problems(e) });
    } finally {
      setBusy(false);
    }
  };
  const approve = (ids: string[]) =>
    decide(
      () => unwrap(commands.approveProposals(ids)),
      `Posted ${ids.length} ${ids.length === 1 ? "entry" : "entries"}.`,
    );
  const dismiss = (id: string) => decide(() => unwrap(commands.dismissProposal(id)), "Dismissed.");
  return (
    <Loaded query={proposals}>
      {(all) => {
        const shown = all.filter((p) => filter === "all" || p.kind === filter);
        const postings = all.filter((p) => p.kind === "posting");
        const certain = postings.filter((p) => p.confidence === "certain" && p.entry !== null);
        const selected = all.find((p) => p.id === item) ?? shown[0];
        return (
          <>
            <Toolbar
              title="Inbox"
              subtitle={`${all.length} items · ${postings.length} to approve · ${all.length - postings.length} deadlines and advice`}
            >
              <SegmentedControl
                label="Show"
                segments={[
                  { id: "all", label: "All" },
                  { id: "posting", label: "To approve" },
                  { id: "deadline", label: "Deadlines" },
                  { id: "advice", label: "Advice" },
                ]}
                value={filter}
                onChange={setFilter}
              />
              <Button
                variant="primary"
                icon={CheckCheck}
                isDisabled={busy || certain.length === 0}
                onPress={() => void approve(certain.map((p) => p.id))}
              >
                Approve {certain.length} certain
              </Button>
            </Toolbar>
            {result && (
              <div
                role={result.ok ? "status" : "alert"}
                className={`mb-2 px-2 text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
              >
                {result.lines.map((l) => (
                  <p key={l}>{l}</p>
                ))}
              </div>
            )}
            <ContentGroup>
              <DataTable
                label="Inbox"
                columns={columns}
                sections={[
                  {
                    id: "posting",
                    title: "To approve",
                    rows: shown.filter((p) => p.kind === "posting"),
                  },
                  {
                    id: "deadline",
                    title: "Deadlines",
                    rows: shown.filter((p) => p.kind === "deadline"),
                  },
                  { id: "advice", title: "Advice", rows: shown.filter((p) => p.kind === "advice") },
                ].filter((s) => s.rows.length > 0)}
                selectedId={selected?.id ?? null}
                onSelect={(id) => navigate("inbox", id, true)}
                empty="Nothing waiting for you."
              />
            </ContentGroup>
            {selected ? (
              <ProposalInspector
                key={selected.id}
                proposal={selected}
                busy={busy}
                onApprove={(ids) => void approve(ids)}
                onDismiss={(id) => void dismiss(id)}
              />
            ) : (
              <EmptyInspector label="Inbox" text="Nothing selected." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}
