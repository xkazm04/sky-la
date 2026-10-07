import {
  type Appearance,
  AppWindow,
  applyAppearance,
  Badge,
  Button,
  ContentGroup,
  DataTable,
  FactList,
  formatMinor,
  Inspector,
  InspectorSection,
  Kbd,
  Menu,
  MenuItem,
  MenuSeparator,
  Popup,
  SearchField,
  SegmentedControl,
  SourceList,
  StatusBar,
  storeAppearance,
  storedAppearance,
  type TableColumn,
  type Tone,
  Toolbar,
} from "@skyla/ui";
import {
  ArrowDownToLine,
  ChartNoAxesColumn,
  ChevronsUpDown,
  Copy,
  Ellipsis,
  FileText,
  Inbox,
  Landmark,
  LayoutGrid,
  LockKeyhole,
  Monitor,
  Moon,
  Percent,
  Plus,
  ScrollText,
  Send,
  Settings,
  ShieldCheck,
  Sparkles,
  Sun,
  Trash2,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";

/**
 * Sample invoices for the composition. Base, VAT and gross are stated, as the
 * core would send them: the webview never computes tax.
 */
interface InvoiceRow {
  id: string;
  number: string;
  client: string;
  issued: string;
  due: string;
  baseMinor: number;
  vatMinor: number;
  grossMinor: number;
  status: { tone: Tone; label: string };
}

const invoices: Record<"attention" | "drafts" | "paid", InvoiceRow[]> = {
  attention: [
    {
      id: "2026-102",
      number: "2026-102",
      client: "Studio Brno",
      issued: "25 Aug",
      due: "24 Sep",
      baseMinor: 8_520_000,
      vatMinor: 1_789_200,
      grossMinor: 10_309_200,
      status: { tone: "warning", label: "Part paid · 53 092,00 open" },
    },
  ],
  drafts: [
    {
      id: "2026-121",
      number: "2026-121",
      client: "Acme Analytics",
      issued: "—",
      due: "—",
      baseMinor: 8_000_000,
      vatMinor: 1_680_000,
      grossMinor: 9_680_000,
      status: { tone: "neutral", label: "Draft" },
    },
    {
      id: "2026-122",
      number: "2026-122",
      client: "Northwind Traders",
      issued: "1 Nov",
      due: "15 Nov",
      baseMinor: 14_000_000,
      vatMinor: 2_940_000,
      grossMinor: 16_940_000,
      status: { tone: "info", label: "Scheduled 1 Nov" },
    },
  ],
  paid: [
    {
      id: "2026-114",
      number: "2026-114",
      client: "Northwind Traders",
      issued: "15 Sep",
      due: "29 Sep",
      baseMinor: 7_000_000,
      vatMinor: 1_470_000,
      grossMinor: 8_470_000,
      status: { tone: "positive", label: "Paid 29 Sep" },
    },
    {
      id: "2026-097",
      number: "2026-097",
      client: "Acme Analytics",
      issued: "11 Aug",
      due: "8 Sep",
      baseMinor: 16_000_000,
      vatMinor: 3_360_000,
      grossMinor: 19_360_000,
      status: { tone: "positive", label: "Paid 8 Sep" },
    },
    {
      id: "2026-089",
      number: "2026-089",
      client: "Northwind Traders",
      issued: "14 Jul",
      due: "4 Aug",
      baseMinor: 14_000_000,
      vatMinor: 2_940_000,
      grossMinor: 16_940_000,
      status: { tone: "positive", label: "Paid 4 Aug" },
    },
  ],
};

const allInvoices = [...invoices.attention, ...invoices.drafts, ...invoices.paid];

const columns: TableColumn<InvoiceRow>[] = [
  { id: "number", title: "Number", isRowHeader: true, width: "6.5rem", cell: (r) => r.number },
  { id: "client", title: "Client", cell: (r) => r.client },
  { id: "issued", title: "Issued", width: "5rem", cell: (r) => r.issued },
  { id: "due", title: "Due", width: "5rem", cell: (r) => r.due },
  {
    id: "amount",
    title: "Amount",
    align: "end",
    width: "8.5rem",
    cell: (r) => formatMinor(r.grossMinor),
  },
  {
    id: "status",
    title: "Status",
    width: "13rem",
    cell: (r) => <Badge tone={r.status.tone}>{r.status.label}</Badge>,
  },
];

const sourceSections = [
  {
    id: "main",
    items: [
      { id: "overview", label: "Overview", icon: LayoutGrid },
      { id: "inbox", label: "Inbox", icon: Inbox, count: 3 },
    ],
  },
  {
    id: "books",
    title: "Books",
    items: [
      { id: "invoices", label: "Invoices", icon: FileText, count: 6 },
      { id: "bank", label: "Bank", icon: Landmark, count: 6 },
      { id: "statements", label: "Statements", icon: ChartNoAxesColumn },
      { id: "taxes", label: "Taxes", icon: Percent },
    ],
  },
  {
    id: "assist",
    title: "Assist",
    items: [{ id: "advisors", label: "Advisors", icon: Sparkles }],
  },
  {
    id: "app",
    title: "App",
    items: [
      { id: "register", label: "Egress register", icon: ScrollText },
      { id: "settings", label: "Settings", icon: Settings },
    ],
  },
];

const statusItems = [
  { id: "encryption", icon: LockKeyhole, label: "Encrypted", tone: "positive" as const },
  {
    id: "journal",
    icon: ShieldCheck,
    label: "Journal balanced · chain verified",
    tone: "positive" as const,
  },
  { id: "egress", icon: Send, label: "Nothing sent to Claude today" },
];

function Composition() {
  const [section, setSection] = useState("invoices");
  const [filter, setFilter] = useState<"all" | "open" | "overdue">("all");
  const [selected, setSelected] = useState("2026-114");
  const invoice = allInvoices.find((i) => i.id === selected) ?? allInvoices[0];
  if (!invoice) return null;
  return (
    <div
      className="h-[640px] overflow-hidden rounded-[22px] shadow-group"
      data-testid="composition"
    >
      <AppWindow
        sidebar={
          <>
            <p className="px-2.5 pb-3 font-semibold text-title">sky-la</p>
            <SourceList
              label="Sections"
              sections={sourceSections}
              selectedId={section}
              onSelect={setSection}
            />
            <Button
              variant="plain"
              className="mt-auto justify-start text-ink"
              icon={ChevronsUpDown}
            >
              Jan Novák · OSVČ
            </Button>
          </>
        }
        inspector={
          <Inspector
            label={`Invoice ${invoice.number}`}
            title={`Invoice ${invoice.number}`}
            subtitle={`${invoice.client} · issued ${invoice.issued}`}
            accessory={
              <Badge tone={invoice.status.tone}>{invoice.status.label.split(" · ")[0]}</Badge>
            }
            actions={
              <>
                <Popup
                  label="Explain this"
                  placement="top end"
                  trigger={
                    <Button variant="plain" icon={Sparkles}>
                      Explain this
                    </Button>
                  }
                >
                  <p className="max-w-64 text-body">
                    Draft for your review: this invoice adds {formatMinor(invoice.vatMinor)} of
                    output VAT to the October return. The figure comes from the ledger, not the
                    advisor.
                  </p>
                </Popup>
                <Button variant="primary">Record payment</Button>
              </>
            }
          >
            <InspectorSection title="Amounts">
              <FactList
                facts={[
                  { label: "Base", value: formatMinor(invoice.baseMinor) },
                  { label: "VAT", value: formatMinor(invoice.vatMinor) },
                  { label: "Total", value: <strong>{formatMinor(invoice.grossMinor)}</strong> },
                ]}
              />
            </InspectorSection>
            <InspectorSection title="Activity">
              <ol className="space-y-2 text-body">
                <li className="flex justify-between gap-3">
                  <span>Issued and posted</span>
                  <span className="text-ink-secondary">{invoice.issued}</span>
                </li>
                <li className="flex justify-between gap-3">
                  <span>Sent to the client</span>
                  <span className="text-ink-secondary">{invoice.issued}</span>
                </li>
              </ol>
            </InspectorSection>
          </Inspector>
        }
        status={<StatusBar items={statusItems} />}
      >
        <Toolbar title="Invoices" subtitle="6 invoices · 53 092,00 Kč open">
          <SegmentedControl
            label="Filter invoices"
            segments={[
              { id: "all", label: "All" },
              { id: "open", label: "Open" },
              { id: "overdue", label: "Overdue" },
            ]}
            value={filter}
            onChange={setFilter}
          />
          <SearchField
            label="Search invoices"
            placeholder="Search"
            shortcut="mod+f"
            className="w-44"
          />
          <Menu
            label="More actions"
            placement="bottom end"
            trigger={<Button aria-label="More actions" icon={Ellipsis} />}
          >
            <MenuItem id="duplicate" icon={Copy} shortcut="mod+d">
              Duplicate
            </MenuItem>
            <MenuItem id="export" icon={ArrowDownToLine}>
              Export ISDOC
            </MenuItem>
            <MenuSeparator />
            <MenuItem id="delete" icon={Trash2} destructive>
              Delete draft
            </MenuItem>
          </Menu>
          <Button variant="primary" icon={Plus}>
            New invoice
          </Button>
        </Toolbar>
        <ContentGroup>
          <DataTable
            label="Invoices"
            columns={columns}
            sections={[
              { id: "attention", title: "Needs attention", rows: invoices.attention },
              { id: "drafts", title: "Drafts and scheduled", rows: invoices.drafts },
              { id: "paid", title: "Paid", rows: invoices.paid },
            ]}
            selectedId={selected}
            onSelect={setSelected}
          />
        </ContentGroup>
      </AppWindow>
    </div>
  );
}

function Specimen({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="flex min-h-12 items-center gap-6 border-hairline border-t px-5 py-3 first:border-t-0">
      <h3 className="w-36 shrink-0 font-medium text-ink-secondary text-subhead">{title}</h3>
      <div className="flex flex-wrap items-center gap-3">{children}</div>
    </div>
  );
}

const swatches = [
  "backdrop",
  "surface",
  "surface-raised",
  "ink",
  "ink-secondary",
  "ink-tertiary",
  "accent",
  "accent-ink",
  "accent-tint",
  "positive-tint",
  "negative-tint",
  "warning-tint",
  "info-tint",
  "neutral-tint",
];

const typeScale = [
  ["text-headline", "Headline 17 · Invoices"],
  ["text-title", "Title 15 · Invoice 2026-114"],
  ["text-body", "Body 13 · Northwind Traders, 84 700,00 Kč"],
  ["text-subhead", "Subhead 12.5 · Due 29 Sep"],
  ["text-footnote", "Footnote 11.5 · 6 invoices · 53 092,00 Kč open"],
  ["text-caption", "Caption 11 · BOOKS"],
] as const;

function Primitives() {
  const [basis, setBasis] = useState<"accrual" | "cash">("accrual");
  return (
    <ContentGroup className="flex-none overflow-visible">
      <Specimen title="Button">
        <Button variant="primary" icon={Plus}>
          New invoice
        </Button>
        <Button>Glass</Button>
        <Button variant="plain">Plain</Button>
        <Button variant="destructive">Delete</Button>
        <Button size="small">Small</Button>
        <Button aria-label="More" icon={Ellipsis} />
        <Button isDisabled>Disabled</Button>
      </Specimen>
      <Specimen title="SegmentedControl">
        <SegmentedControl
          label="Basis"
          segments={[
            { id: "accrual", label: "Accrual" },
            { id: "cash", label: "Cash · daňová evidence" },
          ]}
          value={basis}
          onChange={setBasis}
        />
      </Specimen>
      <Specimen title="SearchField">
        <SearchField label="Search the ledger" placeholder="Search" shortcut="mod+k" />
      </Specimen>
      <Specimen title="Menu and Popup">
        <Menu label="Sort" trigger={<Button>Sort by</Button>}>
          <MenuItem id="due">Due date</MenuItem>
          <MenuItem id="amount">Amount</MenuItem>
          <MenuItem id="client">Client</MenuItem>
        </Menu>
        <Popup label="Filter" trigger={<Button>Filter…</Button>}>
          <p className="text-body">Filters go here.</p>
        </Popup>
      </Specimen>
      <Specimen title="Badge">
        <Badge tone="positive">Paid</Badge>
        <Badge tone="negative">Overdue 13 days</Badge>
        <Badge tone="warning">Part paid</Badge>
        <Badge tone="info">Scheduled</Badge>
        <Badge tone="neutral">Draft</Badge>
        <Badge tone="accent">Proposal</Badge>
      </Specimen>
      <Specimen title="Kbd">
        <Kbd keys="mod+k" />
        <Kbd keys="mod+shift+p" />
        <Kbd keys="esc" />
        <Kbd keys="mod+k" mac />
      </Specimen>
      <Specimen title="Type">
        <div className="flex flex-col gap-1.5">
          {typeScale.map(([cls, text]) => (
            <span key={cls} className={cls}>
              {text}
            </span>
          ))}
        </div>
      </Specimen>
      <Specimen title="Colour">
        {swatches.map((name) => (
          <span
            key={name}
            className="inline-flex items-center gap-1.5 text-footnote text-ink-secondary"
          >
            <span
              aria-hidden
              className="size-4 rounded-full shadow-[inset_0_0_0_0.5px_var(--sk-hairline-strong)]"
              style={{ background: `var(--sk-${name})` }}
            />
            {name}
          </span>
        ))}
      </Specimen>
    </ContentGroup>
  );
}

/** WP-08: every primitive and the tokens, in either appearance. Route `#/gallery`. */
export function Gallery() {
  const [appearance, setAppearance] = useState<Appearance>(storedAppearance);
  useEffect(() => {
    storeAppearance(appearance);
    return applyAppearance(appearance);
  }, [appearance]);
  return (
    <div className="min-h-full bg-backdrop">
      <div className="mx-auto flex max-w-[1320px] flex-col gap-4 p-4">
        <Toolbar title="Design gallery" subtitle="Direction A · tokens and primitives (WP-08)">
          <SegmentedControl
            label="Appearance"
            segments={[
              { id: "system", label: "System", icon: Monitor },
              { id: "light", label: "Light", icon: Sun },
              { id: "dark", label: "Dark", icon: Moon },
            ]}
            value={appearance}
            onChange={setAppearance}
          />
        </Toolbar>
        <Composition />
        <Primitives />
      </div>
    </div>
  );
}
