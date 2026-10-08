import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";
import {
  Header,
  ListBox,
  ListBoxItem,
  ListBoxSection,
  type Selection,
} from "react-aria-components";
import { cx } from "../cx";

export interface SourceItem {
  id: string;
  label: string;
  icon: LucideIcon;
  /** Trailing count, e.g. open items. */
  count?: number;
  /** Trailing badge instead of a count. */
  trailing?: ReactNode;
}

export interface SourceSection {
  id: string;
  /** Section label; the first section usually has none. */
  title?: string;
  items: ReadonlyArray<SourceItem>;
}

export interface SourceListProps {
  /** Accessible name. */
  label: string;
  sections: ReadonlyArray<SourceSection>;
  selectedId: string;
  onSelect: (id: string) => void;
  className?: string;
}

/**
 * The left pane's navigation: 28 px rows, 11 px section labels, and an
 * accent-tinted capsule with an accent icon for the selection.
 */
export function SourceList({ label, sections, selectedId, onSelect, className }: SourceListProps) {
  return (
    <ListBox
      aria-label={label}
      selectionMode="single"
      selectionBehavior="replace"
      disallowEmptySelection
      selectedKeys={[selectedId]}
      onSelectionChange={(keys: Selection) => {
        if (keys === "all") return;
        const [next] = [...keys];
        if (next !== undefined) onSelect(String(next));
      }}
      className={cx("flex flex-col gap-px outline-none", className)}
    >
      {sections.map((section) => (
        <ListBoxSection key={section.id} id={section.id} className="flex flex-col gap-px">
          {section.title && (
            <Header className="px-2.5 pt-4 pb-1 font-semibold text-caption text-ink-secondary">
              {section.title}
            </Header>
          )}
          {section.items.map(({ id, label: text, icon: Icon, count, trailing }) => (
            <ListBoxItem
              key={id}
              id={id}
              textValue={text}
              className="focus-ring flex h-7 cursor-default items-center gap-2 rounded-row px-2.5 text-body text-ink select-none data-hovered:bg-fill data-selected:bg-accent-tint data-selected:font-medium"
            >
              {({ isSelected }) => (
                <>
                  <Icon
                    aria-hidden
                    size={15}
                    strokeWidth={1.9}
                    className={isSelected ? "text-accent-ink" : "text-ink-secondary"}
                  />
                  <span className="min-w-0 flex-1 truncate">{text}</span>
                  {trailing ??
                    (count !== undefined && (
                      // On the selected row's tint, secondary ink falls short of 4.5:1.
                      <span
                        className={`text-footnote ${isSelected ? "text-accent-ink" : "text-ink-secondary"}`}
                      >
                        {count}
                      </span>
                    ))}
                </>
              )}
            </ListBoxItem>
          ))}
        </ListBoxSection>
      ))}
    </ListBox>
  );
}
