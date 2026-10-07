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
  Popup,
  SegmentedControl,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { FileDown, Plus } from "lucide-react";
import { useState } from "react";
import { useQuery } from "../data";
import { downloadBase64, downloadText } from "../download";
import { day, money } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { DemoNote, EmptyInspector, Loaded } from "./common";

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

function InvoiceInspector({ invoice }: { invoice: InvoiceDto }) {
  const s = invoiceStatus(invoice);
  const posted = invoice.entryId !== null;
  const [exported, setExported] = useState<{ id: number; result: Export } | null>(null);
  const result = exported?.id === invoice.id ? exported.result : null;

  const exportAs = async (format: string) => {
    setExported({ id: invoice.id, result: { state: "busy" } });
    try {
      let text: string;
      if (format === "isdoc") {
        const file = await unwrap(commands.invoiceIsdoc(invoice.id));
        downloadText(file.fileName, file.xml, file.mediaType);
        text = `Saved ${file.fileName} (ISDOC 6.0.2).`;
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
                <MenuItem id="isdoc">ISDOC for accounting software</MenuItem>
              </Menu>
            )}
            {posted && invoice.status !== "paid" && (
              <Button variant="primary" onPress={() => navigate("bank")}>
                Match a payment
              </Button>
            )}
            {!posted && (
              <Popup
                label="Issue"
                placement="top end"
                trigger={<Button variant="primary">Issue</Button>}
              >
                <DemoNote>
                  Issuing assigns the number and posts it through the kernel (WP-11).
                </DemoNote>
              </Popup>
            )}
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

const needsAttention = (i: InvoiceDto) => ["overdue", "open", "partPaid"].includes(i.status);
const unposted = (i: InvoiceDto) => i.status === "draft" || i.status === "scheduled";

/** Invoices: one list, sections by state, the document in the inspector. */
export function InvoicesScreen({ item }: { item: string | null }) {
  const [filter, setFilter] = useState<Filter>("all");
  const invoices = useQuery("invoices", () => unwrap(commands.invoices()));
  const sheet = useQuery("bs:2026-10-07", () => unwrap(commands.balanceSheet("2026-10-07")));
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
            ? sheet.data.assets.find((l) => l.code === "311")?.amount
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
              <Popup
                label="New invoice"
                placement="bottom end"
                trigger={
                  <Button variant="primary" icon={Plus}>
                    New invoice
                  </Button>
                }
              >
                <DemoNote>The invoice editor arrives with WP-11.</DemoNote>
              </Popup>
            </Toolbar>
            <ContentGroup>
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
