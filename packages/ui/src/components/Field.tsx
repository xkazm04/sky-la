import { Check, ChevronsUpDown } from "lucide-react";
import type { ReactNode } from "react";
import {
  Button as AriaButton,
  Checkbox as AriaCheckbox,
  type CheckboxProps as AriaCheckboxProps,
  Select as AriaSelect,
  type SelectProps as AriaSelectProps,
  TextField as AriaTextField,
  type TextFieldProps as AriaTextFieldProps,
  FieldError,
  Input,
  Label,
  ListBox,
  ListBoxItem,
  SelectValue,
  Text,
  TextArea,
} from "react-aria-components";
import { cx } from "../cx";
import { PopoverSurface } from "./Popup";

const control =
  "h-7 w-full min-w-0 rounded-[7px] bg-surface px-2 text-body text-ink shadow-[inset_0_0_0_0.5px_var(--sk-hairline-strong)] outline-none placeholder:text-ink-tertiary data-focused:outline-3 data-focused:outline-focus data-invalid:shadow-[inset_0_0_0_1px_var(--sk-negative-ink)] data-disabled:text-ink-disabled";

const labelClass = "text-footnote font-medium text-ink-secondary";

export interface TextFieldProps extends Omit<AriaTextFieldProps, "children"> {
  /** Visible label; pass `labelHidden` to keep it for screen readers only. */
  label: string;
  labelHidden?: boolean;
  placeholder?: string;
  /** Help under the field. */
  description?: ReactNode;
  /** Shown under the field when invalid; marks the field invalid when set. */
  errorMessage?: string;
  /** A multi-line field. */
  multiline?: boolean;
  /** Right-aligned text with tabular figures (amounts, quantities). */
  numeric?: boolean;
}

/** A labelled text input. Numbers are typed as text; the core parses them. */
export function TextField({
  label,
  labelHidden,
  placeholder,
  description,
  errorMessage,
  multiline,
  numeric,
  className,
  ...props
}: TextFieldProps) {
  return (
    <AriaTextField
      {...props}
      isInvalid={props.isInvalid ?? (errorMessage ? true : undefined)}
      className={cx("flex min-w-0 flex-col gap-1", typeof className === "string" ? className : "")}
    >
      <Label className={labelHidden ? "sr-only" : labelClass}>{label}</Label>
      {multiline ? (
        <TextArea placeholder={placeholder} rows={2} className={cx(control, "h-auto py-1.5")} />
      ) : (
        <Input
          placeholder={placeholder}
          className={cx(control, numeric && "text-right tabular-nums")}
        />
      )}
      {description && (
        <Text slot="description" className="text-footnote text-ink-secondary">
          {description}
        </Text>
      )}
      <FieldError className="text-footnote text-negative-ink">{errorMessage}</FieldError>
    </AriaTextField>
  );
}

export interface SelectOption {
  readonly id: string;
  readonly label: string;
  /** A second line in the list, e.g. an IČO. */
  readonly detail?: string;
}

export interface SelectProps
  extends Omit<
    AriaSelectProps<SelectOption>,
    "children" | "items" | "onSelectionChange" | "onChange" | "value" | "selectedKey"
  > {
  label: string;
  labelHidden?: boolean;
  options: readonly SelectOption[];
  /** The selected option's id. */
  value: string | null;
  onChange: (id: string) => void;
  placeholder?: string;
  errorMessage?: string;
}

/** A labelled pop-up list, like a macOS pop-up button. */
export function Select({
  label,
  labelHidden,
  options,
  value,
  onChange,
  placeholder = "Choose…",
  errorMessage,
  className,
  ...props
}: SelectProps) {
  return (
    <AriaSelect
      {...props}
      placeholder={placeholder}
      selectedKey={value}
      onSelectionChange={(key) => key !== null && onChange(String(key))}
      isInvalid={props.isInvalid ?? (errorMessage ? true : undefined)}
      className={cx(
        "group/select flex min-w-0 flex-col gap-1",
        typeof className === "string" ? className : "",
      )}
    >
      <Label className={labelHidden ? "sr-only" : labelClass}>{label}</Label>
      <AriaButton
        className={cx(
          control,
          "flex cursor-default items-center gap-1 text-left data-focus-visible:outline-3 data-focus-visible:outline-focus data-pressed:bg-fill group-data-invalid/select:shadow-[inset_0_0_0_1px_var(--sk-negative-ink)]",
        )}
      >
        <SelectValue className="min-w-0 flex-1 truncate data-placeholder:text-ink-tertiary">
          {({ selectedText, defaultChildren, isPlaceholder }) =>
            isPlaceholder ? defaultChildren : selectedText
          }
        </SelectValue>
        <ChevronsUpDown
          aria-hidden
          size={13}
          strokeWidth={2}
          className="shrink-0 text-ink-secondary"
        />
      </AriaButton>
      <FieldError className="text-footnote text-negative-ink">{errorMessage}</FieldError>
      <PopoverSurface placement="bottom start" className="min-w-(--trigger-width)">
        <ListBox items={options} className="max-h-72 overflow-auto p-1 outline-none">
          {(o) => (
            <ListBoxItem
              id={o.id}
              textValue={o.label}
              className="group flex cursor-default items-center gap-2 rounded-[7px] px-2 py-1 text-body text-ink outline-none select-none data-focused:bg-accent data-focused:text-on-accent"
            >
              {({ isSelected }) => (
                <>
                  <span className="w-3.5 shrink-0">
                    {isSelected && <Check aria-hidden size={13} strokeWidth={2.4} />}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{o.label}</span>
                    {o.detail && (
                      <span className="block truncate text-footnote text-ink-secondary group-data-focused:text-on-accent">
                        {o.detail}
                      </span>
                    )}
                  </span>
                </>
              )}
            </ListBoxItem>
          )}
        </ListBox>
      </PopoverSurface>
    </AriaSelect>
  );
}

export interface CheckboxProps extends Omit<AriaCheckboxProps, "children"> {
  children: ReactNode;
}

/** A labelled checkbox. */
export function Checkbox({ children, className, ...props }: CheckboxProps) {
  return (
    <AriaCheckbox
      {...props}
      className={cx(
        "group flex cursor-default items-center gap-2 text-body text-ink outline-none",
        typeof className === "string" ? className : "",
      )}
    >
      <span
        aria-hidden
        className="flex size-4 shrink-0 items-center justify-center rounded-[4px] bg-surface text-on-accent shadow-[inset_0_0_0_0.5px_var(--sk-hairline-strong)] group-data-focus-visible:outline-3 group-data-focus-visible:outline-focus group-data-selected:bg-accent group-data-selected:shadow-none"
      >
        <Check size={11} strokeWidth={3} className="hidden group-data-selected:block" />
      </span>
      {children}
    </AriaCheckbox>
  );
}
