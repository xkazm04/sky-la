import { commands, unwrap } from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  Inspector,
  InspectorSection,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { useQuery } from "../data";
import { day, money } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { Loaded, Reasons, TwoLine } from "./common";

interface TaxRow {
  id: string;
  title: string;
  detail: string;
  value: string;
  figure: string;
  status: { tone: Tone; label: string };
  about: string[];
}

const columns: TableColumn<TaxRow>[] = [
  {
    id: "title",
    title: "Item",
    isRowHeader: true,
    cell: (r) => <TwoLine title={r.title} detail={r.detail} />,
  },
  { id: "value", title: "Amount", align: "end", width: "10rem", cell: (r) => r.value },
  {
    id: "status",
    title: "Status",
    width: "11rem",
    cell: (r) => <Badge tone={r.status.tone}>{r.status.label}</Badge>,
  },
];

/** Taxes: deadlines, the VAT returns and the income-tax base so far. */
export function TaxesScreen({ item }: { item: string | null }) {
  const entity = useQuery("entity", () => commands.entity());
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  const cash = useQuery("cash:2026-04-01:2026-09-30", () =>
    unwrap(commands.cashBasis("2026-04-01", "2026-09-30")),
  );
  return (
    <Loaded query={cash}>
      {(c) => {
        const deadlines: TaxRow[] =
          proposals.state === "ready"
            ? proposals.data
                .filter((p) => p.kind === "deadline")
                .map((p) => ({
                  id: p.id,
                  title: p.title,
                  detail: p.detail,
                  value: `due ${day(p.dueOn, true)}`,
                  figure: "Due",
                  status: { tone: "info", label: "Upcoming" },
                  about: p.reasons,
                }))
            : [];
        const total = (direction: string, treatment: string) =>
          c.totals.find((t) => t.direction === direction && t.taxTreatment === treatment)?.amount;
        const income: TaxRow[] = [
          {
            id: "taxable-income",
            title: "Taxable income",
            detail: "Cash basis · received April – September",
            value: money(c.taxableIncome),
            figure: "Amount",
            status: { tone: "neutral", label: "From the ledger" },
            about: [
              "Income recognised when it was paid, excluding VAT.",
              "Partly paid invoices count in proportion to what was paid.",
            ],
          },
          {
            id: "deductible",
            title: "Deductible expenses",
            detail: "Cash basis · paid April – September",
            value: money(c.deductibleExpenses),
            figure: "Amount",
            status: { tone: "neutral", label: "From the ledger" },
            about: ["Expenses recognised when they were paid, excluding claimable VAT."],
          },
          {
            id: "non-deductible",
            title: "Non-deductible expenses",
            detail: "Kept out of the tax base automatically",
            value: money(total("expense", "non_deductible")),
            figure: "Amount",
            status: { tone: "neutral", label: "From the ledger" },
            about: ["Client entertainment is not tax-deductible; its input VAT isn't claimed."],
          },
        ];
        const vat: TaxRow[] = [
          {
            id: "vat-september",
            title: "DPH return · September 2026",
            detail: "Form rows from the CZ rule pack",
            value: "Not drafted yet",
            figure: "Status",
            status: { tone: "neutral", label: "Rule pack WP-20" },
            about: [
              "The VAT ledger is computed by the core today; the form-row mapping and the return arrive with the CZ rule pack (WP-20, WP-21).",
            ],
          },
        ];
        const all = [...deadlines, ...vat, ...income];
        const selected = all.find((r) => r.id === item) ?? all[0];
        const e = entity.state === "ready" ? entity.data : undefined;
        return (
          <>
            <Toolbar
              title="Taxes"
              subtitle={
                e
                  ? `${e.legalForm} · ${e.vatPeriod} VAT payer · figures are scenarios for your review, not tax advice`
                  : ""
              }
            >
              <Button onPress={() => navigate("statements")}>Cash-basis detail</Button>
            </Toolbar>
            <ContentGroup>
              <DataTable
                label="Taxes"
                columns={columns}
                sections={[
                  { id: "deadlines", title: "Deadlines", rows: deadlines },
                  { id: "vat", title: "VAT", rows: vat },
                  { id: "income", title: "Income tax · daňová evidence 2026 so far", rows: income },
                ].filter((s) => s.rows.length > 0)}
                selectedId={selected?.id ?? null}
                onSelect={(id) => navigate("taxes", id, true)}
              />
            </ContentGroup>
            {selected && (
              <InspectorPane>
                <Inspector
                  label={selected.title}
                  title={selected.title}
                  subtitle={selected.detail}
                  accessory={<Badge tone={selected.status.tone}>{selected.status.label}</Badge>}
                >
                  <InspectorSection title={selected.figure}>
                    <p className="font-semibold text-title">{selected.value}</p>
                  </InspectorSection>
                  <InspectorSection title="How it's worked out">
                    <Reasons reasons={selected.about} />
                  </InspectorSection>
                  <p className="mt-4 text-footnote text-ink-secondary">
                    A scenario for your review, not tax advice.
                  </p>
                </Inspector>
              </InspectorPane>
            )}
          </>
        );
      }}
    </Loaded>
  );
}
