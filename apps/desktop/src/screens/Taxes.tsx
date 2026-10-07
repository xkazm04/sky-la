import { commands, unwrap, type VatReturnDto } from "@skyla/ipc";
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
  vat?: VatReturnDto;
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

const MONTHS: ReadonlyArray<{ id: string; label: string; from: string; to: string }> = [
  { id: "vat-2026-09", label: "September 2026", from: "2026-09-01", to: "2026-09-30" },
  { id: "vat-2026-08", label: "August 2026", from: "2026-08-01", to: "2026-08-31" },
  { id: "vat-2026-07", label: "July 2026", from: "2026-07-01", to: "2026-07-31" },
];

function VatRows({ vat }: { vat: VatReturnDto }) {
  return (
    <div className="overflow-hidden rounded-inner bg-surface shadow-group">
      <table className="w-full border-collapse text-body" aria-label="DPH return rows">
        <thead>
          <tr className="text-footnote text-ink-secondary">
            <th className="h-7 px-3 text-left font-medium">Row</th>
            <th className="h-7 px-2 text-right font-medium">Base</th>
            <th className="h-7 px-3 text-right font-medium">Tax</th>
          </tr>
        </thead>
        <tbody>
          {vat.rows.map((r) => (
            <tr key={r.row} className="border-hairline border-t align-top">
              <td className="px-3 py-1.5">
                <span className="font-medium">ř. {r.row}</span>
                <span className="block text-footnote text-ink-secondary">{r.label}</span>
              </td>
              <td className="px-2 py-1.5 text-right">{money(r.base, { symbol: false })}</td>
              <td className="px-3 py-1.5 text-right">{money(r.tax, { symbol: false })}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** Taxes: deadlines, the VAT returns and the income-tax base so far. */
export function TaxesScreen({ item }: { item: string | null }) {
  const entity = useQuery("entity", () => commands.entity());
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));
  const cash = useQuery("cash:2026-04-01:2026-09-30", () =>
    unwrap(commands.cashBasis("2026-04-01", "2026-09-30")),
  );
  const returns = useQuery("vat:2026-07..09", () =>
    Promise.all(MONTHS.map((m) => unwrap(commands.vatReturn(m.from, m.to)))),
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
        const vat: TaxRow[] =
          returns.state === "ready"
            ? returns.data.map((v, i) => {
                const month = MONTHS[i];
                const refund = v.payable.minor < 0;
                return {
                  id: month?.id ?? v.from,
                  title: `DPH return · ${month?.label ?? v.from}`,
                  detail: `${v.rows.length} rows · due ${day(v.dueOn, true)}`,
                  value: money(v.payable),
                  figure: refund ? "Excess deduction" : "To pay",
                  status:
                    i === 0
                      ? { tone: "warning", label: "Draft · not filed" }
                      : { tone: "neutral", label: "Past period" },
                  about: [
                    "Computed by the core from the ledger's VAT postings; the rule pack maps each VAT code onto the return's rows.",
                    v.unmapped.length > 0
                      ? `Not complete: ${v.unmapped.join(", ")} have no mapping in the pack.`
                      : "Every VAT code posted in the period is mapped.",
                    "Filing to EPO (XML) arrives with WP-21.",
                  ],
                  vat: v,
                };
              })
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
        const all = [...deadlines, ...vat, ...income];
        const selected = all.find((r) => r.id === item) ?? all[0];
        const e = entity.state === "ready" ? entity.data : undefined;
        const pack = vat[0]?.vat;
        return (
          <>
            <Toolbar
              title="Taxes"
              subtitle={
                e
                  ? `${e.legalForm} · ${e.vatPeriod} VAT payer · scenarios and drafts for your review, not tax advice`
                  : ""
              }
            >
              <Button onPress={() => navigate("settings", "rule-pack")}>
                {pack ? `Rule pack ${pack.pack}` : "Rule pack"}
              </Button>
            </Toolbar>
            <ContentGroup>
              <DataTable
                label="Taxes"
                columns={columns}
                sections={[
                  { id: "deadlines", title: "Deadlines", rows: deadlines },
                  { id: "vat", title: "VAT returns", rows: vat },
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
                  {selected.vat && (
                    <>
                      <InspectorSection title="Rows">
                        <VatRows vat={selected.vat} />
                      </InspectorSection>
                      <InspectorSection title="Totals">
                        <FactList
                          facts={[
                            { label: "Output tax", value: money(selected.vat.outputTax) },
                            { label: "Input tax", value: money(selected.vat.inputTax) },
                            {
                              label: selected.vat.payable.minor < 0 ? "Excess deduction" : "To pay",
                              value: <strong>{money(selected.vat.payable)}</strong>,
                            },
                            { label: "Due", value: day(selected.vat.dueOn, true) },
                          ]}
                        />
                      </InspectorSection>
                    </>
                  )}
                  <InspectorSection title="How it's worked out">
                    <Reasons reasons={selected.about} />
                  </InspectorSection>
                  <p className="mt-4 text-footnote text-ink-secondary">
                    {selected.vat
                      ? `Mapped by rule pack ${selected.vat.pack} (${selected.vat.packReview}). A draft for your review, not tax advice.`
                      : "A scenario for your review, not tax advice."}
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
