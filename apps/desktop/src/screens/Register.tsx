import { commands, type EgressRunDto } from "@skyla/ipc";
import {
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  type TableColumn,
  Toolbar,
} from "@skyla/ui";
import { useQuery } from "../data";
import { moment } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { EmptyInspector, Loaded, Reasons, TwoLine } from "./common";

const columns: TableColumn<EgressRunDto>[] = [
  { id: "at", title: "When", width: "11rem", cell: (r) => moment(r.at) },
  {
    id: "purpose",
    title: "Run",
    isRowHeader: true,
    cell: (r) => <TwoLine title={r.purpose} detail={r.advisor} />,
  },
  { id: "bytes", title: "Sent", align: "end", width: "7rem", cell: (r) => `${r.bytesSent} B` },
  {
    id: "outcome",
    title: "Outcome",
    width: "15rem",
    cell: (r) => <span className="text-ink-secondary">{r.outcome}</span>,
  },
];

/** The egress register: every advisor run, what left the machine and what was withheld. */
export function RegisterScreen({ item }: { item: string | null }) {
  const runs = useQuery("egress_register", () => commands.egressRegister());
  return (
    <Loaded query={runs}>
      {(all) => {
        const selected = all.find((r) => r.id === item) ?? all[0];
        return (
          <>
            <Toolbar
              title="Egress register"
              subtitle={`${all.length} runs · prompts and tool results pass the egress gate · IBANs and personal IDs are never sent`}
            />
            <ContentGroup>
              <DataTable
                label="Advisor runs"
                columns={columns}
                sections={[{ id: "runs", title: "Runs", rows: all }]}
                selectedId={selected?.id ?? null}
                onSelect={(id) => navigate("register", id, true)}
                empty="Nothing has been sent."
              />
            </ContentGroup>
            {selected ? (
              <InspectorPane>
                <Inspector
                  label={selected.purpose}
                  title={selected.purpose}
                  subtitle={`${selected.advisor} · ${moment(selected.at)}`}
                >
                  <InspectorSection title="Sent">
                    <p className="text-body">{selected.sent}</p>
                  </InspectorSection>
                  <InspectorSection title="Withheld by the egress gate">
                    {selected.redacted.length > 0 ? (
                      <Reasons reasons={selected.redacted} />
                    ) : (
                      <p className="text-body text-ink-secondary">Nothing needed withholding.</p>
                    )}
                  </InspectorSection>
                  <InspectorSection title="Run">
                    <FactList
                      facts={[
                        { label: "Provider", value: selected.provider },
                        { label: "Model", value: selected.model },
                        { label: "Tool calls", value: String(selected.toolCalls) },
                        { label: "Bytes sent", value: String(selected.bytesSent) },
                        { label: "Outcome", value: selected.outcome },
                      ]}
                    />
                  </InspectorSection>
                </Inspector>
              </InspectorPane>
            ) : (
              <EmptyInspector label="Egress register" text="No runs yet." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}
