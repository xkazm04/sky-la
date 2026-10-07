import { commands, type ProposalDto, unwrap } from "@skyla/ipc";
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
import { CheckCheck } from "lucide-react";
import { useState } from "react";
import { useQuery } from "../data";
import { day, signed } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { DemoNote, EmptyInspector, EntryTable, Loaded, Reasons, TwoLine } from "./common";

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

function ProposalInspector({ proposal }: { proposal: ProposalDto }) {
  const isPosting = proposal.kind === "posting";
  return (
    <InspectorPane>
      <Inspector
        label={proposal.title}
        title={proposal.title}
        subtitle={proposal.source}
        actions={
          isPosting ? (
            <>
              <Button variant="plain">Edit</Button>
              <Popup
                label="Approve and post"
                placement="top end"
                trigger={<Button variant="primary">Approve and post</Button>}
              >
                <DemoNote>
                  Approving sends the entry to the kernel, which re-validates and posts it. That
                  arrives with bank matching (WP-18); nothing posts in the demo.
                </DemoNote>
              </Popup>
            </>
          ) : proposal.kind === "advice" ? (
            <Button variant="primary">Open the scenario</Button>
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
        {isPosting && (
          <p className="mt-4 text-footnote text-ink-secondary">Nothing posts until you approve.</p>
        )}
      </Inspector>
    </InspectorPane>
  );
}

/** Inbox (pattern B): decisions first. */
export function InboxScreen({ item }: { item: string | null }) {
  const [filter, setFilter] = useState<Filter>("all");
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  return (
    <Loaded query={proposals}>
      {(all) => {
        const shown = all.filter((p) => filter === "all" || p.kind === filter);
        const postings = all.filter((p) => p.kind === "posting");
        const certain = postings.filter((p) => p.confidence === "certain").length;
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
              <Popup
                label={`Approve ${certain} certain`}
                placement="bottom end"
                trigger={
                  <Button variant="primary" icon={CheckCheck}>
                    Approve {certain} certain
                  </Button>
                }
              >
                <DemoNote>
                  Bulk approval posts every certain match through the kernel. It arrives with WP-18.
                </DemoNote>
              </Popup>
            </Toolbar>
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
              <ProposalInspector proposal={selected} />
            ) : (
              <EmptyInspector label="Inbox" text="Nothing selected." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}
