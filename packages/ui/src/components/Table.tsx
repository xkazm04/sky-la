import type { ReactNode } from "react";
import {
  Table as AriaTable,
  Cell,
  Column,
  Row,
  type Selection,
  TableBody,
  TableHeader,
} from "react-aria-components";
import { cx } from "../cx";

export interface TableColumn<T> {
  id: string;
  title: string;
  /** Money and counts align to the end. */
  align?: "start" | "end";
  /** CSS width, e.g. `"8rem"` or `"30%"`. */
  width?: string;
  /** The column that names the row for assistive technology (one per table). */
  isRowHeader?: boolean;
  cell: (row: T) => ReactNode;
}

export interface TableSection<T> {
  id: string;
  /** In-list section header, e.g. "Needs attention". */
  title: string;
  rows: ReadonlyArray<T>;
}

export interface DataTableProps<T extends object> {
  /** Accessible name. */
  label: string;
  columns: ReadonlyArray<TableColumn<T>>;
  sections: ReadonlyArray<TableSection<T>>;
  /** A row's stable key; `row.id` when absent. */
  rowKey?: (row: T) => string;
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  /** Double-click or Enter. */
  onAction?: (id: string) => void;
  /** Shown when there are no rows at all. */
  empty?: ReactNode;
  className?: string;
}

const SECTION = "section:";

/**
 * The content pane's list: 40 px rows, hairline separators, in-list section
 * headers. Arrow keys move, the selection follows focus.
 */
export function DataTable<T extends object>({
  label,
  columns,
  sections,
  rowKey = (row: T) => (row as unknown as { id: string }).id,
  selectedId,
  onSelect,
  onAction,
  empty,
  className,
}: DataTableProps<T>) {
  const sectionKeys = sections.map((s) => SECTION + s.id);
  const items = sections.flatMap((section) => [
    { kind: "section" as const, key: SECTION + section.id, section },
    ...section.rows.map((row) => ({ kind: "row" as const, key: rowKey(row), row })),
  ]);
  return (
    <AriaTable
      aria-label={label}
      selectionMode={onSelect ? "single" : "none"}
      selectionBehavior="replace"
      disallowEmptySelection={Boolean(onSelect)}
      selectedKeys={selectedId ? [selectedId] : []}
      onSelectionChange={(keys: Selection) => {
        if (keys === "all") return;
        const [next] = [...keys];
        if (next !== undefined) onSelect?.(String(next));
      }}
      onRowAction={(key) => onAction?.(String(key))}
      disabledKeys={sectionKeys}
      disabledBehavior="all"
      className={cx("w-full border-separate border-spacing-0 text-body outline-none", className)}
    >
      <TableHeader>
        {columns.map((column) => (
          <Column
            key={column.id}
            id={column.id}
            isRowHeader={column.isRowHeader}
            style={column.width ? { width: column.width } : undefined}
            className={cx(
              "sticky top-0 z-10 h-8 bg-surface px-3 font-medium text-footnote text-ink-secondary first:pl-5 last:pr-5",
              "shadow-[inset_0_-0.5px_0_var(--sk-hairline-strong)]",
              column.align === "end" ? "text-right" : "text-left",
            )}
          >
            {column.title}
          </Column>
        ))}
      </TableHeader>
      <TableBody
        items={items}
        renderEmptyState={() => (
          <div className="px-5 py-10 text-center text-ink-secondary">
            {empty ?? "Nothing here."}
          </div>
        )}
      >
        {(item) =>
          item.kind === "section" ? (
            <Row id={item.key} className="cursor-default">
              <Cell
                colSpan={columns.length}
                className="h-8 px-5 pt-3 pb-1 font-semibold text-caption text-ink-secondary uppercase tracking-[0.05em]"
              >
                {item.section.title} · {item.section.rows.length}
              </Cell>
            </Row>
          ) : (
            <Row
              id={item.key}
              className="group/row cursor-default outline-none data-hovered:bg-fill data-selected:bg-accent-tint data-focus-visible:shadow-[inset_0_0_0_2px_var(--sk-focus)]"
            >
              {columns.map((column) => (
                <Cell
                  key={column.id}
                  className={cx(
                    "h-10 truncate border-hairline border-t px-3 first:pl-5 last:pr-5",
                    column.align === "end" ? "text-right" : "text-left",
                  )}
                >
                  {column.cell(item.row)}
                </Cell>
              ))}
            </Row>
          )
        }
      </TableBody>
    </AriaTable>
  );
}
