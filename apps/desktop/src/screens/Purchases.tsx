import {
  commands,
  type PurchaseDraftDto,
  type PurchaseDto,
  type PurchaseFormDto,
  type PurchaseLineDraftDto,
  unwrap,
} from "@skyla/ipc";
import {
  Badge,
  Button,
  ContentGroup,
  DataTable,
  FactList,
  Inspector,
  InspectorSection,
  Select,
  type TableColumn,
  TextField,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import { Plus, Trash2 } from "lucide-react";
import { useId, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { day, money } from "../format";
import { navigate } from "../router";
import { InspectorPane } from "../shell/Shell";
import { EmptyInspector, Loaded, TwoLine } from "./common";

const purchaseKey = (p: PurchaseDto) => String(p.entryId);

function purchaseStatus(p: PurchaseDto): { tone: Tone; label: string } {
  if (p.status === "paid") return { tone: "positive", label: "Paid" };
  if (p.status === "overdue") return { tone: "negative", label: `Overdue ${p.daysOverdue} days` };
  return { tone: "info", label: p.dueOn ? `Due ${day(p.dueOn)}` : "Open" };
}

const columns: TableColumn<PurchaseDto>[] = [
  { id: "number", title: "Number", isRowHeader: true, width: "9rem", cell: (p) => p.number },
  {
    id: "supplier",
    title: "Supplier",
    cell: (p) => <TwoLine title={p.supplier} detail={p.dic ? `DIČ ${p.dic}` : undefined} />,
  },
  { id: "date", title: "Tax point", width: "6rem", cell: (p) => day(p.issuedOn) },
  { id: "amount", title: "Amount", align: "end", width: "8.5rem", cell: (p) => money(p.gross) },
  {
    id: "status",
    title: "Status",
    width: "9.5rem",
    cell: (p) => {
      const s = purchaseStatus(p);
      return <Badge tone={s.tone}>{s.label}</Badge>;
    },
  },
];

/** Received invoices: what suppliers billed, and the editor to record one. */
export function PurchasesScreen({ item }: { item: string | null }) {
  if (item === "new") return <PurchaseEditor />;
  return <PurchaseList item={item} />;
}

function PurchaseList({ item }: { item: string | null }) {
  const purchases = useQuery("purchases", () => unwrap(commands.purchases()));
  return (
    <Loaded query={purchases}>
      {(all) => {
        const open = all.filter((p) => p.status !== "paid");
        const selected = all.find((p) => purchaseKey(p) === item) ?? open[0] ?? all[0];
        const owed = open.reduce((sum, p) => sum + p.open.minor, 0);
        return (
          <>
            <Toolbar
              title="Purchases"
              subtitle={`${all.length} received invoices · ${open.length} open · owed ${money({ minor: owed, currency: all[0]?.open.currency ?? "CZK" })}`}
            >
              <Button variant="primary" icon={Plus} onPress={() => navigate("purchases", "new")}>
                Record received invoice
              </Button>
            </Toolbar>
            <ContentGroup>
              <DataTable
                label="Received invoices"
                columns={columns}
                rowKey={purchaseKey}
                sections={[
                  { id: "open", title: "To pay", rows: open },
                  { id: "paid", title: "Paid", rows: all.filter((p) => p.status === "paid") },
                ].filter((s) => s.rows.length > 0)}
                selectedId={selected ? purchaseKey(selected) : null}
                onSelect={(id) => navigate("purchases", id, true)}
                empty={
                  <p className="p-5 text-body text-ink-secondary">
                    No received invoices yet. Record one when a supplier bills you.
                  </p>
                }
              />
            </ContentGroup>
            {selected ? (
              <PurchaseInspector purchase={selected} />
            ) : (
              <EmptyInspector label="Purchases" text="No received invoice selected." />
            )}
          </>
        );
      }}
    </Loaded>
  );
}

function PurchaseInspector({ purchase: p }: { purchase: PurchaseDto }) {
  const s = purchaseStatus(p);
  return (
    <InspectorPane>
      <Inspector
        label={`Received invoice ${p.number}`}
        title={`Received invoice ${p.number}`}
        subtitle={p.supplier}
        accessory={<Badge tone={s.tone}>{s.label}</Badge>}
        actions={
          p.status !== "paid" && (
            <Button variant="primary" onPress={() => navigate("bank")}>
              Match a payment
            </Button>
          )
        }
      >
        <InspectorSection title="Supplier">
          <FactList
            facts={[
              { label: "Name", value: p.supplier },
              { label: "DIČ", value: p.dic ?? "—" },
              { label: "Tax point", value: day(p.issuedOn, true) },
              { label: "Due", value: p.dueOn ? day(p.dueOn, true) : "—" },
            ]}
          />
        </InspectorSection>
        <InspectorSection title="Amounts">
          <FactList
            facts={[
              { label: "Without VAT", value: money(p.base) },
              { label: "VAT deducted", value: money(p.vat) },
              { label: "Total", value: money(p.gross) },
              { label: "Paid", value: money(p.paid) },
              { label: "Still owed", value: money(p.open) },
            ]}
          />
        </InspectorSection>
        <p className="mt-3 text-footnote text-ink-secondary">
          Posted as journal entry #{p.entryId}. The bank settles it when a payment matches.
        </p>
      </Inspector>
    </InspectorPane>
  );
}

interface LineDraft extends PurchaseLineDraftDto {
  readonly key: number;
}

let nextKey = 1;

const NO_VAT = "none";

function PurchaseEditor() {
  const form = useQuery("purchase_form", () => unwrap(commands.purchaseForm()));
  return <Loaded query={form}>{(f) => <Editor form={f} />}</Loaded>;
}

/** The webview collects what was typed; the core checks it and computes the VAT. */
function Editor({ form }: { form: PurchaseFormDto }) {
  const defaultAccount =
    form.accounts.find((a) => a.code === "501")?.code ?? form.accounts[0]?.code ?? "";
  const defaultVat = form.vatCodes[0]?.code ?? null;
  const emptyLine = (): LineDraft => ({
    key: nextKey++,
    description: "",
    account: defaultAccount,
    vatCode: defaultVat,
    base: "",
  });
  const [head, setHead] = useState({
    supplier: "",
    ico: "",
    dic: "",
    number: "",
    issueDate: form.today,
    taxPointDate: "",
    dueDate: "",
    statedVat: "",
  });
  const [lines, setLines] = useState<LineDraft[]>(() => [emptyLine()]);
  const [saving, setSaving] = useState(false);
  const [errors, setErrors] = useState<string[]>([]);
  const errorsId = useId();
  const field = (k: keyof typeof head) => (v: string) => setHead((h) => ({ ...h, [k]: v }));
  const update = (key: number, patch: Partial<PurchaseLineDraftDto>) =>
    setLines((all) => all.map((l) => (l.key === key ? { ...l, ...patch } : l)));
  const problemFor = (n: number | null, ...words: string[]) =>
    errors.find(
      (p) => (n === null || p.startsWith(`line ${n}:`)) && words.some((w) => p.includes(w)),
    );

  const save = async () => {
    setSaving(true);
    setErrors([]);
    const draft: PurchaseDraftDto = {
      supplier: head.supplier,
      ico: head.ico || null,
      dic: head.dic || null,
      number: head.number,
      issueDate: head.issueDate,
      taxPointDate: head.taxPointDate,
      dueDate: head.dueDate || null,
      lines: lines.map(({ key: _key, ...l }) => l),
      statedVat: head.statedVat || null,
    };
    try {
      const saved = await unwrap(commands.recordPurchase(draft));
      invalidateAll();
      navigate("purchases", String(saved.entryId), true);
    } catch (e) {
      setErrors(problems(e));
      setSaving(false);
    }
  };

  const accountOptions = form.accounts.map((a) => ({ id: a.code, label: a.code, detail: a.name }));
  const vatOptions = [
    ...form.vatCodes.map((c) => ({ id: c.code, label: `${c.ratePercent} %`, detail: c.name })),
    { id: NO_VAT, label: "No VAT", detail: "Nothing deducted" },
  ];

  return (
    <>
      <Toolbar title="Record received invoice" subtitle="Posted to the books when you save">
        <Button variant="plain" onPress={() => navigate("purchases", null)}>
          Cancel
        </Button>
        <Button variant="primary" onPress={() => void save()} isDisabled={saving}>
          Save and post
        </Button>
      </Toolbar>
      <ContentGroup>
        <form
          aria-label="Received invoice"
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
              <p className="font-semibold">The core couldn't record it:</p>
              <ul className="mt-1 list-disc pl-5">
                {errors.map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </div>
          )}
          <div className="grid grid-cols-3 gap-4">
            <TextField
              className="col-span-2"
              label="Supplier"
              value={head.supplier}
              onChange={field("supplier")}
              isInvalid={problemFor(null, "supplier's name", "already recorded") !== undefined}
            />
            <TextField
              label="Invoice number"
              value={head.number}
              onChange={field("number")}
              isInvalid={problemFor(null, "number") !== undefined}
            />
            <TextField
              label="IČO"
              value={head.ico}
              onChange={field("ico")}
              isInvalid={problemFor(null, "IČO") !== undefined}
            />
            <TextField
              label="DIČ"
              value={head.dic}
              onChange={field("dic")}
              isInvalid={problemFor(null, "DIČ") !== undefined}
            />
            <TextField
              label="Issued"
              value={head.issueDate}
              onChange={field("issueDate")}
              placeholder="YYYY-MM-DD"
              isInvalid={
                problemFor(null, "issue date", "outside the periods", "closed") !== undefined
              }
            />
            <TextField
              label="Tax point (DUZP)"
              value={head.taxPointDate}
              onChange={field("taxPointDate")}
              placeholder="Same as issued"
              isInvalid={problemFor(null, "tax point") !== undefined}
            />
            <TextField
              label="Due"
              value={head.dueDate}
              onChange={field("dueDate")}
              placeholder="YYYY-MM-DD"
              isInvalid={problemFor(null, "due date") !== undefined}
            />
          </div>

          <fieldset className="m-0 min-w-0 border-0 p-0">
            <legend className="sr-only">Lines</legend>
            <div className="grid grid-cols-[minmax(0,1fr)_7rem_8rem_8rem_1.75rem] items-start gap-x-2 gap-y-2">
              <div aria-hidden className="contents text-footnote font-medium text-ink-secondary">
                <span>Description</span>
                <span>Account</span>
                <span>VAT</span>
                <span className="text-right">Without VAT</span>
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
                      placeholder="What it was for"
                    />
                    <Select
                      label={`Line ${n} account`}
                      labelHidden
                      options={accountOptions}
                      value={l.account}
                      onChange={(account) => update(l.key, { account })}
                      isInvalid={problemFor(n, "account") !== undefined}
                    />
                    <Select
                      label={`Line ${n} VAT`}
                      labelHidden
                      options={vatOptions}
                      value={l.vatCode ?? NO_VAT}
                      onChange={(v) => update(l.key, { vatCode: v === NO_VAT ? null : v })}
                    />
                    <TextField
                      label={`Line ${n} amount without VAT`}
                      labelHidden
                      numeric
                      value={l.base}
                      onChange={(base) => update(l.key, { base })}
                      isInvalid={problemFor(n, "amount") !== undefined}
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
            <p className="mt-2 text-footnote text-ink-secondary">
              Type amounts as you would in Czech: 1 200,00. The core works out the VAT with the rule
              pack's rate on the tax point.
            </p>
            <div className="mt-2">
              <Button
                variant="plain"
                icon={Plus}
                onPress={() => setLines((all) => [...all, emptyLine()])}
              >
                Add line
              </Button>
            </div>
          </fieldset>

          {form.vatPayer && (
            <TextField
              className="max-w-xs"
              label="VAT on the invoice, to check (optional)"
              numeric
              value={head.statedVat}
              onChange={field("statedVat")}
              isInvalid={problemFor(null, "states VAT", "stated VAT") !== undefined}
            />
          )}
        </form>
      </ContentGroup>
      <InspectorPane>
        <Inspector label="Received invoice" title="Received invoice" subtitle="Not posted yet">
          <InspectorSection title="When you save">
            <p className="text-body text-ink-secondary">
              The core checks everything and posts one entry: each line to its expense account, the
              VAT it gives to 343 (when you deduct it), and the total owed to 321. The kontrolní
              hlášení lists it with the supplier's DIČ; the bank settles it when the payment
              arrives.
            </p>
          </InspectorSection>
        </Inspector>
      </InspectorPane>
    </>
  );
}
