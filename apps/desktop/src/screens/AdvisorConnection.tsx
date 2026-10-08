import type { AdvisorStatusDto } from "@skyla/ipc";
import { Badge, Button, type Tone } from "@skyla/ui";
import type { ReactNode } from "react";
import { moment } from "../format";

/** The badge each advisor row shows for the connection. */
export function connectionBadge(s: AdvisorStatusDto | undefined): { tone: Tone; label: string } {
  if (!s) return { tone: "neutral", label: "Checking…" };
  if (s.demo) return { tone: "neutral", label: "Demo · recorded runs" };
  switch (s.state) {
    case "ready":
      return { tone: "positive", label: "Ready" };
    case "not_installed":
      return { tone: "warning", label: "Claude Code not found" };
    case "not_signed_in":
      return { tone: "warning", label: "Not signed in" };
    case "rate_limited":
      return { tone: "warning", label: "Usage limit reached" };
    default:
      return { tone: "neutral", label: s.state };
  }
}

function Command({ children }: { children: string }) {
  return (
    <code className="rounded-[4px] bg-fill px-1 font-sans font-medium text-ink">{children}</code>
  );
}

/**
 * What the connection to the user's Claude Code installation is, and what
 * to do when advisors can't run. sky-la never handles the sign-in itself.
 */
export function AdvisorConnection({
  status,
  onCheckAgain,
}: {
  status: AdvisorStatusDto;
  onCheckAgain?: () => void;
}) {
  const badge = connectionBadge(status);
  const recheck = onCheckAgain && (
    <Button size="small" onPress={onCheckAgain}>
      Check again
    </Button>
  );
  let body: ReactNode;
  if (status.demo) {
    body = <p>This demo answers from recorded runs, so no model is called.</p>;
  } else if (status.state === "ready") {
    body = (
      <p>
        Using Claude Code {status.version}, signed in from your terminal. sky-la never sees your
        Claude credentials.
      </p>
    );
  } else if (status.state === "not_installed") {
    body = (
      <>
        <p>
          Advisors run on Claude Code, and it isn't installed where sky-la looked. Install it, then
          sign in from a terminal with <Command>claude auth login</Command>. The books work as usual
          without it.
        </p>
        {status.lookedIn.length > 0 && (
          <details className="mt-1">
            <summary className="cursor-default text-ink-secondary">Where sky-la looked</summary>
            <ul className="mt-1 list-disc pl-5 text-ink-secondary">
              {status.lookedIn.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </details>
        )}
      </>
    );
  } else if (status.state === "not_signed_in") {
    body = (
      <p>
        Claude Code is installed but not signed in. Open a terminal and run{" "}
        <Command>claude auth login</Command>, then check again. sky-la never asks for or stores your
        sign-in.
      </p>
    );
  } else if (status.state === "rate_limited") {
    body = (
      <p>
        Your Claude usage limit is reached
        {status.resetsAt ? `; it resets on ${moment(status.resetsAt)}` : ""}. Advisors pause until
        then; the books work as usual.
      </p>
    );
  } else {
    body = <p>Advisors can't run right now ({status.state}).</p>;
  }
  return (
    <section
      aria-label="Advisor connection"
      className="flex flex-col gap-2 px-5 py-4 text-body"
      data-state={status.state}
    >
      <div className="flex items-center justify-between gap-2">
        <Badge tone={badge.tone}>{badge.label}</Badge>
        {!status.demo && status.state !== "ready" && recheck}
      </div>
      {body}
    </section>
  );
}
