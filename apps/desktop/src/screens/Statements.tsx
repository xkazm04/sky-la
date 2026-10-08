import {
  commands,
  type JournalEntryDto,
  type ReportingPeriodsDto,
  type SnapshotDto,
  type StatementLineDto,
  unwrap,
} from "@skyla/ipc";
import {
  ContentGroup,
  FactList,
  Inspector,
  InspectorSection,
  SegmentedControl,
  Toolbar,
} from "@skyla/ui";
import { useState } from "react";
import { useQuery } from "../data";
import { day, money, shortHash } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { EmptyInspector, Loaded } from "./common";
import { ExplainThis } from "./ExplainThis";

type Report = "pnl" | "balance" | "cash";

/** A statement row, clickable to drill into the entries behind it. */
function Row({
  line,
  selected,
  onSelect,
  comparing,
  compare,
}: {
  line: StatementLineDto;
  selected: boolean;
  onSelect: () => void;
  comparing: boolean;
  compare?: StatementLineDto | undefined;
}) {
  return (
    <tr className={`border-hairline border-t ${selected ? "bg-accent-tint" : "hover:bg-fill"}`}>
      <td className="w-16 py-2 pl-5 text-ink-secondary">{line.code}</td>
      <td className="py-2">
        <button
          type="button"
          onClick={onSelect}
          className="cursor-default rounded-[4px] text-left outline-none focus-visible:outline-3 focus-visible:outline-focus"
        >
          {line.nameEn}
        </button>
      </td>
      <td className="py-2 pr-4 text-right">{money(line.amount, { symbol: false })}</td>
      {comparing && (
        <td className="py-2 pr-5 text-right text-ink-secondary">
          {money(compare?.amount, { symbol: false })}
        </td>
      )}
    </tr>
  );
}

function Group({
  title,
  lines,
  total,
  totalLabel,
  selected,
  onSelect,
  compareLines,
  compareTotal,
}: {
  title: string;
  lines: StatementLineDto[];
  total: StatementLineDto["amount"];
  totalLabel: string;
  selected: string | null;
  onSelect: (code: string) => void;
  compareLines?: StatementLineDto[];
  compareTotal?: StatementLineDto["amount"];
}) {
  const comparing = compareLines !== undefined;
  return (
    <tbody>
      <tr>
        <th
          colSpan={comparing ? 4 : 3}
          className="px-5 pt-4 pb-1 text-left font-semibold text-caption text-ink-secondary uppercase tracking-[0.05em]"
        >
          {title}
        </th>
      </tr>
      {lines.map((line) => (
        <Row
          key={line.code}
          line={line}
          selected={selected === line.code}
          onSelect={() => onSelect(line.code)}
          comparing={comparing}
          compare={compareLines?.find((c) => c.code === line.code)}
        />
      ))}
      <tr className="border-hairline-strong border-t font-semibold">
        <td />
        <td className="py-2">{totalLabel}</td>
        <td className="py-2 pr-4 text-right">{money(total, { symbol: false })}</td>
        {comparing && (
          <td className="py-2 pr-5 text-right">{money(compareTotal, { symbol: false })}</td>
        )}
      </tr>
    </tbody>
  );
}

function Provenance({ snapshot }: { snapshot: SnapshotDto }) {
  return (
    <p className="px-5 pt-4 pb-5 text-footnote text-ink-secondary">
      Generated from {snapshot.entries} journal entries, the last posted #
      {snapshot.lastPostedSeq ?? "—"}. Input snapshot {shortHash(snapshot.hash)}. Recomputed by the
      core on every open; nothing on this page is typed in by hand.
    </p>
  );
}

