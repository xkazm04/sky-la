import {
  type ClientDto,
  commands,
  type InvoiceDraftDto,
  type InvoiceDraftLineDto,
  type InvoiceFormDto,
  unwrap,
} from "@skyla/ipc";
import {
  Button,
  Checkbox,
  ContentGroup,
  FactList,
  Inspector,
  InspectorSection,
  Select,
  TextField,
  Toolbar,
} from "@skyla/ui";
import { Plus, Trash2 } from "lucide-react";
import { useId, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { day } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { Loaded } from "./common";

interface LineDraft extends InvoiceDraftLineDto {
  readonly key: number;
}

const REPEAT = [
  { id: "never", label: "Just this once" },
  { id: "monthly", label: "Every month" },
  { id: "quarterly", label: "Every quarter" },
  { id: "yearly", label: "Every year" },
];

/** The customer list's last option: someone not invoiced before. */
const NEW_CLIENT = "\u0000new";
const BLANK_CLIENT: ClientDto = { name: "", ico: "", dic: "", address: "" };

let nextKey = 1;
const emptyLine = (vatCode: string): LineDraft => ({
  key: nextKey++,
  description: "",
  quantity: "1",
  unit: "h",
  unitPrice: "",
  vatCode,
});

/**
 * The invoice editor. The webview only collects what was typed; the core
 * parses the numbers, checks everything, computes the totals and saves the
 * draft. Issuing happens from the draft's inspector.
 */
export function InvoiceEditor() {
  const form = useQuery("invoice_form", () => unwrap(commands.invoiceForm()));
  return <Loaded query={form}>{(f) => <Editor form={f} />}</Loaded>;
}

/** The editor on a saved draft, to change it before it's issued. */
export function DraftEditor({ id }: { id: number }) {
  const form = useQuery("invoice_form", () => unwrap(commands.invoiceForm()));
  const draft = useQuery(`invoice_draft:${id}`, () => unwrap(commands.invoiceDraft(id)));
  return (
    <Loaded query={form}>
      {(f) => (
        <Loaded query={draft}>{(d) => <Editor form={f} editing={{ id, draft: d }} />}</Loaded>
      )}
    </Loaded>
  );
}

function Editor({
  form,
  editing,
}: {
  form: InvoiceFormDto;
  editing?: { id: number; draft: InvoiceDraftDto };
}) {
  const defaultVat = form.vatCodes[0]?.code ?? "";
  const initial = editing?.draft;
  const [client, setClient] = useState<string | null>(
    initial ? initial.client : form.clients.length === 0 ? NEW_CLIENT : null,
  );
  const [newClient, setNewClient] = useState<ClientDto>(BLANK_CLIENT);
  const isNew = client === NEW_CLIENT;
  const newField = (k: keyof ClientDto) => (v: string) => setNewClient((c) => ({ ...c, [k]: v }));
  const [dueDays, setDueDays] = useState(
    String(initial?.dueDays ?? (form.dueDays.includes(14) ? 14 : form.dueDays[0])),
  );
  const [note, setNote] = useState(initial?.note ?? "");
  const [lines, setLines] = useState<LineDraft[]>(() =>
    initial && initial.lines.length > 0
      ? initial.lines.map((l) => ({ ...l, key: nextKey++ }))
      : [emptyLine(defaultVat)],
  );
  const [saving, setSaving] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  const [repeat, setRepeat] = useState<string>("never");
  const [start, setStart] = useState(form.today);
  const [templateName, setTemplateName] = useState("");
  const [autoIssue, setAutoIssue] = useState(false);
  const errorsId = useId();

  const update = (key: number, patch: Partial<InvoiceDraftLineDto>) =>
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...patch } : l)));

  const save = async () => {
    setSaving(true);
    setErrors([]);
    const draft = {
      client: isNew ? "" : (client ?? ""),
      newClient: isNew ? newClient : null,
      dueDays: Number(dueDays),
      note,
      lines: lines.map(({ key: _key, ...line }) => line),
    };
    try {
      if (editing) {
        await unwrap(commands.updateInvoiceDraft(editing.id, draft));
        invalidateAll();
        navigate("invoices", `draft-${editing.id}`, true);
        return;
      }
      if (repeat !== "never") {
        await unwrap(
          commands.createRecurring({
            name: templateName,
            draft,
            frequency: repeat,
            interval: 1,
            start,
            autoIssue,
          }),
        );
        invalidateAll();
        navigate("invoices", null, true);
        return;
      }
      const saved = await unwrap(commands.createInvoiceDraft(draft));
      invalidateAll();
      navigate("invoices", `draft-${saved.id}`, true);
    } catch (e) {
      setErrors(problems(e));
      setSaving(false);
    }
  };

  // Point each problem the core listed at the field it's about.
  const problemFor = (n: number | null, ...words: string[]) =>
    errors.find(
      (p) => (n === null || p.startsWith(`line ${n}:`)) && words.some((w) => p.includes(w)),
    );

  const vatOptions = form.vatCodes.map((c) => ({
    id: c.code,
    label: `${c.ratePercent} %`,
    detail: c.name,
  }));

  return (
    <>
      <Toolbar
        title={editing ? "Edit draft" : "New invoice"}
        subtitle={
          editing
            ? `Draft · checked again when saved · issued as ${form.nextNumber} or later`
            : repeat === "never"
              ? `Draft · issued as ${form.nextNumber} or later`
              : `Recurring template · ${REPEAT.find((r) => r.id === repeat)?.label.toLowerCase()}, from ${day(start, true)}`
        }
      >
        <Button
          variant="plain"
          onPress={() => navigate("invoices", editing ? `draft-${editing.id}` : null)}
        >
          Cancel
        </Button>
        <Button variant="primary" onPress={() => void save()} isDisabled={saving}>
          {editing ? "Save changes" : repeat === "never" ? "Save draft" : "Save template"}
        </Button>
      </Toolbar>
      <ContentGroup>
        <form
          aria-label={editing ? "Edit draft" : "New invoice"}
          aria-describedby={errors.length > 0 ? errorsId : undefined}
          className="flex flex-col gap-5 p-5"
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          {errors.length > 0 && (
            <div
              id={errorsId}
              role="alert"
              className="rounded-inner bg-negative-tint px-3 py-2 text-body text-negative-ink"
            >
              <p className="font-semibold">The core couldn't save the draft:</p>
              <ul className="mt-1 list-disc pl-5">
                {errors.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </div>
          )}
          <div className="grid grid-cols-[minmax(0,2fr)_minmax(0,1fr)] gap-4">
            <Select
              label="Customer"
              options={[
                ...form.clients.map((c) => ({
                  id: c.name,
                  label: c.name,
                  detail: [c.ico && `IČO ${c.ico}`, c.dic && `DIČ ${c.dic}`]
                    .filter(Boolean)
                    .join(" · "),
                })),
                { id: NEW_CLIENT, label: "New customer…", detail: "Not invoiced before" },
              ]}
              value={client}
              onChange={setClient}
              placeholder="Choose a customer…"
              isInvalid={problemFor(null, "customer") !== undefined}
            />
            <Select
              label="Payment terms"
              options={form.dueDays.map((d) => ({ id: String(d), label: `${d} days` }))}
              value={dueDays}
              onChange={setDueDays}
            />
          </div>

          {isNew && (
            <fieldset className="m-0 grid min-w-0 grid-cols-2 gap-4 border-0 p-0">
              <legend className="sr-only">New customer</legend>
              <TextField
                className="col-span-2"
                label="Customer's legal name"
                value={newClient.name}
                onChange={newField("name")}
                isInvalid={problemFor(null, "name", "already a customer") !== undefined}
              />
              <TextField
                label="IČO"
                value={newClient.ico ?? ""}
                onChange={newField("ico")}
                isInvalid={problemFor(null, "IČO") !== undefined}
              />
              <TextField
                label="DIČ"
                value={newClient.dic ?? ""}
                onChange={newField("dic")}
                isInvalid={problemFor(null, "DIČ") !== undefined}
              />
              <TextField
                className="col-span-2"
                label="Address, as printed on the invoice"
                value={newClient.address ?? ""}
                onChange={newField("address")}
                isInvalid={problemFor(null, "address") !== undefined}
              />
            </fieldset>
          )}

          <fieldset className="m-0 min-w-0 border-0 p-0">
            <legend className="sr-only">Lines</legend>
            <div className="grid grid-cols-[minmax(0,1fr)_4.5rem_4rem_7.5rem_6.5rem_1.75rem] items-start gap-x-2 gap-y-2">
              <div aria-hidden className="contents text-footnote font-medium text-ink-secondary">
                <span>Description</span>
                <span className="text-right">Quantity</span>
                <span>Unit</span>
                <span className="text-right">Unit price</span>
                <span>VAT</span>
                <span />
              </div>
              {lines.map((l, i) => {
                const n = i + 1;
                return (
                  <div key={l.key} className="contents">
                    <TextField
                      label={`Line ${n} description`}
                      labelHidden
                      value={l.description}
                      onChange={(description) => update(l.key, { description })}
                      placeholder="What you supplied"
                    />
                    <TextField
                      label={`Line ${n} quantity`}
                      labelHidden
                      numeric
                      isInvalid={problemFor(n, "quantity") !== undefined}
                      value={l.quantity}
                      onChange={(quantity) => update(l.key, { quantity })}
                    />
                    <TextField
                      label={`Line ${n} unit`}
                      labelHidden
                      value={l.unit}
                      onChange={(unit) => update(l.key, { unit })}
                    />
                    <TextField
                      label={`Line ${n} unit price`}
                      labelHidden
                      numeric
                      isInvalid={problemFor(n, "unit price") !== undefined}
                      placeholder="1 200,00"
                      value={l.unitPrice}
                      onChange={(unitPrice) => update(l.key, { unitPrice })}
                    />
                    <Select
                      label={`Line ${n} VAT`}
                      labelHidden
                      isInvalid={problemFor(n, "VAT code") !== undefined}
                      options={vatOptions}
                      value={l.vatCode}
                      onChange={(vatCode) => update(l.key, { vatCode })}
                    />
                    <Button
                      variant="plain"
                      icon={Trash2}
                      aria-label={`Remove line ${n}`}
                      isDisabled={lines.length === 1}
                      onPress={() => setLines((all) => all.filter((x) => x.key !== l.key))}
                    />
                  </div>
                );
              })}
            </div>
            <p className="mt-1.5 text-footnote text-ink-secondary">
              Prices exclude VAT. Type amounts as you would in Czech: 1 200,00.
            </p>
            <Button
              variant="plain"
              icon={Plus}
              className="mt-2 -ml-2"
              onPress={() => setLines((all) => [...all, emptyLine(defaultVat)])}
            >
              Add line
            </Button>
          </fieldset>

          <TextField label="Note on the invoice" multiline value={note} onChange={setNote} />
          <div className={editing ? "hidden" : "grid grid-cols-3 gap-4"}>
            <Select label="Repeat" options={REPEAT} value={repeat} onChange={setRepeat} />
            {repeat !== "never" && (
              <>
                <TextField
                  label="First invoice on"
                  value={start}
                  onChange={setStart}
                  placeholder="YYYY-MM-DD"
                  isInvalid={problemFor(null, "first date") !== undefined}
                />
                <TextField
                  label="Template name"
                  value={templateName}
                  onChange={setTemplateName}
                  placeholder="The customer's name"
                />
                <div className="col-span-3">
                  <Checkbox isSelected={autoIssue} onChange={setAutoIssue}>
                    Issue each one automatically (otherwise each waits as a draft)
                  </Checkbox>
                  <p className="mt-1 text-footnote text-ink-secondary">
                    Line descriptions and the note may use {"{month}"}, {"{MM}"} and {"{YYYY}"} for
                    each invoice's month.
                  </p>
                </div>
              </>
            )}
          </div>
          <button type="submit" hidden aria-hidden tabIndex={-1} />
        </form>
      </ContentGroup>
      <InspectorPane>
        <Inspector
          label={editing ? "Draft being edited" : "New invoice"}
          title={editing ? editing.draft.client : "New invoice"}
          subtitle="Draft"
        >
          <InspectorSection title="When you issue it">
            <FactList
              facts={[
                { label: "Number", value: form.nextNumber },
                { label: "Issue date", value: day(form.today, true) },
                {
                  label: "Due",
                  value: `${dueDays} days after issue`,
                },
              ]}
            />
          </InspectorSection>
          <p className="mt-3 text-footnote text-ink-secondary">
            Saving keeps a draft you can still change or delete. The core computes VAT and totals
            with the rule pack. Issuing posts the invoice to the ledger; after that it can't change,
            and corrections are credit notes.
          </p>
        </Inspector>
      </InspectorPane>
    </>
  );
}
