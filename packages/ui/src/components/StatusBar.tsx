import type { LucideIcon } from "lucide-react";
import type { HTMLAttributes } from "react";
import { cx } from "../cx";
import type { Tone } from "./Badge";

export interface StatusItem {
  id: string;
  icon: LucideIcon;
  label: string;
  tone?: Tone;
}

export interface StatusBarProps extends HTMLAttributes<HTMLElement> {
  items: ReadonlyArray<StatusItem>;
}

const iconTone: Record<Tone, string> = {
  positive: "text-positive-ink",
  negative: "text-negative-ink",
  warning: "text-warning-ink",
  info: "text-info-ink",
  neutral: "text-ink-secondary",
  accent: "text-accent-ink",
};

/** The 30 px status line: encryption, journal integrity, egress summary. */
export function StatusBar({ items, className, ...props }: StatusBarProps) {
  return (
    <section
      aria-label="Status"
      {...props}
      className={cx(
        "flex h-[30px] shrink-0 items-center gap-4 px-2 text-footnote text-ink-secondary",
        className,
      )}
    >
      {items.map(({ id, icon: Icon, label, tone = "neutral" }) => (
        <span key={id} className="inline-flex items-center gap-1.5 whitespace-nowrap">
          <Icon aria-hidden size={13} strokeWidth={2} className={iconTone[tone]} />
          {label}
        </span>
      ))}
    </section>
  );
}
