import {
  commands,
  type TaxProjectionDto,
  type TaxScenarioDto,
  type TaxScenariosDto,
  unwrap,
} from "@skyla/ipc";
import { Badge, Button, FactList, InspectorSection, Select, TextField } from "@skyla/ui";
import { type FormEvent, useState } from "react";
import { problems, useQuery } from "../data";
import { day, money, signed } from "../format";
import { Reasons } from "./common";

const GROUPS = [
  { id: "trade", label: "Other trade (60 %)" },
  { id: "craft", label: "Craft trade (80 %)" },
  { id: "liberal", label: "Other self-employment (40 %)" },
] as const;

/** The key both the Taxes list and this panel use for the books' scenarios. */
export const BOOKS_SCENARIOS = "tax-scenarios:books";

export function useBooksScenarios() {
  return useQuery(BOOKS_SCENARIOS, () => unwrap(commands.incomeTaxScenarios(null)));
}

function ScenarioFacts({ s, baseline }: { s: TaxScenarioDto; baseline: boolean }) {
  const facts = [
    {
      label: s.flatRatePercent ? `Expenses (${s.flatRatePercent} %)` : "Expenses",
      value: s.capped ? `${money(s.expenses)} (capped)` : money(s.expenses),
    },
    { label: "Profit", value: money(s.profit) },
    { label: `Income tax (${s.taxRatePercent} %, after credit)`, value: money(s.tax) },
    { label: `Social insurance (${s.socialRatePercent} %)`, value: money(s.social) },
    { label: `Health insurance (${s.healthRatePercent} %)`, value: money(s.health) },
    { label: "Tax and insurance", value: <strong>{money(s.total)}</strong> },
  ];
  if (!baseline) {
    facts.push(
      { label: "Against the baseline", value: signed(s.vsBaselineTotal) },
      { label: "Pension assessment base", value: signed(s.vsBaselinePensionBase) },
    );
  }
  return <FactList facts={facts} />;
}

function Analysis({ t }: { t: TaxScenariosDto }) {
  return (
    <>
      {t.scenarios.map((s) => (
        <InspectorSection
          key={s.id}
          title={s.label}
          accessory={
            s.id === t.lowestTotal ? (
              <Badge tone="positive">Lowest total</Badge>
            ) : s.id === t.baseline ? (
              <Badge tone="neutral">Baseline</Badge>
            ) : undefined
          }
        >
          <ScenarioFacts s={s} baseline={s.id === t.baseline} />
        </InspectorSection>
      ))}
      {t.questions.length > 0 && (
        <InspectorSection title="Facts that would change this">
          <Reasons reasons={t.questions} />
        </InspectorSection>
      )}
      <InspectorSection title="Not evaluated">
        <Reasons reasons={t.notEvaluated} />
      </InspectorSection>
      <InspectorSection title="Assumptions">
        <Reasons
          reasons={[
            ...t.assumptions,
            "A lower social insurance base now means a lower pension later.",
          ]}
        />
      </InspectorSection>
    </>
  );
}

const EMPTY: TaxProjectionDto = {
  income: "",
  expenses: "",
  flatRate: "trade",
  purchaseDescription: "",
  purchasePrice: "",
};

/** The scenario engine's answer, from the books or a projection the user types. */
export function TaxScenariosPanel() {
  const books = useBooksScenarios();
  const [draft, setDraft] = useState<TaxProjectionDto>(EMPTY);
  const [projected, setProjected] = useState<TaxScenariosDto | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const set = (field: keyof TaxProjectionDto) => (value: string) =>
    setDraft((d) => ({ ...d, [field]: value }));

  async function compute(e: FormEvent) {
    e.preventDefault();
    try {
      setProjected(await unwrap(commands.incomeTaxScenarios(draft)));
      setErrors([]);
    } catch (err) {
      setErrors(problems(err));
    }
  }

  const shown = projected ?? (books.state === "ready" ? books.data : null);
  return (
    <>
      <InspectorSection title="Your projection for the year">
        <form aria-label="Projection" className="flex flex-col gap-2" onSubmit={compute}>
          <TextField label="Income (§ 7)" numeric value={draft.income} onChange={set("income")} />
          <TextField
            label="Actual expenses"
            numeric
            value={draft.expenses}
            onChange={set("expenses")}
          />
          <Select
            label="Flat-rate group"
            options={GROUPS}
            value={draft.flatRate ?? null}
            onChange={set("flatRate")}
          />
          <TextField
            label="Planned purchase"
            placeholder="Optional, e.g. Laptop"
            value={draft.purchaseDescription}
            onChange={set("purchaseDescription")}
          />
          <TextField
            label="Its price excluding VAT"
            numeric
            value={draft.purchasePrice}
            onChange={set("purchasePrice")}
          />
          {errors.length > 0 && (
            <div role="alert" className="text-footnote text-negative-ink">
              {errors.map((p) => (
                <p key={p}>{p}</p>
              ))}
            </div>
          )}
          <div className="flex justify-end gap-2 pt-1">
            {projected && (
              <Button
                onPress={() => {
                  setProjected(null);
                  setErrors([]);
                }}
              >
                Use the books
              </Button>
            )}
            <Button variant="primary" type="submit">
              Compare
            </Button>
          </div>
        </form>
      </InspectorSection>
      {shown && (
        <>
          <p className="mt-3 text-footnote text-ink-secondary" data-testid="scenario-source">
            {shown.source === "books"
              ? `From the books, ${day(shown.from)} – ${day(shown.to, true)}: income ${money(shown.income)}, expenses ${money(shown.actualExpenses)}.`
              : `Your projection for ${shown.year}: income ${money(shown.income)}, expenses ${money(shown.actualExpenses)}.`}
          </p>
          <Analysis t={shown} />
        </>
      )}
    </>
  );
}
