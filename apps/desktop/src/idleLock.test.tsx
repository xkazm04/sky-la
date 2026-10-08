// @vitest-environment happy-dom
import { cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const lock = vi.fn(async () => ({ status: "ok", data: {} }));
vi.mock("@skyla/ipc", () => ({ commands: { lock: () => lock() } }));
vi.mock("./data", () => ({ invalidateAll: vi.fn() }));
const navigate = vi.fn();
vi.mock("./router", () => ({ navigate: (...a: unknown[]) => navigate(...a) }));

const { IDLE_LOCK_MINUTES, useIdleLock } = await import("./idleLock");
const MINUTE = 60_000;

beforeEach(() => {
  vi.useFakeTimers();
  lock.mockClear();
  navigate.mockClear();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useIdleLock", () => {
  it("locks after the idle time and goes to the unlock screen", async () => {
    renderHook(() => useIdleLock(true));
    await vi.advanceTimersByTimeAsync((IDLE_LOCK_MINUTES - 1) * MINUTE);
    expect(lock).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(MINUTE);
    expect(lock).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenCalledWith("unlock", null, true);
  });

  it("starts over on every key press", async () => {
    renderHook(() => useIdleLock(true));
    await vi.advanceTimersByTimeAsync((IDLE_LOCK_MINUTES - 1) * MINUTE);
    globalThis.dispatchEvent(new KeyboardEvent("keydown", { key: "a" }));
    await vi.advanceTimersByTimeAsync((IDLE_LOCK_MINUTES - 1) * MINUTE);
    expect(lock).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(MINUTE);
    expect(lock).toHaveBeenCalledTimes(1);
  });

  it("never locks the demo or locked books", async () => {
    renderHook(() => useIdleLock(false));
    await vi.advanceTimersByTimeAsync(10 * IDLE_LOCK_MINUTES * MINUTE);
    expect(lock).not.toHaveBeenCalled();
  });
});
