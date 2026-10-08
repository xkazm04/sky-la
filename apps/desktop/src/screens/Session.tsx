import {
  commands,
  type EntitySetupDto,
  type RecoveryKeyDto,
  type SessionStateDto,
  unwrap,
} from "@skyla/ipc";
import { Button, Checkbox, Select, TextField } from "@skyla/ui";
import { KeyRound, LockKeyhole, Printer } from "lucide-react";
import { type FormEvent, type ReactNode, useState } from "react";
import { invalidateAll, problems, useQuery } from "../data";
import { navigate } from "../router";

/** A centred sheet over the window background, for the steps before the books open. */
function Sheet({
  title,
  intro,
  children,
}: {
  title: string;
  intro: ReactNode;
  children: ReactNode;
}) {
  return (
    <main className="flex min-h-dvh items-center justify-center bg-backdrop p-6">
      <section
        aria-label={title}
        className="glass-panel w-full max-w-xl rounded-panel p-6 shadow-group"
      >
        <h1 className="font-semibold text-title">{title}</h1>
        <div className="mt-1 text-body text-ink-secondary">{intro}</div>
        <div className="mt-5">{children}</div>
      </section>
    </main>
  );
}

function Problems({ list }: { list: string[] }) {
  if (list.length === 0) return null;
  return (
    <div
      role="alert"
      className="rounded-inner bg-negative-tint px-3 py-2 text-body text-negative-ink"
    >
      <ul className="list-disc pl-5">
        {list.map((p) => (
          <li key={p}>{p}</li>
        ))}
      </ul>
    </div>
  );
}

