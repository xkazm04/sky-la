import { commands, type ImportDocumentDto, type ImportPreviewDto, unwrap } from "@skyla/ipc";
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
import { useState } from "react";
import { invalidateAll, problems } from "../data";
import { day, money } from "../format";
import { InspectorPane } from "../shell/Shell";

/** A file picked for import, with what the core says it holds. */
export interface PendingImport {
  fileName: string;
  contentBase64: string;
  preview: ImportPreviewDto;
}

const WONT = { tone: "warning", label: "Won't import" } as const;
const STATUS: Record<string, { tone: Tone; label: string }> = {
  new: { tone: "positive", label: "New" },
  duplicate: { tone: "neutral", label: "Already here" },
  problem: WONT,
};

const columns: ReadonlyArray<TableColumn<ImportDocumentDto>> = [
  { id: "number", title: "Number", width: "8rem", cell: (d) => d.number },
  { id: "customer", title: "Customer", cell: (d) => d.customer },
  { id: "issued", title: "Issued", width: "5.5rem", cell: (d) => day(d.issueDate) },
  { id: "total", title: "Total", align: "end", width: "8.5rem", cell: (d) => money(d.total) },
  {
    id: "status",
    title: "Status",
    width: "9.5rem",
    cell: (d) => {
      const s = STATUS[d.status] ?? WONT;
      return <Badge tone={s.tone}>{s.label}</Badge>;
    },
  },
];

const key = (d: ImportDocumentDto) => `${d.position}:${d.number}`;

function count(n: number, one: string, many: string) {
  return `${n} ${n === 1 ? one : many}`;
}

/** The preview of an invoice import from Pohoda or Fakturoid, and the go. */
export function InvoiceImport({
  pending,
  onClose,
}: {
  pending: PendingImport;
  onClose: (message: string | null) => void;
}) {
  const { preview } = pending;
  const [selected, setSelected] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  const docs = preview.documents;
  const of = (status: string) => docs.filter((d) => d.status === status);
  const chosen = docs.find((d) => key(d) === selected) ?? of("new")[0] ?? docs[0];

  const commit = async () => {
    setBusy(true);
    setErrors([]);
    try {
      const done = await unwrap(
        commands.commitInvoiceImport(pending.fileName, pending.contentBase64),
      );
      invalidateAll();
      onClose(
        `Imported ${count(done.imported.length, "invoice", "invoices")} from ${done.source}: ${done.imported.join(", ")}.`,
      );
    } catch (e) {
      setErrors(problems(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Toolbar
        title={`Import from ${preview.source}`}
        subtitle={`${preview.file} · ${count(preview.new, "new invoice", "new invoices")} · ${of("duplicate").length} already here · ${of("problem").length} won't import`}
      >
        <Button isDisabled={busy} onPress={() => onClose(null)}>
          Cancel
        </Button>
        <Button
          variant="primary"
          isDisabled={busy || preview.new === 0}
          onPress={() => void commit()}
        >
          {preview.new === 0
            ? "Nothing to import"
            : `Import ${count(preview.new, "invoice", "invoices")}`}
        </Button>
      </Toolbar>
      <ContentGroup>
        {(errors.length > 0 || preview.problems.length > 0) && (
          <div
            role={errors.length > 0 ? "alert" : "status"}
            className={`mb-2 px-2 text-footnote ${errors.length > 0 ? "text-negative-ink" : "text-ink-secondary"}`}
          >
            {[...errors, ...preview.problems.map((p) => `Skipped: ${p}`)].map((l) => (
              <p key={l}>{l}</p>
            ))}
          </div>
        )}
        <DataTable
          label="Invoices in the file"
          columns={columns}
          rowKey={key}
          sections={[
            { id: "new", title: "Will be imported", rows: of("new") },
            { id: "problem", title: "Won't be imported", rows: of("problem") },
            { id: "duplicate", title: "Already in these books", rows: of("duplicate") },
          ].filter((s) => s.rows.length > 0)}
          selectedId={chosen ? key(chosen) : null}
          onSelect={setSelected}
        />
      </ContentGroup>
      <InspectorPane>
        <Inspector
          label="Imported invoice"
          title={chosen ? `Faktura ${chosen.number}` : "Nothing in the file"}
          subtitle={
            chosen ? `${chosen.customer} · issued ${day(chosen.issueDate, true)}` : undefined
          }
        >
          {chosen && (
            <>
              {chosen.problems.length > 0 && (
                <InspectorSection title={chosen.status === "problem" ? "Why not" : "Note"}>
                  <ul className="list-disc pl-5 text-body">
                    {chosen.problems.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                </InspectorSection>
              )}
              <InspectorSection title="Amounts as the file states them">
                <FactList
                  facts={[
                    { label: "Base", value: money(chosen.base) },
                    { label: "VAT", value: money(chosen.vat) },
                    { label: "Total", value: money(chosen.total) },
                  ]}
                />
              </InspectorSection>
            </>
          )}
          <InspectorSection title="What importing does">
            <p className="text-body text-ink-secondary">
              Each new invoice is posted as one entry (receivable, revenue and VAT at the rate the
              rule pack gives on its tax point) and kept as an issued document with its original
              number. Payments aren't imported: your bank statements settle them as usual.
            </p>
          </InspectorSection>
        </Inspector>
      </InspectorPane>
    </>
  );
}
