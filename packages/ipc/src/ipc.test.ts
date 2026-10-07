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
    connectCore({ forceMock: true });
    expect(all.length).toBeGreaterThan(20);
    for (const r of all) {
      const call = invoke(r.command, r.args as Record<string, unknown>);
      if (r.isError) await expect(call).rejects.toEqual(r.result);
      else await expect(call).resolves.toEqual(r.result);
    }
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
