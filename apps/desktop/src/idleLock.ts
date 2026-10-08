import { commands } from "@skyla/ipc";
import { useEffect } from "react";
import { invalidateAll } from "./data";
import { navigate } from "./router";

/** Real books lock after this long without a key press or pointer move. */
export const IDLE_LOCK_MINUTES = 15;

/** Locks open real books after a while idle, then asks for the passphrase. */
export function useIdleLock(active: boolean) {
  useEffect(() => {
    if (!active) return;
    const lock = async () => {
      await commands.lock();
      invalidateAll();
      navigate("unlock", null, true);
    };
    let timer = setTimeout(() => void lock(), IDLE_LOCK_MINUTES * 60_000);
    const reset = () => {
      clearTimeout(timer);
      timer = setTimeout(() => void lock(), IDLE_LOCK_MINUTES * 60_000);
    };
    const events = ["pointerdown", "pointermove", "keydown", "wheel"] as const;
    for (const e of events) globalThis.addEventListener(e, reset, { passive: true });
    return () => {
      clearTimeout(timer);
      for (const e of events) globalThis.removeEventListener(e, reset);
    };
  }, [active]);
}
