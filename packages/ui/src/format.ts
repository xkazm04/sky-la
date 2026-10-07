/**
 * Display formatting for amounts the core computed. Money arrives as integer
 * minor units; this only lays out digits (no arithmetic beyond splitting
 * whole and fractional parts), so the webview never rounds or converts.
 */

const NBSP = " ";
const SYMBOLS: Record<string, string> = { CZK: "Kč", EUR: "€", USD: "$", GBP: "£" };

/** `8470000, "CZK"` → `84 700,00 Kč` (cs-CZ grouping with non-breaking spaces). */
export function formatMinor(minor: number, currency = "CZK", options: { symbol?: boolean } = {}) {
  if (!Number.isSafeInteger(minor)) throw new RangeError(`not an integer amount: ${minor}`);
  const negative = minor < 0;
  const digits = String(Math.abs(minor)).padStart(3, "0");
  const whole = digits.slice(0, -2).replace(/\B(?=(\d{3})+(?!\d))/g, NBSP);
  const text = `${negative ? "−" : ""}${whole},${digits.slice(-2)}`;
  if (options.symbol === false) return text;
  return `${text}${NBSP}${SYMBOLS[currency] ?? currency}`;
}
