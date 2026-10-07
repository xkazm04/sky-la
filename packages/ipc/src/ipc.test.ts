import { describe, expect, it } from "vitest";
import { createMockTransport, IpcError, isTauriRuntime, selectTransport } from "./index";

describe("mock transport", () => {
  it("answers app_info from the default handlers", async () => {
    const info = await createMockTransport().invoke("app_info");
    expect(info).toEqual({ name: "sky-la", version: "0.1.0", transport: "mock" });
  });

  it("rejects a command that has no handler with an IpcError naming the command", async () => {
    const transport = createMockTransport({});
    await expect(transport.invoke("app_info")).rejects.toBeInstanceOf(IpcError);
    await expect(transport.invoke("app_info")).rejects.toThrow(/^app_info: /);
  });

  it("uses custom handlers when given", async () => {
    const transport = createMockTransport({
      app_info: () => ({ name: "sky-la", version: "9.9.9", transport: "mock" }),
    });
    expect((await transport.invoke("app_info")).version).toBe("9.9.9");
  });
});

describe("selectTransport", () => {
  it("falls back to the mock outside the Tauri webview", () => {
    expect(isTauriRuntime()).toBe(false);
    expect(selectTransport().kind).toBe("mock");
  });

  it("honours forceMock", () => {
    expect(selectTransport({ forceMock: true }).kind).toBe("mock");
  });
});