function Drilldown({ code, from, to }: { code: string; from: string; to: string }) {
  const journal = useQuery(`journal:${from}:${to}`, () => unwrap(commands.journal(from, to)));
  const [cited, setCited] = useState<number[]>([]);
  return (
    <InspectorPane>
      <Inspector
        label={`Account ${code}`}
        title={`Account ${code}`}
        subtitle={`${day(from, true)} – ${day(to, true)}`}
      >
        <ExplainThis
          key={`${code}:${from}:${to}`}
          target={{ kind: "account", account: code, from, to, entry: null }}
          onCites={setCited}
        />
        <Loaded query={journal}>
          {(entries: JournalEntryDto[]) => {
            const hits = entries.flatMap((e) =>
              e.lines
                .filter((l) => l.account === code || l.account.startsWith(`${code}.`))
                .map((l) => ({ e, l })),
            );
            return (
              <InspectorSection title={`Entries behind this line · ${hits.length}`}>
                <ul className="overflow-hidden rounded-inner bg-surface shadow-group">
                  {hits.map(({ e, l }) => (
                    <li
                      key={`${e.id}-${l.lineNo}`}
                      data-cited={cited.includes(e.id) || undefined}
                      className="flex items-center gap-3 border-hairline border-t px-3 py-2 first:border-t-0 data-cited:bg-accent-tint"
                    >
                      <span className="w-9 shrink-0 text-footnote text-ink-secondary">#{e.id}</span>
                      <span className="w-12 shrink-0 text-footnote text-ink-secondary">
                        {day(e.date)}
                      </span>
                      <span className="min-w-0 flex-1 truncate">{e.memo}</span>
                      <span className="text-right">{money(l.functional, { symbol: false })}</span>
                    </li>
                  ))}
                </ul>
                <p className="mt-2 text-footnote text-ink-secondary">
                  Debits positive, credits negative.
                </p>
              </InspectorSection>
            );
          }}
        </Loaded>
      </Inspector>
    </InspectorPane>
  );
}

/** Statements (pattern C): the books as a document, every figure opens to its entries. */
export function StatementsScreen({ item }: { item: string | null }) {
  const periods = useQuery("reporting_periods", () => unwrap(commands.reportingPeriods()));
  return <Loaded query={periods}>{(p) => <Statements item={item} periods={p} />}</Loaded>;
}

