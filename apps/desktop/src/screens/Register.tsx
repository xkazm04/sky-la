import { commands, type EgressRunDto, unwrap } from "@skyla/ipc";
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
import { moment, shortHash } from "../format";
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

/** Exactly what a run sent, as the register replays it. */
function WhatWasShared({ id }: { id: string }) {
  const payload = useQuery(`egress_payload:${id}`, () => unwrap(commands.egressPayload(id)));
  return (
    <InspectorSection title="What was shared">
      <Loaded query={payload}>
        {(p) => (
          <>
            <p className="mb-1.5 text-footnote text-ink-secondary">
              {p.bytes} bytes, replayed from the register exactly as sent · link {shortHash(p.hash)}
            </p>
            <section
              aria-label="Payload sent"
              // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must take focus
              tabIndex={0}
              data-testid="payload"
              className="max-h-80 overflow-auto rounded-inner bg-surface p-3 shadow-group outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--sk-focus)]"
            >
              <pre className="whitespace-pre-wrap break-words font-mono text-footnote">
                {p.text}
              </pre>
            </section>
          </>
        )}
      </Loaded>
    </InspectorSection>
  );
}

/** The egress register: every advisor run, what left the machine and what was withheld. */
export function RegisterScreen({ item }: { item: string | null }) {
  const runs = useQuery("egress_register", () => unwrap(commands.egressRegister()));
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
                  <InspectorSection title="Allowed to send">
                    <p className="text-body">{selected.sent}</p>
                  </InspectorSection>
                  <InspectorSection title="Withheld by the egress gate">
                    {selected.redacted.length > 0 ? (
                      <Reasons reasons={selected.redacted} />
                    ) : (
                      <p className="text-body text-ink-secondary">Nothing needed withholding.</p>
                    )}
                  </InspectorSection>
                  <WhatWasShared id={selected.id} />
                  <InspectorSection title="Run">
                    <FactList
                      facts={[
                        { label: "Provider", value: selected.provider },
                        { label: "Model", value: selected.model },
                        { label: "Tool calls", value: String(selected.toolCalls) },
                        { label: "Bytes sent", value: String(selected.bytesSent) },
                        { label: "Outcome", value: selected.outcome },
                        {
                          label: "Register",
                          value: selected.intact
                            ? "Unaltered (hash chain verified)"
                            : "Altered after recording",
                        },
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
