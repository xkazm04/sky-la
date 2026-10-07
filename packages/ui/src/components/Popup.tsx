import type { ReactElement, ReactNode } from "react";
import {
  composeRenderProps,
  Dialog,
  DialogTrigger,
  Popover,
  type PopoverProps,
} from "react-aria-components";
import { cx } from "../cx";

/** Shared popover surface for popups and menus. */
export function PopoverSurface({ className, ...props }: PopoverProps) {
  return (
    <Popover
      offset={6}
      {...props}
      className={composeRenderProps(className, (custom) =>
        cx(
          "min-w-48 rounded-inner bg-surface-raised text-ink shadow-popover outline-none data-entering:animate-[sk-pop-in_120ms_ease-out] data-exiting:animate-[sk-pop-out_90ms_ease-in]",
          custom,
        ),
      )}
    />
  );
}

export interface PopupProps {
  /** The element that opens it, usually a `Button`. */
  trigger: ReactElement;
  /** Accessible name for the dialog. */
  label: string;
  children: ReactNode;
  placement?: PopoverProps["placement"];
  className?: string;
}

/** A non-modal popover with arbitrary content (filters, a detail, a form). */
export function Popup({
  trigger,
  label,
  children,
  placement = "bottom start",
  className,
}: PopupProps) {
  return (
    <DialogTrigger>
      {trigger}
      <PopoverSurface placement={placement}>
        <Dialog aria-label={label} className={cx("p-3 outline-none", className)}>
          {children}
        </Dialog>
      </PopoverSurface>
    </DialogTrigger>
  );
}