function Statements({ item, periods }: { item: string | null; periods: ReportingPeriodsDto }) {
  // The ended quarters, then the year to date; segments drop the year.
  const ranges = [...periods.quarters, periods.yearToDate].map((p) => ({
    ...p,
    short: p.label.replace(/ \d{4}$/, ""),
  }));
  const [report, setReport] = useState<Report>("pnl");
  const [range, setRange] = useState<string>(periods.lastQuarter?.id ?? periods.yearToDate.id);
  const r = ranges.find((x) => x.id === range) ?? periods.yearToDate;
  const select = (code: string) => navigate("statements", code, true);
  const before = periods.priorQuarter;

  const pnl = useQuery(`pnl:${r.from}:${r.to}`, () => unwrap(commands.profitAndLoss(r.from, r.to)));
  const prior = useQuery(`pnl:${before?.from}:${before?.to}`, () =>
    before ? unwrap(commands.profitAndLoss(before.from, before.to)) : Promise.resolve(null),
  );
  const sheet = useQuery(`bs:${r.to}`, () => unwrap(commands.balanceSheet(r.to)));
  const cash = useQuery(`cash:${r.from}:${r.to}`, () => unwrap(commands.cashBasis(r.from, r.to)));

  const titles: Record<Report, string> = {
    pnl: "Profit and loss",
    balance: "Balance sheet",
    cash: "Cash basis · daňová evidence",
  };
  return (
    <>
      <Toolbar
        title={titles[report]}
        subtitle={
          report === "balance"
            ? `As of ${day(r.to, true)} · Kč`
            : `${day(r.from)} – ${day(r.to, true)} · Kč, excl. VAT`
        }
      >
        <SegmentedControl
          label="Statement"
          segments={[
            { id: "pnl", label: "P&L" },
            { id: "balance", label: "Balance sheet" },
            { id: "cash", label: "Cash basis" },
          ]}
          value={report}
          onChange={setReport}
        />
        <SegmentedControl
          label="Period"
          segments={ranges.map((x) => ({ id: x.id, label: x.short }))}
          value={range}
          onChange={setRange}
        />
      </Toolbar>
      <ContentGroup>
        {report === "pnl" && (
          <Loaded query={pnl}>
            {(p) => {
              const comparing =
                range === periods.lastQuarter?.id && prior.state === "ready" && prior.data
                  ? prior.data
                  : undefined;
              return (
                <>
                  <table className="w-full border-collapse text-body" aria-label="Profit and loss">
                    <thead>
                      <tr className="text-footnote text-ink-secondary">
                        <th className="h-8 pl-5 text-left font-medium">Account</th>
                        <th className="h-8 text-left font-medium" />
                        <th className="h-8 pr-4 text-right font-medium">{r.label}</th>
                        {comparing && (
                          <th className="h-8 pr-5 text-right font-medium">{before?.label}</th>
                        )}
                      </tr>
                    </thead>
                    <Group
                      title="Revenue"
                      lines={p.revenue}
                      total={p.totalRevenue}
                      totalLabel="Total revenue"
                      selected={item}
                      onSelect={select}
                      compareLines={comparing?.revenue}
                      compareTotal={comparing?.totalRevenue}
                    />
                    <Group
                      title="Expenses"
                      lines={p.expenses}
                      total={p.totalExpenses}
                      totalLabel="Total expenses"
                      selected={item}
                      onSelect={select}
                      compareLines={comparing?.expenses}
                      compareTotal={comparing?.totalExpenses}
                    />
                    <tbody>
                      <tr className="border-hairline-strong border-t font-semibold text-title">
                        <td />
                        <td className="py-3">Profit before tax</td>
                        <td className="py-3 pr-4 text-right">
                          {money(p.profit, { symbol: false })}
                        </td>
                        {comparing && (
                          <td className="py-3 pr-5 text-right">
                            {money(comparing.profit, { symbol: false })}
                          </td>
                        )}
                      </tr>
                    </tbody>
                  </table>
                  <Provenance snapshot={p.snapshot} />
                </>
              );
            }}
          </Loaded>
        )}
        {report === "balance" && (
          <Loaded query={sheet}>
            {(b) => (
              <>
                <table className="w-full border-collapse text-body" aria-label="Balance sheet">
                  <Group
                    title="Assets"
                    lines={b.assets}
                    total={b.totalAssets}
                    totalLabel="Total assets"
                    selected={item}
                    onSelect={select}
                  />
                  <Group
                    title="Liabilities"
                    lines={b.liabilities}
                    total={b.totalLiabilities}
                    totalLabel="Total liabilities"
                    selected={item}
                    onSelect={select}
                  />
                  <Group
                    title="Equity"
                    lines={[
                      ...b.equity,
                      {
                        code: "—",
                        nameCs: "Výsledek hospodaření",
                        nameEn: "Profit not yet closed",
                        amount: b.unclosedProfit,
                      },
                    ]}
                    total={b.totalEquity}
                    totalLabel="Total equity"
                    selected={item}
                    onSelect={select}
                  />
                </table>
                <p className="px-5 pt-3 text-footnote text-ink-secondary">
                  {b.balances
                    ? "Assets equal liabilities plus equity."
                    : "The balance sheet doesn't balance."}
                </p>
                <Provenance snapshot={b.snapshot} />
              </>
            )}
          </Loaded>
        )}
        {report === "cash" && (
          <Loaded query={cash}>
            {(c) => (
              <>
                <div className="px-5 pt-4">
                  <FactList
                    facts={c.totals.map((t) => ({
                      label: `${t.direction === "income" ? "Income" : "Expenses"} · ${t.taxTreatment.replaceAll("_", " ")}`,
                      value: money(t.amount),
                    }))}
                  />
                </div>
                <table
                  className="mt-3 w-full border-collapse text-body"
                  aria-label="Cash-basis recognitions"
                >
                  <thead>
                    <tr className="text-footnote text-ink-secondary">
                      <th className="h-8 pl-5 text-left font-medium">Paid</th>
                      <th className="h-8 text-left font-medium">Account</th>
                      <th className="h-8 text-left font-medium">Treatment</th>
                      <th className="h-8 pr-5 text-right font-medium">Amount</th>
                    </tr>
                  </thead>
                  <tbody>
                    {c.lines.map((l) => (
                      <tr
                        key={`${l.cashEntryId}-${l.settledEntryId ?? "direct"}-${l.account}-${l.amount.minor}`}
                        className="border-hairline border-t"
                      >
                        <td className="py-2 pl-5">{day(l.date)}</td>
                        <td className="py-2">
                          {l.account}
                          {l.settledEntryId ? " · settled invoice" : ""}
                        </td>
                        <td className="py-2 text-ink-secondary">
                          {l.taxTreatment.replaceAll("_", " ")}
                        </td>
                        <td className="py-2 pr-5 text-right">
                          {money(l.amount, { symbol: false })}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <Provenance snapshot={c.snapshot} />
              </>
            )}
          </Loaded>
        )}
      </ContentGroup>
      {item && /^\d{3}/.test(item) ? (
        <Drilldown code={item} from={report === "balance" ? periods.booksFrom : r.from} to={r.to} />
      ) : (
        <EmptyInspector
          label="Statement"
          text="Select a line to see the journal entries behind it."
        />
      )}
    </>
  );
}
