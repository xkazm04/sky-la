import {
  CircleAlert,
  CircleCheck,
  CircleDot,
  Info,
  type LucideIcon,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import type { ReactNode } from "react";
import { cx } from "../cx";

/** Semantic tone. Status is always icon + label, never colour alone. */
export type Tone = "positive" | "negative" | "warning" | "info" | "neutral" | "accent";

export const toneIcon: Record<Tone, LucideIcon> = {
  positive: CircleCheck,
  negative: CircleAlert,
  warning: TriangleAlert,
  info: Info,
  neutral: CircleDot,
  accent: Sparkles,
};

export const toneClasses: Record<Tone, string> = {
  positive: "bg-positive-tint text-positive-ink",
  negative: "bg-negative-tint text-negative-ink",
  warning: "bg-warning-tint text-warning-ink",
  info: "bg-info-tint text-info-ink",
  neutral: "bg-neutral-tint text-neutral-ink",
  accent: "bg-accent-tint text-accent-ink",
};

export interface BadgeProps {
  tone?: Tone;
  /** Overrides the tone's default icon. */
  icon?: LucideIcon;
  /** The label. Required: a badge is never colour or icon alone. */
  children: ReactNode;
  className?: string;
}

/** A status pill: icon + label on a matched-lightness tint. */
export function Badge({ tone = "neutral", icon, children, className }: BadgeProps) {
  const Icon = icon ?? toneIcon[tone];
  return (
    <span
      className={cx(
        "inline-flex h-5 shrink-0 items-center gap-1 rounded-full pr-2 pl-1.5 font-medium text-footnote whitespace-nowrap",
        toneClasses[tone],
        className,
      )}
    >
      <Icon aria-hidden size={12} strokeWidth={2.25} />
      {children}
    </span>
  );
}
