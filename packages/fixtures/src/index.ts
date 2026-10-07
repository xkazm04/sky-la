/**
 * Demo dataset shared by the mock IPC transport, unit tests and Playwright.
 * It is the entity used on the design canvas. WP-09 grows it into the full
 * dataset (invoices, bank lines, journal entries, proposals).
 */

export interface DemoEntity {
  readonly displayName: string;
  readonly legalForm: "OSVČ" | "s.r.o.";
  readonly vatPeriod: "monthly" | "quarterly" | "non-payer";
}

export const demoEntity: DemoEntity = {
  displayName: "Jan Novák",
  legalForm: "OSVČ",
  vatPeriod: "monthly",
};
