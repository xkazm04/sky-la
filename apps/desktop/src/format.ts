import type { MoneyDto } from "@skyla/ipc";
import { formatMinor } from "@skyla/ui";

/** An amount from the core, laid out in Czech style. No arithmetic. */
export function money(value: MoneyDto | null | undefined, options?: { symbol?: boolean }): string {
  if (!value) return "—";
  return formatMinor(value.minor, value.currency, options);
}

/** A signed amount with an explicit plus for money coming in. */
export function signed(value: MoneyDto): string {
  return value.minor > 0 ? `+${money(value)}` : money(value);
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** `2026-09-29` → `29 Sep`; with `year`, `29 Sep 2026`. */
export function day(date: string | null | undefined, year = false): string {
  if (!date) return "—";
  const [y, m, d] = date.split("-");
  const month = MONTHS[Number(m) - 1] ?? m;
  return `${Number(d)} ${month}${year ? ` ${y}` : ""}`;
}

/** `2026-10-06T18:04:12Z` → `6 Oct 2026, 18:04 UTC`. */
export function moment(at: string): string {
  const [date, time = ""] = at.split("T");
  return `${day(date, true)}, ${time.slice(0, 5)} UTC`;
}

/** A shortened hash for display, `7f3a…c91e`. */
export function shortHash(hash: string | null | undefined): string {
  return hash ? `${hash.slice(0, 4)}…${hash.slice(-4)}` : "—";
}

/** Keeps `803 622,31 Kč` and `60 %` on one line (presentation only). */
export function keepFiguresTogether(text: string): string {
  return text.replace(/(\d) (?=\d{3}\b)/g, "$1\u00a0").replace(/(\d) (Kč|%)/g, "$1\u00a0$2");
}
