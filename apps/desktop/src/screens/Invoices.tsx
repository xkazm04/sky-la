import { commands, type InvoiceDto, unwrap } from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  Menu,
  MenuItem,
  MenuSeparator,
  Popup,
  SegmentedControl,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { FileDown, FileUp, Plus, Repeat, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { downloadBase64, downloadText, fileToBase64 } from "../download";
import { day, money } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { EmptyInspector, Loaded } from "./common";
import { InvoiceEditor } from "./InvoiceEditor";
import { InvoiceImport, type PendingImport } from "./InvoiceImport";

type Filter = "all" | "open" | "paid";

/** The route key: the number once issued, `draft-<id>` before. */
export const invoiceKey = (i: InvoiceDto) => i.number ?? `draft-${i.id}`;

export function invoiceStatus(i: InvoiceDto): { tone: Tone; label: string } {
  switch (i.status) {
    case "paid":
      return { tone: "positive", label: `Paid ${day(i.paidOn)}` };
    case "overdue":
      return { tone: "negative", label: `Overdue ${i.daysOverdue} days` };
    case "partPaid":
      return { tone: "warning", label: "Part paid" };
    case "credited":
      return { tone: "neutral", label: "Credited" };
    case "draft":
      return { tone: "neutral", label: "Draft" };
    case "scheduled":
      return { tone: "info", label: `Scheduled ${day(i.scheduledFor)}` };
    default:
      return { tone: "info", label: `Due ${day(i.dueOn)}` };
  }
}

const columns: TableColumn<InvoiceDto>[] = [
  {
    id: "number",
    title: "Number",
    isRowHeader: true,
    width: "6.5rem",
    cell: (i) => i.number ?? <span className="text-ink-secondary">Draft</span>,
  },
  { id: "client", title: "Client", cell: (i) => i.client },
  { id: "issued", title: "Issued", width: "5.5rem", cell: (i) => day(i.issuedOn) },
  { id: "due", title: "Due", width: "5.5rem", cell: (i) => day(i.dueOn) },
  { id: "amount", title: "Amount", align: "end", width: "8.5rem", cell: (i) => money(i.gross) },
  {
    id: "status",
    title: "Status",
    width: "9.5rem",
    cell: (i) => {
      const s = invoiceStatus(i);
      return <Badge tone={s.tone}>{s.label}</Badge>;
    },
  },
];

function InvoicePaper({ invoice }: { invoice: InvoiceDto }) {
  return (
    <div className="rounded-inner bg-surface p-4 text-body shadow-group">
      <div className="flex items-baseline justify-between">
        <p className="font-semibold">
          {invoice.number ? `Faktura ${invoice.number}` : "Návrh faktury"}
        </p>
        <p className="text-footnote text-ink-secondary">daňový doklad</p>
      </div>
      <p className="mt-0.5 text-footnote text-ink-secondary">
        Odběratel: {invoice.client} · vystaveno {day(invoice.issuedOn, true)} · splatnost{" "}
        {day(invoice.dueOn, true)}
      </p>
      <table className="mt-3 w-full border-collapse">
        <tbody>
          {invoice.lines.map((line) => (
            <tr key={line.description} className="border-hairline border-t align-top">
              <td className="py-1.5 pr-2">
                {line.description}
                <span className="block text-footnote text-ink-secondary">
                  {line.quantity} {line.unit} × {money(line.unitPrice)} · DPH {line.vatRatePercent}{" "}
                  %
                </span>
              </td>
              <td className="py-1.5 text-right whitespace-nowrap">{money(line.base)}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <dl className="mt-2 space-y-0.5 border-hairline-strong border-t pt-2">
        <div className="flex justify-between">
          <dt className="text-ink-secondary">Základ</dt>
          <dd className="m-0">{money(invoice.base)}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-ink-secondary">DPH</dt>
          <dd className="m-0">{money(invoice.vat)}</dd>
        </div>
        <div className="flex justify-between font-semibold">
          <dt>Celkem k úhradě</dt>
          <dd className="m-0">{money(invoice.gross)}</dd>
        </div>
      </dl>
    </div>
  );
}

type Export =
  | { state: "busy" }
  | { state: "done"; text: string }
  | { state: "failed"; text: string };

/** Exchange formats the core writes, with the standard each follows. */
const XML_FORMATS: Record<string, string> = {
  isdoc: "ISDOC 6.0.2",
  ubl: "UBL 2.1, Peppol BIS Billing 3.0",
  cii: "CII D16B, EN 16931",
};

function InvoiceInspector({ invoice }: { invoice: InvoiceDto }) {
  const s = invoiceStatus(invoice);
  const posted = invoice.entryId !== null;
  const [exported, setExported] = useState<{ id: number; result: Export } | null>(null);
  const result = exported?.id === invoice.id ? exported.result : null;

  const exportAs = async (format: string) => {
    setExported({ id: invoice.id, result: { state: "busy" } });
    try {
      let text: string;
      if (format in XML_FORMATS) {
        const file = await unwrap(commands.invoiceXml(invoice.id, format));
        downloadText(file.fileName, file.xml, file.mediaType);
        text = `Saved ${file.fileName} (${XML_FORMATS[format]}).`;
      } else {
        const pdf = await unwrap(commands.invoicePdf(invoice.id, format));
        downloadBase64(pdf.fileName, pdf.pdfBase64, "application/pdf");
        const qr = pdf.spayd ? " with the QR Platba code" : "";
        text = `Saved ${pdf.fileName}${qr}.`;
      }
      setExported({ id: invoice.id, result: { state: "done", text } });
    } catch (e) {
      const text = e instanceof Error ? e.message : String(e);
      setExported({ id: invoice.id, result: { state: "failed", text } });
    }
  };

  return (
    <InspectorPane>
      <Inspector
        label={invoice.number ? `Invoice ${invoice.number}` : "Draft invoice"}
        title={invoice.number ? `Invoice ${invoice.number}` : "Draft invoice"}
        subtitle={invoice.client}
        accessory={<Badge tone={s.tone}>{s.label}</Badge>}
        actions={
          <>
            {posted && (
              <Menu
                label="Export"
                placement="bottom end"
                onAction={(format) => void exportAs(format)}
                trigger={
                  <Button variant="plain" icon={FileDown} isDisabled={result?.state === "busy"}>
                    Export
                  </Button>
                }
              >
                <MenuItem id="cs">Czech PDF</MenuItem>
                <MenuItem id="en">English PDF</MenuItem>
                <MenuSeparator />
                <MenuItem id="isdoc">ISDOC for accounting software</MenuItem>
                <MenuItem id="ubl">UBL for Peppol</MenuItem>
                <MenuItem id="cii">CII for Factur-X and ZUGFeRD</MenuItem>
              </Menu>
            )}
            {posted && invoice.status !== "paid" && (
              <Button variant="primary" onPress={() => navigate("bank")}>
                Match a payment
              </Button>
            )}
            {!posted && <DraftActions invoice={invoice} />}
          </>
        }
      >
        <InspectorSection title="Document">
          <InvoicePaper invoice={invoice} />
        </InspectorSection>
        <InspectorSection title="Payment">
          <FactList
            facts={[
              { label: "Paid", value: money(invoice.paid) },
              { label: "Open", value: <strong>{money(invoice.open)}</strong> },
              ...(invoice.paidOn
                ? [{ label: "Paid in full", value: day(invoice.paidOn, true) }]
                : []),
            ]}
          />
        </InspectorSection>
        {result && result.state !== "busy" && (
          <p
            role="status"
            className={`mt-3 text-footnote ${result.state === "failed" ? "text-negative-ink" : "text-ink-secondary"}`}
          >
            {result.text}
          </p>
        )}
        {posted && (
          <p className="mt-3 text-footnote text-ink-secondary">
            Posted as journal entry #{invoice.entryId}. Amounts come from the ledger.
          </p>
        )}
      </Inspector>
    </InspectorPane>
  );
}

/** Issue (with a confirmation naming the number) or delete a draft. */
function DraftActions({ invoice }: { invoice: InvoiceDto }) {
  const form = useQuery("invoice_form", () => unwrap(commands.invoiceForm()));
  const [failure, setFailure] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const run = async (action: () => Promise<string | null>) => {
    setBusy(true);
    setFailure([]);
    try {
      const next = await action();
      invalidateAll();
      navigate("invoices", next, true);
    } catch (e) {
      setFailure(problems(e));
      setBusy(false);
    }
  };
  const ready = form.state === "ready" ? form.data : null;
  return (
    <>
      <Popup
        label="Delete draft"
        placement="top end"
        trigger={
          <Button variant="plain" icon={Trash2} isDisabled={busy}>
            Delete
          </Button>
        }
      >
        <p className="max-w-64 text-body">
          Delete this draft? It has no number yet, so nothing is lost from the series.
        </p>
        <div className="mt-3 flex justify-end">
          <Button
            variant="destructive"
            onPress={() =>
              void run(async () => {
                await unwrap(commands.deleteInvoiceDraft(invoice.id));
                return null;
              })
            }
          >
            Delete draft
          </Button>
        </div>
      </Popup>
      <Popup
        label="Issue invoice"
        placement="top end"
        trigger={
          <Button variant="primary" isDisabled={busy || !ready}>
            Issue
          </Button>
        }
      >
        <div className="max-w-72 text-body">
          <p className="font-semibold">
            Issue as {ready?.nextNumber} on {day(ready?.today, true)}?
          </p>
          <p className="mt-1 text-ink-secondary">
            The invoice is numbered and posted to the ledger. After that it can't change;
            corrections are credit notes.
          </p>
          {failure.length > 0 && (
            <ul role="alert" className="mt-2 list-disc pl-5 text-negative-ink">
              {failure.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          )}
          <div className="mt-3 flex justify-end">
            <Button
              variant="primary"
              isDisabled={busy || !ready}
              onPress={() =>
                void run(async () => {
                  const issued = await unwrap(
                    commands.issueInvoice(invoice.id, ready?.today ?? ""),
                  );
                  return issued.number;
                })
              }
            >
              Issue invoice
            </Button>
          </div>
        </div>
      </Popup>
    </>
  );
}

const needsAttention = (i: InvoiceDto) => ["overdue", "open", "partPaid"].includes(i.status);
const unposted = (i: InvoiceDto) => i.status === "draft" || i.status === "scheduled";

/** Invoices: one list, sections by state, the document in the inspector. */
const FREQUENCY: Record<string, string> = {
  monthly: "every month",
  quarterly: "every quarter",
  yearly: "every year",
  weekly: "every week",
};

/** Recurring templates, each pausable; they run when the books open. */
function RecurringPopup() {
  const templates = useQuery("recurring_templates", () => unwrap(commands.recurringTemplates()));
  const [error, setError] = useState<string[]>([]);
  const list = templates.state === "ready" ? templates.data : [];
  return (
    <Popup
      label="Recurring invoices"
      placement="bottom end"
      trigger={
        <Button icon={Repeat} isDisabled={templates.state !== "ready"}>
          {`Recurring · ${list.length}`}
        </Button>
      }
    >
      <div className="w-96">
        <p className="px-1 pb-2 font-semibold text-body">Recurring invoices</p>
        {list.length === 0 ? (
          <p className="px-1 text-body text-ink-secondary">
            None yet. In a new invoice, choose how often it repeats.
          </p>
        ) : (
          <ul className="overflow-hidden rounded-inner bg-surface shadow-group">
            {list.map((t) => (
              <li
                key={t.id}
                className="flex items-center gap-3 border-hairline border-t px-3 py-2 first:border-t-0"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium text-body">{t.name}</p>
                  <p className="truncate text-footnote text-ink-secondary">
                    {t.client} · {money(t.gross)} {FREQUENCY[t.frequency] ?? t.frequency} ·{" "}
                    {t.active ? (t.next ? `next ${day(t.next, true)}` : "finished") : "paused"}
                    {t.autoIssue ? " · issued automatically" : " · as drafts"}
                  </p>
                </div>
                <Button
                  variant="plain"
                  onPress={() =>
                    void unwrap(commands.setRecurringActive(t.id, !t.active))
                      .then(() => {
                        setError([]);
                        invalidateAll();
                      })
                      .catch((e) => setError(problems(e)))
                  }
                >
                  {t.active ? "Pause" : "Resume"}
                </Button>
              </li>
            ))}
          </ul>
        )}
        {error.length > 0 && (
          <p role="alert" className="mt-2 text-footnote text-negative-ink">
            {error.join(" ")}
          </p>
        )}
      </div>
    </Popup>
  );
}

export function InvoicesScreen({ item }: { item: string | null }) {
  if (item === "new") return <InvoiceEditor />;
  return <InvoiceList item={item} />;
}

function InvoiceList({ item }: { item: string | null }) {
  const [filter, setFilter] = useState<Filter>("all");
  const invoices = useQuery("invoices", () => unwrap(commands.invoices()));
  const periods = useQuery("reporting_periods", () => unwrap(commands.reportingPeriods()));
  const today = periods.state === "ready" ? periods.data.today : null;
  const sheet = useQuery(`bs:${today}`, () =>
    today ? unwrap(commands.balanceSheet(today)) : Promise.resolve(null),
  );
  const fileInput = useRef<HTMLInputElement>(null);
  const [pending, setPending] = useState<PendingImport | null>(null);
  const [notice, setNotice] = useState<{ ok: boolean; lines: string[] } | null>(null);
  const pick = async (file: File) => {
    setNotice(null);
    try {
      const contentBase64 = await fileToBase64(file);
      const preview = await unwrap(commands.previewInvoiceImport(file.name, contentBase64));
      setPending({ fileName: file.name, contentBase64, preview });
    } catch (e) {
      setNotice({ ok: false, lines: problems(e) });
    }
  };
  if (pending) {
    return (
      <InvoiceImport
        pending={pending}
        onClose={(message) => {
          setPending(null);
          if (message) setNotice({ ok: true, lines: [message] });
        }}
      />
    );
  }
  return (
    <Loaded query={invoices}>
      {(all) => {
        const rows = all;
        const keep = (i: InvoiceDto) =>
          filter === "all" || (filter === "open" ? needsAttention(i) : i.status === "paid");
        const selected =
          rows.find((r) => invoiceKey(r) === item) ?? rows.find(needsAttention) ?? rows[0];
        const receivables =
          sheet.state === "ready"
            ? sheet.data?.assets.find((l) => l.code === "311")?.amount
            : undefined;
        return (
          <>
            <Toolbar
              title="Invoices"
              subtitle={`${all.length} invoices · ${all.filter((i) => i.status === "overdue").length} overdue · receivables ${money(receivables)}`}
            >
              <SegmentedControl
                label="Filter invoices"
                segments={[
                  { id: "all", label: "All" },
                  { id: "open", label: "Open" },
                  { id: "paid", label: "Paid" },
                ]}
                value={filter}
                onChange={setFilter}
              />
              <input
                ref={fileInput}
                type="file"
                accept=".xml,.csv"
                aria-label="Invoices exported from Pohoda or Fakturoid"
                className="hidden"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) void pick(f);
                  e.target.value = "";
                }}
              />
              <RecurringPopup />
              <Button icon={FileUp} onPress={() => fileInput.current?.click()}>
                Import…
              </Button>
              <Button variant="primary" icon={Plus} onPress={() => navigate("invoices", "new")}>
                New invoice
              </Button>
            </Toolbar>
            <ContentGroup>
              {notice && (
                <div
                  role={notice.ok ? "status" : "alert"}
                  className={`mb-2 px-2 text-footnote ${notice.ok ? "text-ink-secondary" : "text-negative-ink"}`}
                >
                  {notice.lines.map((l) => (
                    <p key={l}>{l}</p>
                  ))}
                </div>
              )}
              <DataTable
                label="Invoices"
                columns={columns}
                rowKey={invoiceKey}
                sections={[
                  {
                    id: "attention",
                    title: "Needs attention",
                    rows: rows.filter((r) => needsAttention(r) && keep(r)),
                  },
                  {
                    id: "drafts",
                    title: "Drafts and scheduled",
                    rows: rows.filter((r) => unposted(r) && keep(r)),
                  },
                  {
                    id: "paid",
                    title: "Paid",
                    rows: rows.filter((r) => r.status === "paid" && keep(r)),
                  },
                ].filter((s) => s.rows.length > 0)}
                selectedId={selected ? invoiceKey(selected) : null}
                onSelect={(id) => navigate("invoices", id, true)}
              />
            </ContentGroup>
            {selected ? (
              <InvoiceInspector invoice={selected} />
            ) : (
              <EmptyInspector label="Invoices" text="No invoice selected." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}
