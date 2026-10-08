import { commands, type ObligationDto, unwrap } from "@skyla/ipc";
import { Badge, InspectorSection } from "@skyla/ui";
import { useQuery } from "../data";
import { day } from "../format";

const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** `2025-12` → `Dec 2025`; `2026-Q3` → `Q3 2026`; a year stays as it is. */
function periodLabel(period: string): string {
  const [year, part] = period.split("-");
  if (!part) return period;
  if (part.startsWith("Q")) return `${part} ${year}`;
  return `${MONTH_NAMES[Number(part) - 1]?.slice(0, 3) ?? part} ${year}`;
}

const ACTION = { file: "File", pay: "Pay", file_and_pay: "File and pay" } as const;

export function useObligations(year: number) {
  return useQuery(`obligations:${year}`, () => unwrap(commands.obligations(year)));
}

function Row({ o }: { o: ObligationDto }) {
  const past = o.status === "past";
  return (
    <li
      className={`flex items-start gap-3 border-hairline border-t px-3 py-1.5 first:border-t-0 ${past ? "text-ink-secondary" : ""}`}
    >
      <span className="w-14 shrink-0 font-medium">{day(o.due)}</span>
      <span className="min-w-0 flex-1">
        <span className="block">{o.name}</span>
        <span className="block text-footnote text-ink-secondary">
          {ACTION[o.action as keyof typeof ACTION] ?? o.action} · for {periodLabel(o.period)}
          {o.shifted ? ` · moved from ${day(o.nominal)}` : ""}
        </span>
      </span>
      {o.status === "next" && <Badge tone="info">Next</Badge>}
    </li>
  );
}

/** The year's deadlines from the rule pack, month by month. */
export function ObligationsCalendar({ obligations }: { obligations: ObligationDto[] }) {
  const months = MONTH_NAMES.map((name, i) => ({
    name,
    items: obligations.filter((o) => Number(o.due.slice(5, 7)) === i + 1),
  })).filter((m) => m.items.length > 0);
  return (
    <>
      {months.map((m) => (
        <InspectorSection key={m.name} title={m.name}>
          <ul
            aria-label={`Deadlines in ${m.name}`}
            className="overflow-hidden rounded-inner bg-surface text-body shadow-group"
          >
            {m.items.map((o) => (
              <Row key={`${o.obligation}:${o.period}`} o={o} />
            ))}
          </ul>
        </InspectorSection>
      ))}
    </>
  );
}
