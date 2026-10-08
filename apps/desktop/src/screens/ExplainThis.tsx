import { commands, type ExplainTargetDto, type ExplanationDto, unwrap } from "@skyla/ipc";
import { Badge, Button, InspectorSection, Popup } from "@skyla/ui";
import { Sparkles } from "lucide-react";
import { useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { keepFiguresTogether } from "../format";
import { navigate } from "../router";

/**
 * "Explain this" (D-015): asks the advisor about a figure after saying what
 * it sends, and shows the explanation only once the core has checked every
 * figure and citation against the books.
 */
export function ExplainThis({
  target,
  onCites,
}: {
  target: ExplainTargetDto;
  onCites: (cites: number[]) => void;
}) {
  const policies = useQuery("egress_policies", () => commands.egressPolicies());
  const policy =
    policies.state === "ready" ? policies.data.find((p) => p.task === "explain.figure") : undefined;
  const [result, setResult] = useState<ExplanationDto | null>(null);
  const [failure, setFailure] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const run = async () => {
    setBusy(true);
    setFailure([]);
    try {
      const r = await unwrap(commands.explain(target, true));
      setResult(r);
      onCites(r.status === "accepted" ? r.cites : []);
      invalidateAll();
    } catch (e) {
      setFailure(problems(e));
    } finally {
      setBusy(false);
    }
  };
  if (policy?.policy === "never") {
    return (
      <p className="mt-4 text-footnote text-ink-secondary">
        Explanations are turned off in Settings.
      </p>
    );
  }
  return (
    <InspectorSection title="Explain this">
      <div className="flex flex-col gap-2">
        <div>
          {policy?.policy === "always" ? (
            <Button icon={Sparkles} isDisabled={busy} onPress={() => void run()}>
              Explain this
            </Button>
          ) : (
            <Popup
              label="Explain this"
              placement="bottom start"
              trigger={
                <Button icon={Sparkles} isDisabled={busy}>
                  Explain this…
                </Button>
              }
            >
              {(close) => (
                <div className="max-w-80 text-body">
                  <p className="font-semibold">
                    Ask your Claude Code installation to explain this?
                  </p>
                  <p className="mt-1 text-ink-secondary">
                    It may send: {policy?.scope.join("; ") ?? "totals"}. Names become pseudonyms;
                    account and personal ID numbers are never sent. The run is recorded in the
                    egress register.
                  </p>
                  <div className="mt-3 flex justify-end">
                    <Button
                      variant="primary"
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
          )}
        </div>
        {failure.length > 0 && (
          <p role="alert" className="text-footnote text-negative-ink">
            {failure.join(" ")}
          </p>
        )}
        {result?.status === "accepted" && (
          <div data-testid="explanation" className="rounded-inner bg-surface p-3 shadow-group">
            <div className="mb-1.5 flex items-center justify-between gap-2">
              <Badge tone="info">Explanation</Badge>
              <span className="text-footnote text-ink-secondary">
                {result.grounded} figures and {result.cites.length} entries checked
              </span>
            </div>
            <p className="text-body">{keepFiguresTogether(result.text ?? "")}</p>
            <p className="mt-2 text-footnote text-ink-secondary">
              Cites {result.cites.map((c) => `#${c}`).join(", ")}, highlighted below.{" "}
              {result.runId && (
                <Button
                  variant="plain"
                  size="small"
                  onPress={() => navigate("register", result.runId)}
                >
                  What was shared
                </Button>
              )}
            </p>
          </div>
        )}
        {result && result.status !== "accepted" && (
          <div
            role="alert"
            className="rounded-inner bg-negative-tint px-3 py-2 text-body text-negative-ink"
          >
            <p className="font-semibold">
              {result.status === "rejected"
                ? "The explanation was rejected, so nothing from it is shown:"
                : "It didn't run:"}
            </p>
            <ul className="mt-1 list-disc pl-5">
              {result.problems.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </InspectorSection>
  );
}
