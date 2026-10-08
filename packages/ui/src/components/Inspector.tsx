import type { ReactNode } from "react";
import { cx } from "../cx";

export interface InspectorProps {
  /** Landmark name, e.g. "Invoice 2026-114". */
  label: string;
  title: ReactNode;
  subtitle?: ReactNode;
  /** Trailing header content, e.g. a status badge. */
  accessory?: ReactNode;
  children: ReactNode;
  /** Pinned to the bottom. */
  actions?: ReactNode;
  className?: string;
}

/** The right pane: the selected object, its details and its actions. */
export function Inspector({
  label,
  title,
  subtitle,
  accessory,
  children,
  actions,
  className,
}: InspectorProps) {
  return (
    <aside
      aria-label={label}
      className={cx("glass-panel flex h-full min-h-0 flex-col rounded-panel", className)}
    >
      <header className="flex items-start gap-3 px-4 pt-4 pb-3">
        <div className="min-w-0 flex-1">
          <h2 className="line-clamp-2 font-semibold text-title">{title}</h2>
          {subtitle && (
            <p className="mt-0.5 line-clamp-2 text-footnote text-ink-secondary">{subtitle}</p>
          )}
        </div>
        {accessory}
      </header>
      {/* Focusable so a keyboard can scroll a long inspector (WCAG 2.1.1). */}
      <section
        aria-label={`${label} details`}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must take focus
        tabIndex={0}
        className="min-h-0 flex-1 overflow-y-auto px-4 pb-4 outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--sk-focus)]"
      >
        {children}
      </section>
      {actions && (
        <footer className="flex items-center justify-end gap-2 border-hairline-strong border-t px-4 py-3">
          {actions}
        </footer>
      )}
    </aside>
  );
}

export interface InspectorSectionProps {
  title: string;
  /** Trailing header content, e.g. a badge. */
  accessory?: ReactNode;
  children: ReactNode;
  className?: string;
}

/** A titled group inside the inspector. */
export function InspectorSection({ title, accessory, children, className }: InspectorSectionProps) {
  return (
    <section className={cx("mt-4 first:mt-0", className)}>
      {accessory ? (
        <div className="mb-1.5 flex items-center justify-between gap-2">
          <h3 className="font-semibold text-caption text-ink-secondary">{title}</h3>
          {accessory}
        </div>
      ) : (
        <h3 className="mb-1.5 font-semibold text-caption text-ink-secondary">{title}</h3>
      )}
      {children}
    </section>
  );
}

export interface FactListProps {
  facts: ReadonlyArray<{ label: string; value: ReactNode }>;
}

/** Label/value pairs, values end-aligned with tabular figures. */
export function FactList({ facts }: FactListProps) {
  return (
    <dl className="overflow-hidden rounded-inner bg-surface shadow-group">
      {facts.map(({ label, value }) => (
        <div
          key={label}
          className="flex h-8 items-center gap-3 border-hairline border-t px-3 first:border-t-0"
        >
          <dt className="flex-1 text-ink-secondary">{label}</dt>
          <dd className="m-0 text-right">{value}</dd>
        </div>
      ))}
    </dl>
  );
}
