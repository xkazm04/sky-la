import type { ProposedEntryDto } from "@skyla/ipc";
import { Inspector } from "@skyla/ui";
import type { ReactNode } from "react";
import type { Query } from "../data";
import { money } from "../format";
import { InspectorPane } from "../shell/Shell";

/** Renders a query's data, or its loading or error state. */
export function Loaded<T>({
  query,
  children,
}: {
  query: Query<T>;
  children: (data: T) => ReactNode;
}) {
  if (query.state === "loading") return <p className="px-5 py-8 text-ink-secondary">Loading…</p>;
  if (query.state === "error")
    return (
      <p role="alert" className="px-5 py-8 text-negative-ink">
        The core couldn't answer: {query.message}
      </p>
    );
  return <>{children(query.data)}</>;
}

/** Two lines in a 40 px row: the title and a secondary detail. */
export function TwoLine({ title, detail }: { title: ReactNode; detail?: ReactNode }) {
  return (
    <span className="flex min-w-0 flex-col leading-tight">
      <span className="truncate">{title}</span>
      {detail && <span className="truncate text-footnote text-ink-secondary">{detail}</span>}
    </span>
  );
}

/** The inspector when nothing is selected. */
export function EmptyInspector({ label, text }: { label: string; text: string }) {
  return (
    <InspectorPane>
      <Inspector label={label} title={label}>
        <p className="text-ink-secondary">{text}</p>
      </Inspector>
    </InspectorPane>
  );
}

/** A proposed journal entry: account, debit, credit, with totals. */
export function EntryTable({ entry }: { entry: ProposedEntryDto }) {
  return (
    <div className="overflow-hidden rounded-inner bg-surface shadow-group">
      <table className="w-full border-collapse text-body">
        <thead>
          <tr className="text-footnote text-ink-secondary">
            <th className="h-7 px-3 text-left font-medium">Account</th>
            <th className="h-7 px-2 text-right font-medium">Debit</th>
            <th className="h-7 px-3 text-right font-medium">Credit</th>
          </tr>
        </thead>
        <tbody>
          {entry.lines.map((line) => (
            <tr
              key={`${line.account}-${line.debit ? "debit" : "credit"}`}
              className="border-hairline border-t"
            >
              <td className="px-3 py-1.5">
                <span className="font-medium">{line.account}</span>{" "}
                <span className="text-ink-secondary">{line.accountName}</span>
              </td>
              <td className="px-2 text-right">
                {line.debit ? money(line.debit, { symbol: false }) : "—"}
              </td>
              <td className="px-3 text-right">
                {line.credit ? money(line.credit, { symbol: false }) : "—"}
              </td>
            </tr>
          ))}
          <tr className="border-hairline-strong border-t font-medium">
            <td className="px-3 py-1.5">{entry.balanced ? "Balanced" : "Not balanced"}</td>
            <td className="px-2 text-right">{money(entry.totalDebit, { symbol: false })}</td>
            <td className="px-3 text-right">{money(entry.totalCredit, { symbol: false })}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

/** Why: a short list of reasons. */
export function Reasons({ reasons }: { reasons: readonly string[] }) {
  return (
    <ul className="space-y-1.5 text-body">
      {reasons.map((reason) => (
        <li key={reason} className="flex gap-2">
          <span aria-hidden className="mt-[7px] size-1 shrink-0 rounded-full bg-ink-secondary" />
          <span>{reason}</span>
        </li>
      ))}
    </ul>
  );
}

/** Copy for actions that need a later work packet. */
export function DemoNote({ children }: { children: ReactNode }) {
  return <p className="max-w-64 text-body">{children}</p>;
}
