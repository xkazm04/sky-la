import { commands, unwrap } from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { invalidateAll, useQuery } from "../data";
import { moment } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { AdvisorConnection, connectionBadge } from "./AdvisorConnection";
import { Loaded, Reasons, TwoLine } from "./common";

interface Advisor {
  id: string;
  name: string;
  role: string;
  reads: string[];
  proposes: string;
}

const ADVISORS: Advisor[] = [
  {
    id: "tax",
    name: "Tax advisor",
    role: "Scenarios for the annual return and VAT, drafted for your review",
    reads: [
      "Period summaries and totals",
      "The VAT ledger by form row",
      "Rule-pack values with their citations",
    ],
    proposes: "Scenarios and drafts. Every figure in its text must match the engine.",
  },
  {
    id: "financial",
    name: "Financial advisor",
    role: "Cost and profit breakdowns, inefficiencies, trends",
    reads: ["Account totals by month and vendor", "Invoice and payment timing"],
    proposes: "Findings for your review in the inbox.",
  },
  {
    id: "explain",
    name: "Explain this",
    role: "Inline explanations of any figure, account or line",
    reads: ["The figure you point at and the entries behind it"],
    proposes: "An explanation only; it never posts.",
  },
];

const columns = (status: { tone: Tone; label: string }): TableColumn<Advisor>[] => [
  {
    id: "name",
    title: "Advisor",
    isRowHeader: true,
    cell: (a) => <TwoLine title={a.name} detail={a.role} />,
  },
  {
    id: "status",
    title: "Status",
    width: "12rem",
    cell: () => <Badge tone={status.tone}>{status.label}</Badge>,
  },
];

/** Advisors: what each one can read and propose, and what it sent. */
export function AdvisorsScreen({ item }: { item: string | null }) {
  const runs = useQuery("egress_register", () => commands.egressRegister());
  const status = useQuery("advisor_status", () => commands.advisorStatus());
  const connection = status.state === "ready" ? status.data : undefined;
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  const selected = ADVISORS.find((a) => a.id === item) ?? ADVISORS[0];
  return (
    <>
      <Toolbar
        title="Advisors"
        subtitle="They propose, explain and calculate through the engine; they never post"
      >
        <Button onPress={() => navigate("register")}>Egress register</Button>
      </Toolbar>
      <ContentGroup>
        <DataTable
          label="Advisors"
          columns={columns(connectionBadge(connection))}
          sections={[
            { id: "advisors", title: "Uses your Claude Code installation", rows: ADVISORS },
          ]}
          selectedId={selected?.id ?? null}
          onSelect={(id) => navigate("advisors", id, true)}
        />
        {connection && <AdvisorConnection status={connection} onCheckAgain={invalidateAll} />}
        <div className="px-5 pb-4 text-footnote text-ink-secondary">
          sky-la runs the <code className="font-sans">claude</code> command you installed, signed in
          from your terminal with <span className="font-medium text-ink">claude auth login</span>.
          It never sees your Claude credentials, and every prompt and tool result passes the egress
          gate first.
        </div>
      </ContentGroup>
      {selected && (
        <InspectorPane>
          <Inspector label={selected.name} title={selected.name} subtitle={selected.role}>
            <InspectorSection title="Can read">
              <Reasons reasons={selected.reads} />
            </InspectorSection>
            <InspectorSection title="Can propose">
              <p className="text-body">{selected.proposes}</p>
            </InspectorSection>
            <InspectorSection title="Recent findings">
              <Loaded query={proposals}>
                {(all) => (
                  <Reasons
                    reasons={all
                      .filter(
                        (p) =>
                          p.kind === "advice" &&
                          p.source
                            .toLowerCase()
                            .startsWith(selected.name.split(" ")[0]?.toLowerCase() ?? ""),
                      )
                      .map((p) => p.title)
                      .concat(
                        selected.id === "explain"
                          ? ["Ask from any figure: the sparkle button next to it."]
                          : [],
                      )}
                  />
                )}
              </Loaded>
            </InspectorSection>
            <InspectorSection title="Runs">
              <Loaded query={runs}>
                {(all) => (
                  <FactList
                    facts={all
                      .filter((r) => r.advisor === selected.name)
                      .map((r) => ({ label: moment(r.at), value: `${r.bytesSent} bytes` }))
                      .concat([{ label: "All runs", value: "Egress register" }])}
                  />
                )}
              </Loaded>
            </InspectorSection>
          </Inspector>
        </InspectorPane>
      )}
    </>
  );
}
