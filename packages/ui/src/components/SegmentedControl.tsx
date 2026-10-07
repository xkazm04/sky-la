import type { LucideIcon } from "lucide-react";
import { ToggleButton, ToggleButtonGroup } from "react-aria-components";
import { cx } from "../cx";

export interface Segment<K extends string> {
  id: K;
  label: string;
  icon?: LucideIcon;
}

export interface SegmentedControlProps<K extends string> {
  /** Accessible name, e.g. "Filter invoices". */
  label: string;
  segments: ReadonlyArray<Segment<K>>;
  value: K;
  onChange: (value: K) => void;
  className?: string;
}

/**
 * A glass capsule of mutually exclusive segments (a toolbar filter, an
 * accrual/cash switch). Arrow keys move between segments.
 */
export function SegmentedControl<K extends string>({
  label,
  segments,
  value,
  onChange,
  className,
}: SegmentedControlProps<K>) {
  return (
    <ToggleButtonGroup
      aria-label={label}
      selectionMode="single"
      disallowEmptySelection
      selectedKeys={[value]}
      onSelectionChange={(keys) => {
        const [next] = [...keys];
        if (next !== undefined) onChange(String(next) as K);
      }}
      className={cx(
        "glass-capsule inline-flex h-7 shrink-0 items-center gap-0.5 rounded-full p-0.5",
        className,
      )}
    >
      {segments.map(({ id, label: text, icon: Icon }) => (
        <ToggleButton
          key={id}
          id={id}
          className="focus-ring inline-flex h-6 cursor-default items-center gap-1.5 rounded-full px-3 font-medium text-ink-secondary text-subhead transition-colors duration-150 select-none data-hovered:text-ink data-selected:bg-segment-selected data-selected:text-ink data-selected:shadow-raised"
        >
          {Icon && <Icon aria-hidden size={13} strokeWidth={2} />}
          {text}
        </ToggleButton>
      ))}
    </ToggleButtonGroup>
  );
}
