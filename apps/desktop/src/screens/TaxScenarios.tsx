import {
  commands,
  type TaxAdviceDto,
  type TaxProjectionDto,
  type TaxScenarioDto,
  type TaxScenariosDto,
  unwrap,
} from "@skyla/ipc";
import { Badge, Button, FactList, InspectorSection, Popup, Select, TextField } from "@skyla/ui";
import { type FormEvent, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { day, keepFiguresTogether, money, signed } from "../format";
import { navigate } from "../router";
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

/** Asks the tax advisor to explain what's on screen, after saying what it sends. */
function AskTaxAdvisor({ projection }: { projection: TaxProjectionDto | null }) {
  const policies = useQuery("egress_policies", () => commands.egressPolicies());
  const policy =
    policies.state === "ready" ? policies.data.find((p) => p.task === "tax.scenarios") : undefined;
  const [advice, setAdvice] = useState<TaxAdviceDto | null>(null);
  const [failure, setFailure] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    setFailure([]);
    try {
      const a = await unwrap(commands.runTaxAdvisor(projection, true));
      setAdvice(a);
      invalidateAll();
    } catch (e) {
      setFailure(problems(e));
    } finally {
      setBusy(false);
    }
  };
  const ask =
    policy?.policy === "never" ? (
      <p className="text-footnote text-ink-secondary">The tax advisor is turned off in Settings.</p>
    ) : policy?.policy === "always" ? (
      <Button isDisabled={busy} onPress={() => void run()}>
        Explain with the tax advisor
      </Button>
    ) : (
      <Popup
        label="Send to the tax advisor"
        placement="bottom end"
        trigger={<Button isDisabled={busy}>Explain with the tax advisor…</Button>}
      >
        {(close) => (
          <div className="max-w-80 text-body">
            <p className="font-semibold">Send these scenarios to your Claude Code installation?</p>
            <p className="mt-1 text-ink-secondary">
              It may send: {policy?.scope.join("; ") ?? "totals"}. IBANs, account, personal ID and
              card numbers are never sent, and names become pseudonyms. The run is recorded in the
              egress register.
            </p>
            <div className="mt-3 flex justify-end">
              <Button
                variant="primary"
                isDisabled={busy}
                onPress={() => {
                  close();
                  void run();
                }}
              >
                Send and explain
              </Button>
            </div>
          </div>
        )}
      </Popup>
    );
  return (
    <InspectorSection title="Tax advisor">
      <div className="flex flex-col gap-2">
        <div>{ask}</div>
        {failure.length > 0 && (
          <p role="alert" className="text-footnote text-negative-ink">
            {failure.join(" ")}
          </p>
        )}
        {advice && advice.status === "accepted" && (
          <div data-testid="tax-advice" className="rounded-inner bg-surface p-3 shadow-group">
            <div className="mb-1.5 flex items-center justify-between gap-2">
              <Badge tone="info">Draft for your review</Badge>
              <span className="text-footnote text-ink-secondary">
                {advice.grounded} figures checked against the engine
              </span>
            </div>
            <p className="text-body">{keepFiguresTogether(advice.explanation ?? "")}</p>
            {advice.questions.length > 0 && (
              <div className="mt-2">
                <p className="font-medium text-footnote">It needs to know</p>
                <Reasons reasons={advice.questions} />
              </div>
            )}
            <p className="mt-2 text-footnote text-ink-secondary">
              A scenario, not tax advice.{" "}
              {advice.runId && (
                <Button
                  variant="plain"
                  size="small"
                  onPress={() => navigate("register", advice.runId)}
                >
                  What was shared
                </Button>
              )}
            </p>
          </div>
        )}
        {advice && advice.status !== "accepted" && (
          <div
            role="alert"
            className="rounded-inner bg-negative-tint px-3 py-2 text-body text-negative-ink"
          >
            <p className="font-semibold">
              {advice.status === "rejected"
                ? "The answer was rejected, so nothing from it is shown:"
                : "The tax advisor didn't run:"}
            </p>
            <ul className="mt-1 list-disc pl-5">
              {advice.problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </InspectorSection>
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
  // The projection that was compared, which the advisor explains.
  const [used, setUsed] = useState<TaxProjectionDto | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const set = (field: keyof TaxProjectionDto) => (value: string) =>
    setDraft((d) => ({ ...d, [field]: value }));

  async function compute(e: FormEvent) {
    e.preventDefault();
    try {
      setProjected(await unwrap(commands.incomeTaxScenarios(draft)));
      setUsed(draft);
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
                  setUsed(null);
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
          <AskTaxAdvisor
            key={projected ? "projection" : "books"}
            projection={projected ? used : null}
          />
          <Analysis t={shown} />
        </>
      )}
    </>
  );
}
