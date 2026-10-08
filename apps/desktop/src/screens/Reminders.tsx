import { commands, type DunningNoticeDto, type InvoiceDto, unwrap } from "@skyla/ipc";
import { Button, FactList, InspectorSection, SegmentedControl } from "@skyla/ui";
import { useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { day, money } from "../format";

const TONE: Record<string, string> = {
  friendly: "friendly",
  firm: "firm",
  final: "final notice",
};

/**
 * An overdue invoice's payment reminder: the core drafts it with the pack's
 * late interest; the user sends it from their own mail and marks it sent.
 */
export function ReminderSection({ invoice }: { invoice: InvoiceDto }) {
  const periods = useQuery("reporting_periods", () => unwrap(commands.reportingPeriods()));
  const today = periods.state === "ready" ? periods.data.today : null;
  const queue = useQuery(`dunning_queue:${today}`, () =>
    today ? unwrap(commands.dunningQueue(today)) : Promise.resolve([] as DunningNoticeDto[]),
  );
  const sent = useQuery(`reminders_sent:${invoice.id}`, () =>
    unwrap(commands.remindersSent(invoice.id)),
  );
  const notice =
    queue.state === "ready" ? queue.data.find((n) => n.documentId === invoice.id) : undefined;
  const history = sent.state === "ready" ? sent.data : [];
  if (!notice && history.length === 0) return null;
  return (
    <InspectorSection title="Payment reminder">
      {notice && <Notice notice={notice} />}
      {history.length > 0 && (
        <FactList
          facts={history.map((h) => ({
            label: `Reminder ${h.step}`,
            value: `sent ${day(h.sentOn, true)}`,
          }))}
        />
      )}
    </InspectorSection>
  );
}

function Notice({ notice }: { notice: DunningNoticeDto }) {
  const [lang, setLang] = useState<"cs" | "en">("cs");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const subject = lang === "cs" ? notice.subjectCs : notice.subjectEn;
  const body = lang === "cs" ? notice.bodyCs : notice.bodyEn;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(`${subject}\n\n${body}`);
      setResult({ ok: true, text: "Copied: paste it into an email to the customer." });
    } catch {
      setResult({ ok: false, text: "Couldn't copy; select the text instead." });
    }
  };
  const markSent = async () => {
    setBusy(true);
    try {
      await unwrap(commands.recordReminder(notice.documentId, notice.step));
      invalidateAll();
    } catch (e) {
      setResult({ ok: false, text: problems(e).join(" ") });
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <p className="text-body">
        Reminder {notice.step} ({TONE[notice.tone] ?? notice.tone}) is due since{" "}
        {day(notice.scheduledOn, true)}, {notice.daysOverdue} days after the due date.
        {notice.interest && <> Late interest so far: {money(notice.interest.total)}.</>}
      </p>
      {notice.interestProblem && (
        <p className="text-footnote text-ink-secondary">{notice.interestProblem}</p>
      )}
      <SegmentedControl
        label="Language"
        segments={[
          { id: "cs", label: "Czech" },
          { id: "en", label: "English" },
        ]}
        value={lang}
        onChange={setLang}
      />
      <section
        aria-label="Reminder text"
        className="rounded-inner bg-surface p-3 text-body shadow-group select-text"
      >
        <p className="font-semibold">{subject}</p>
        <p className="mt-2 whitespace-pre-wrap">{body}</p>
      </section>
      <div className="flex justify-end gap-2">
        <Button variant="plain" size="small" onPress={() => void copy()}>
          Copy text
        </Button>
        <Button variant="primary" size="small" isDisabled={busy} onPress={() => void markSent()}>
          Mark as sent
        </Button>
      </div>
      {result && (
        <p
          role={result.ok ? "status" : "alert"}
          className={`text-footnote ${result.ok ? "text-ink-secondary" : "text-negative-ink"}`}
        >
          {result.text}
        </p>
      )}
      <p className="text-footnote text-ink-secondary">
        sky-la doesn't send email. Send it from your own mail, then mark it as sent; the next,
        firmer reminder follows the dunning policy.
      </p>
    </div>
  );
}