/** Shows the recovery key once, then asks for its last group back. */
function RecoveryKey({ shown, onDone }: { shown: RecoveryKeyDto; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  const [typed, setTyped] = useState("");
  const [errors, setErrors] = useState<string[]>([]);
  const confirm = async (e: FormEvent) => {
    e.preventDefault();
    try {
      const ok = await unwrap(commands.confirmRecoveryKey(typed));
      if (ok) onDone();
      else setErrors(["That isn't the last group. Check what you wrote down."]);
    } catch (err) {
      setErrors(problems(err));
    }
  };
  return (
    <div className="flex flex-col gap-4">
      <p className="text-body">
        If you forget your passphrase, this key is the only way back into your books. Write it down
        or print it and keep it somewhere safe, away from this computer. sky-la shows it only now.
      </p>
      <div>
        <p className="mb-1 font-medium text-footnote text-ink-secondary">Recovery key</p>
        <p
          data-testid="recovery-key"
          className="select-all rounded-inner bg-surface px-4 py-3 text-center font-mono font-semibold text-headline tracking-wide shadow-group"
        >
          {shown.key}
        </p>
      </div>
      {!saved ? (
        <div className="flex justify-end gap-2">
          <Button icon={Printer} onPress={() => globalThis.print()}>
            Print
          </Button>
          <Button variant="primary" onPress={() => setSaved(true)}>
            I've saved it
          </Button>
        </div>
      ) : (
        <form
          aria-label="Confirm the recovery key"
          className="flex flex-col gap-3"
          onSubmit={confirm}
        >
          <TextField
            label={`Type the last group (group ${shown.groups})`}
            value={typed}
            onChange={setTyped}
            autoComplete="off"
          />
          <Problems list={errors} />
          <div className="flex justify-end">
            <Button variant="primary" type="submit">
              Confirm
            </Button>
          </div>
        </form>
      )}
    </div>
  );
}

const VAT = [
  { id: "monthly", label: "VAT payer, monthly" },
  { id: "quarterly", label: "VAT payer, quarterly" },
  { id: "none", label: "Not a VAT payer" },
] as const;

const GROUPS = [
  { id: "trade", label: "Other trade (60 %)" },
  { id: "craft", label: "Craft trade (80 %)" },
  { id: "liberal", label: "Other self-employment (40 %)" },
] as const;

const BLANK: EntitySetupDto = {
  displayName: "",
  ico: "",
  dic: "",
  address: "",
  vatPeriod: "monthly",
  registration: "",
  iban: "",
  bankName: "",
  email: null,
  flatRateGroup: "trade",
};

/** First run: who the books are for, a passphrase, and the recovery key. */
export function SetupScreen() {
  const [step, setStep] = useState<"profile" | "passphrase" | "key" | "done">("profile");
  const [setup, setSetup] = useState<EntitySetupDto>(BLANK);
  const [pass, setPass] = useState("");
  const [again, setAgain] = useState("");
  const [shown, setShown] = useState<RecoveryKeyDto | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const field = (k: keyof EntitySetupDto) => (v: string) => setSetup((s) => ({ ...s, [k]: v }));

  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (pass !== again) {
      setErrors(["The two passphrases differ."]);
      return;
    }
    try {
      setShown(await unwrap(commands.createEntity(setup, pass)));
      setErrors([]);
      setStep("key");
    } catch (err) {
      setErrors(problems(err));
      // The form's own problems send the user back to it.
      if (problems(err).some((p) => !p.includes("passphrase"))) setStep("profile");
    }
  };

  if (step === "key" && shown) {
    return (
      <Sheet title="Your recovery key" intro="One more step before your books open.">
        <RecoveryKey shown={shown} onDone={() => setStep("done")} />
      </Sheet>
    );
  }
  if (step === "done") {
    return (
      <Sheet title="Your books are ready" intro={`Set up for ${setup.displayName}.`}>
        <p className="text-body">
          They're encrypted on this computer. sky-la backs them up every day to the Backups folder
          beside them; Settings shows where.
        </p>
        <div className="mt-4 flex justify-end">
          <Button
            variant="primary"
            onPress={() => {
              invalidateAll();
              navigate("overview");
            }}
          >
            Open the books
          </Button>
        </div>
      </Sheet>
    );
  }
  if (step === "passphrase") {
    return (
      <Sheet
        title="Choose a passphrase"
        intro="It encrypts your books on this computer. Use a few unrelated words; at least 10 characters."
      >
        <form aria-label="Passphrase" className="flex flex-col gap-3" onSubmit={create}>
          <TextField
            label="Passphrase"
            type="password"
            value={pass}
            onChange={setPass}
            autoComplete="new-password"
          />
          <TextField
            label="The same again"
            type="password"
            value={again}
            onChange={setAgain}
            autoComplete="new-password"
          />
          <Problems list={errors} />
          <div className="flex justify-between gap-2">
            <Button variant="plain" onPress={() => setStep("profile")}>
              Back
            </Button>
            <Button variant="primary" type="submit">
              Create the books
            </Button>
          </div>
        </form>
      </Sheet>
    );
  }
  return (
    <Sheet
      title="Set up your books"
      intro="Who they're for, as printed on your invoices. You can change these later."
    >
      <form
        aria-label="Who the books are for"
        className="grid grid-cols-2 gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          setStep("passphrase");
        }}
      >
        <TextField
          className="col-span-2"
          label="Name"
          value={setup.displayName}
          onChange={field("displayName")}
        />
        <TextField label="IČO" value={setup.ico} onChange={field("ico")} />
        <TextField label="DIČ" value={setup.dic ?? ""} onChange={field("dic")} />
        <TextField
          className="col-span-2"
          label="Address"
          multiline
          value={setup.address}
          onChange={field("address")}
        />
        <Select label="VAT" options={VAT} value={setup.vatPeriod} onChange={field("vatPeriod")} />
        <Select
          label="Flat-rate group"
          options={GROUPS}
          value={setup.flatRateGroup ?? null}
          onChange={field("flatRateGroup")}
        />
        <TextField
          className="col-span-2"
          label="Trade register line"
          value={setup.registration}
          onChange={field("registration")}
        />
        <TextField
          label="Business account IBAN"
          value={setup.iban ?? ""}
          onChange={field("iban")}
        />
        <TextField label="Bank" value={setup.bankName} onChange={field("bankName")} />
        <div className="col-span-2">
          <Problems list={errors} />
        </div>
        <div className="col-span-2 flex justify-between gap-2">
          <Button
            variant="plain"
            onPress={() =>
              void unwrap(commands.openDemo()).then(() => {
                invalidateAll();
                navigate("overview");
              })
            }
          >
            Explore the demo instead
          </Button>
          <Button variant="primary" type="submit">
            Next
          </Button>
        </div>
      </form>
    </Sheet>
  );
}

