import recordings from "@skyla/fixtures/ipc-recordings.json";
import { invoke } from "@tauri-apps/api/core";
import { clearMocks } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { commands, connectCore, IpcError, isTauriRuntime, type Recording, unwrap } from "./index";

const all = recordings as readonly Recording[];

afterEach(() => clearMocks());

describe("connectCore", () => {
  it("replays the recordings outside the Tauri webview", () => {
    expect(isTauriRuntime()).toBe(false);
    expect(connectCore()).toBe("mock");
  });
});

// WP-09 acceptance (the webview half): the mock answers every recorded
// request exactly as recorded. The Rust half (apps/desktop/src-tauri/tests/ipc.rs)
// replays the same recordings over the real Tauri IPC, so both transports
// return identical answers.
describe("the mock transport", () => {
  it("answers every recording identically through Tauri's invoke", async () => {
    expect(all.length).toBeGreaterThan(20);
    // The initial answers in one session, then each scenario in its own,
    // in the order it was recorded.
    const groups = [
      all.filter((r) => r.scenario === undefined),
      ...[...new Set(all.flatMap((r) => (r.scenario ? [r.scenario] : [])))].map((name) =>
        all.filter((r) => r.scenario === name),
      ),
    ];
    expect(groups.length).toBeGreaterThan(1);
    for (const group of groups) {
      clearMocks();
      connectCore({ forceMock: true });
      for (const r of group) {
        const call = invoke(r.command, r.args as Record<string, unknown>);
        if (r.isError) await expect(call).rejects.toEqual(r.result);
        else await expect(call).resolves.toEqual(r.result);
      }
    }
  });

  it("moves through a scenario's states and falls back to the initial answers", async () => {
    const scripted: Recording[] = [
      { command: "count", args: {}, result: 1, isError: false },
      { command: "other", args: {}, result: "untouched", isError: false },
      { command: "add", args: {}, result: null, isError: false, scenario: "s", leadsTo: "s/added" },
      { command: "count", args: {}, result: 2, isError: false, scenario: "s", state: "s/added" },
    ];
    connectCore({ forceMock: true, recordings: scripted });
    expect(await invoke("count")).toBe(1);
    await invoke("add");
    expect(await invoke("count")).toBe(2);
    expect(await invoke("other")).toBe("untouched");
    // The write was recorded from the initial state only.
    await expect(invoke("add")).rejects.toMatchObject({ code: "not_recorded" });
    // A new session starts over.
    clearMocks();
    connectCore({ forceMock: true, recordings: scripted });
    expect(await invoke("count")).toBe(1);
  });

  it("serves the generated, typed commands", async () => {
    connectCore({ forceMock: true });
    const q3 = await unwrap(commands.profitAndLoss("2026-07-01", "2026-09-30"));
    expect(q3.profit).toEqual({ minor: 28_035_000, currency: "CZK" });
    expect(q3.snapshot.hash).toMatch(/^[0-9a-f]{64}$/);
    const info = await commands.appInfo();
    expect(info.name).toBe("sky-la");
    const tb = await unwrap(commands.trialBalance(null, "2026-09-30"));
    expect(tb.totalDebit).toEqual(tb.totalCredit);
  });

  it("refuses a request that wasn't recorded, as a typed failure", async () => {
    connectCore({ forceMock: true });
    const result = await commands.profitAndLoss("2026-01-01", "2026-01-31");
    expect(result.status).toBe("error");
    if (result.status === "error") expect(result.error.code).toBe("not_recorded");
    await expect(unwrap(commands.profitAndLoss("2026-01-01", "2026-01-31"))).rejects.toBeInstanceOf(
      IpcError,
    );
  });
});
