import type { ReactNode } from "react";
import { cx } from "../cx";

export interface AppWindowProps {
  /** Left pane content (the source list and entity switcher). */
  sidebar: ReactNode;
  /** Right pane, usually an `Inspector`. */
  inspector?: ReactNode;
  /** The status line under the content. */
  status?: ReactNode;
  children: ReactNode;
  className?: string;
}

/**
 * Direction A's window: a backdrop with three panes. The side panes float as
 * glass panels inset 8 px from the edge; the content sits between them.
 */
export function AppWindow({ sidebar, inspector, status, children, className }: AppWindowProps) {
  return (
    <div className={cx("flex h-full min-h-0 gap-2 bg-backdrop p-2", className)}>
      <div className="glass-panel flex w-[232px] shrink-0 flex-col rounded-panel px-2.5 py-3">
        {sidebar}
      </div>
      <div className="flex min-w-0 flex-1 flex-col">
        <main className="flex min-h-0 flex-1 flex-col">{children}</main>
        {status}
      </div>
      {inspector && <div className="w-[340px] shrink-0">{inspector}</div>}
    </div>
  );
}

export interface ToolbarProps {
  title: ReactNode;
  /** Key figures, e.g. "6 open · 214 500,00 Kč due". */
  subtitle?: ReactNode;
  /** Glass capsules: filter, search, icon group, then one primary action. */
  children?: ReactNode;
}

/** No solid bar: the title and subtitle at the top-left, capsules on the right. */
export function Toolbar({ title, subtitle, children }: ToolbarProps) {
  return (
    <header className="flex min-h-14 shrink-0 items-center gap-3 px-2 pt-1 pb-3">
      {/* Key figures keep their room; the capsules give way first. */}
      <div className="min-w-48 flex-1">
        <h1 className="truncate font-semibold text-headline">{title}</h1>
        {subtitle && <p className="truncate text-footnote text-ink-secondary">{subtitle}</p>}
      </div>
      {children && <div className="flex min-w-0 shrink items-center gap-2">{children}</div>}
    </header>
  );
}

/** The one white list or table group per view. */
export function ContentGroup({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cx(
        "min-h-0 flex-1 overflow-auto rounded-group bg-surface shadow-group",
        className,
      )}
    >
      {children}
    </div>
  );
}
