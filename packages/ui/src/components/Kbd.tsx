import { cx } from "../cx";

const MAC: Record<string, string> = {
  mod: "⌘",
  cmd: "⌘",
  shift: "⇧",
  alt: "⌥",
  ctrl: "⌃",
  enter: "↩",
  esc: "esc",
  backspace: "⌫",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
};

const OTHER: Record<string, string> = {
  ...MAC,
  mod: "Ctrl",
  cmd: "Ctrl",
  shift: "Shift",
  alt: "Alt",
  ctrl: "Ctrl",
  enter: "Enter",
  esc: "Esc",
  backspace: "Backspace",
};

const NAMES: Record<string, string> = {
  mod: "Command",
  cmd: "Command",
  shift: "Shift",
  alt: "Option",
  ctrl: "Control",
  enter: "Return",
  esc: "Escape",
};

/** True on macOS, where shortcuts use ⌘ and symbol glyphs. */
export function isMac(): boolean {
  const nav = globalThis.navigator as
    | (Navigator & { userAgentData?: { platform?: string } })
    | undefined;
  const platform = nav?.userAgentData?.platform ?? nav?.platform ?? "";
  return /mac/i.test(platform);
}

export interface KbdProps {
  /** Keys, e.g. `["mod", "k"]` or `"mod+k"`. `mod` is ⌘ on macOS and Ctrl elsewhere. */
  keys: string | string[];
  /** Force a platform; defaults to the running one. */
  mac?: boolean;
  className?: string;
}

/** A keyboard shortcut hint. */
export function Kbd({ keys, mac = isMac(), className }: KbdProps) {
  const list = Array.isArray(keys) ? keys : keys.split("+");
  const table = mac ? MAC : OTHER;
  const label = list
    .map((k) => (mac ? (NAMES[k] ?? k.toUpperCase()) : (table[k] ?? k.toUpperCase())))
    .join(mac ? " " : "+");
  return (
    <kbd
      aria-label={label}
      className={cx(
        "inline-flex h-[18px] min-w-[18px] items-center justify-center gap-0.5 rounded-key bg-fill px-1 font-medium font-sans text-caption text-ink-secondary shadow-[inset_0_0_0_0.5px_var(--sk-hairline-strong)]",
        className,
      )}
    >
      {list.map((k) => (
        <span key={k}>{table[k] ?? k.toUpperCase()}</span>
      ))}
    </kbd>
  );
}
