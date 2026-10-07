import { Search, X } from "lucide-react";
import {
  Button as AriaButton,
  SearchField as AriaSearchField,
  type SearchFieldProps as AriaSearchFieldProps,
  Input,
} from "react-aria-components";
import { cx } from "../cx";
import { Kbd } from "./Kbd";

export interface SearchFieldProps extends Omit<AriaSearchFieldProps, "className"> {
  /** Accessible name. */
  label: string;
  placeholder?: string;
  /** Shortcut hint shown while empty, e.g. `"mod+f"`. */
  shortcut?: string;
  className?: string;
}

/** A glass search capsule with a clear button. Escape clears it. */
export function SearchField({
  label,
  placeholder,
  shortcut,
  className,
  ...props
}: SearchFieldProps) {
  return (
    <AriaSearchField
      {...props}
      aria-label={label}
      className={cx(
        "group @container glass-capsule flex h-7 w-56 min-w-28 shrink items-center gap-1.5 rounded-full pr-1 pl-2.5 has-[input[data-focus-visible]]:outline-3 has-[input[data-focus-visible]]:outline-focus",
        className,
      )}
    >
      <Search aria-hidden size={14} strokeWidth={2} className="shrink-0 text-ink-secondary" />
      <Input
        placeholder={placeholder}
        className="min-w-0 flex-1 bg-transparent text-body text-ink outline-none placeholder:text-ink-tertiary [&::-webkit-search-cancel-button]:hidden"
      />
      {shortcut && (
        // Only while empty, and only when the field is wide enough for the hint.
        <span className="mr-1 hidden @min-[10rem]:group-data-[empty]:flex">
          <Kbd keys={shortcut} />
        </span>
      )}
      <AriaButton className="focus-ring inline-flex size-5 cursor-default items-center justify-center rounded-full text-ink-secondary data-hovered:bg-fill-hover group-data-[empty]:hidden">
        <X aria-hidden size={12} strokeWidth={2.25} />
      </AriaButton>
    </AriaSearchField>
  );
}
