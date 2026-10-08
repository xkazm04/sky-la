import { commands, type MoneyDto, type ReportingPeriodsDto, unwrap } from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  Inspector,
  InspectorSection,
  type TableColumn,
  Toolbar,
} from "@skyla/ui";
import { useQuery } from "../data";
import { day, money } from "../format";
import { navigate, type Screen } from "../router";
import { InspectorPane } from "../shell/Shell";
import { Loaded, Reasons, TwoLine } from "./common";

interface Figure {
  id: string;
  label: string;
  detail: string;
  value: string;
  target: Screen;
  targetItem?: string;
  about: string;
}

const columns: TableColumn<Figure>[] = [
  {
    id: "label",
    title: "Figure",
    isRowHeader: true,
    cell: (f) => <TwoLine title={f.label} detail={f.detail} />,
  },
  {
    id: "value",
    title: "Amount",
    align: "end",
    width: "11rem",
    cell: (f) => <span className="font-medium">{f.value}</span>,
  },
];

function line(lines: { code: string; amount: MoneyDto }[] | undefined, code: string) {
  return lines?.find((l) => l.code === code)?.amount;
}

/** Overview: the figures that matter, each one opening the screen behind it. */
export function OverviewScreen({ item }: { item: string | null }) {
  const periods = useQuery("reporting_periods", () => unwrap(commands.reportingPeriods()));
  return <Loaded query={periods}>{(p) => <Overview item={item} periods={p} />}</Loaded>;
}

function Overview({ item, periods }: { item: string | null; periods: ReportingPeriodsDto }) {
  // The last quarter that has ended, or the books so far.
  const q = periods.lastQuarter ?? periods.yearToDate;
  const asOf = periods.today;
  const result = useQuery(`pnl:${q.from}:${q.to}`, () =>
    unwrap(commands.profitAndLoss(q.from, q.to)),
  );
  const sheet = useQuery(`bs:${asOf}`, () => unwrap(commands.balanceSheet(asOf)));
  const proposals = useQuery("proposals", () => unwrap(commands.proposals()));

  return (
    <Loaded query={result}>
      {(p) => {
        const b = sheet.state === "ready" ? sheet.data : undefined;
        const figures: Figure[] = [
          {
            id: "revenue",
            label: `Revenue · ${q.label}`,
            detail: "Accrual basis, excluding VAT",
            value: money(p.totalRevenue),
            target: "statements",
            about: `Revenue booked in ${q.label} from issued invoices and other income.`,
          },
          {
            id: "expenses",
            label: `Expenses · ${q.label}`,
            detail: "Accrual basis, excluding VAT",
            value: money(p.totalExpenses),
            target: "statements",
            about: `Costs booked in ${q.label}, the non-deductible ones included.`,
          },
          {
            id: "profit",
            label: `Profit before tax · ${q.label}`,
            detail: "Revenue minus expenses, from the ledger",
            value: money(p.profit),
            target: "statements",
            about:
              "Computed by the core from the posted journal; the snapshot on the statement identifies exactly which entries.",
          },
          {
            id: "bank",
            label: `Bank · ${day(asOf, true)}`,
            detail: "Account 221, as the books stand",
            value: money(line(b?.assets, "221")),
            target: "bank",
            about:
              "The ledger balance of account 221. The bank workbench ties each imported statement to it.",
          },
          {
            id: "receivables",
            label: `Receivables · ${day(asOf, true)}`,
            detail: "What clients still owe",
            value: money(line(b?.assets, "311")),
            target: "invoices",
            about: "Account 311: issued invoices not yet paid. Invoices shows which are overdue.",
          },
          {
            id: "payables",
            label: `Payables · ${day(asOf, true)}`,
            detail: "What you still owe suppliers",
            value: money(line(b?.liabilities, "321")),
            target: "bank",
            about: "Account 321: received invoices not yet paid.",
          },
        ];
        const decisions = proposals.state === "ready" ? proposals.data : [];
        const selected = figures.find((f) => f.id === item) ?? figures[0];
        return (
          <>
            <Toolbar
              title="Overview"
              subtitle={`As of ${day(asOf, true)} · ${decisions.filter((d) => d.kind === "posting").length} decisions waiting in the inbox`}
            >
              <Button variant="primary" onPress={() => navigate("inbox")}>
                Open the inbox
              </Button>
            </Toolbar>
            <ContentGroup>
              <DataTable
                label="Key figures"
                columns={columns}
                sections={[
                  { id: "results", title: "Results", rows: figures.slice(0, 3) },
                  { id: "balances", title: "Balances", rows: figures.slice(3) },
                ]}
                selectedId={selected?.id ?? null}
                onSelect={(id) => navigate("overview", id, true)}
                onAction={(id) => {
                  const f = figures.find((x) => x.id === id);
                  if (f) navigate(f.target, f.targetItem);
                }}
              />
            </ContentGroup>
            {selected && (
              <InspectorPane>
                <Inspector
                  label={selected.label}
                  title={selected.label}
                  subtitle={selected.detail}
                  accessory={<span className="font-semibold text-title">{selected.value}</span>}
                  actions={
                    <Button
                      variant="primary"
                      onPress={() => navigate(selected.target, selected.targetItem)}
                    >
                      Open
                    </Button>
                  }
                >
                  <InspectorSection title="About this figure">
                    <p className="text-body">{selected.about}</p>
                  </InspectorSection>
                  <InspectorSection title="Coming up">
                    <ul className="space-y-2">
                      {decisions
                        .filter((d) => d.kind === "deadline")
                        .map((d) => (
                          <li
                            key={d.id}
                            className="flex items-center justify-between gap-2 text-body"
                          >
                            <span>{d.title}</span>
                            <Badge tone="info">Due {day(d.dueOn)}</Badge>
                          </li>
                        ))}
                    </ul>
                  </InspectorSection>
                  <InspectorSection title="Advice">
                    <Reasons
                      reasons={decisions.filter((d) => d.kind === "advice").map((d) => d.title)}
                    />
                  </InspectorSection>
                </Inspector>
              </InspectorPane>
            )}
          </>
        );
      }}
    </Loaded>
  );
}