/** Unlocking: the passphrase, optionally remembered in the OS keychain. */
export function UnlockScreen() {
  const session = useQuery("session_state", () => commands.sessionState());
  const who = session.state === "ready" ? session.data.entity : null;
  const [pass, setPass] = useState("");
  // Unticked means "forget it", so start from what's remembered now.
  const [remember, setRemember] = useState<boolean | null>(null);
  const remembered = session.state === "ready" && session.data.remembered;
  const [errors, setErrors] = useState<string[]>([]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    try {
      await unwrap(commands.unlock(pass, remember ?? remembered));
      invalidateAll();
      navigate("overview");
    } catch (err) {
      setErrors(problems(err));
    }
  };
  return (
    <Sheet
      title={who ? `Unlock ${who}'s books` : "Unlock your books"}
      intro="Your books are encrypted on this computer."
    >
      <form aria-label="Unlock" className="flex flex-col gap-3" onSubmit={submit}>
        <TextField
          label="Passphrase"
          type="password"
          value={pass}
          onChange={setPass}
          autoComplete="current-password"
        />
        <Checkbox isSelected={remember ?? remembered} onChange={setRemember}>
          Remember on this computer (in the system keychain)
        </Checkbox>
        <Problems list={errors} />
        <div className="flex justify-between gap-2">
          <Button variant="plain" icon={KeyRound} onPress={() => navigate("recover")}>
            Forgot the passphrase?
          </Button>
          <Button variant="primary" icon={LockKeyhole} type="submit">
            Unlock
          </Button>
        </div>
      </form>
    </Sheet>
  );
}

/** The recovery key opens the books, a new passphrase replaces the old, and a new key is shown. */
export function RecoverScreen() {
  const [key, setKey] = useState("");
  const [pass, setPass] = useState("");
  const [again, setAgain] = useState("");
  const [shown, setShown] = useState<RecoveryKeyDto | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pass !== again) {
      setErrors(["The two passphrases differ."]);
      return;
    }
    try {
      setShown(await unwrap(commands.recover(key, pass)));
      setErrors([]);
    } catch (err) {
      setErrors(problems(err));
    }
  };
  if (shown) {
    return (
      <Sheet title="Your new recovery key" intro="The old one has been used and no longer works.">
        <RecoveryKey
          shown={shown}
          onDone={() => {
            invalidateAll();
            navigate("overview");
          }}
        />
      </Sheet>
    );
  }
  return (
    <Sheet
      title="Recover your books"
      intro="Enter the recovery key you saved when you set them up, and choose a new passphrase."
    >
      <form aria-label="Recover" className="flex flex-col gap-3" onSubmit={submit}>
        <TextField label="Recovery key" value={key} onChange={setKey} autoComplete="off" />
        <TextField
          label="New passphrase"
          type="password"
          value={pass}
          onChange={setPass}
          autoComplete="new-password"
        />
        <TextField
          label="The same again"
          type="password"
          value={again}
          onChange={setAgain}
          autoComplete="new-password"
        />
        <Problems list={errors} />
        <div className="flex justify-between gap-2">
          <Button variant="plain" onPress={() => navigate("unlock")}>
            Back
          </Button>
          <Button variant="primary" type="submit">
            Recover
          </Button>
        </div>
      </form>
    </Sheet>
  );
}

/** Where a fresh start goes in the desktop app: set up, unlock, or the books. */
export function sessionRoute(state: SessionStateDto): "setup" | "unlock" | null {
  if (state.state === "needs_setup") return "setup";
  if (state.state === "locked") return "unlock";
  return null;
}
