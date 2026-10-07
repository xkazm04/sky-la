import type { LucideIcon } from "lucide-react";
import type { ReactElement, ReactNode } from "react";
import {
  Menu as AriaMenu,
  MenuItem as AriaMenuItem,
  type MenuItemProps as AriaMenuItemProps,
  MenuTrigger,
  Separator,
  Text,
} from "react-aria-components";
import { cx } from "../cx";
import { Kbd } from "./Kbd";
import { PopoverSurface } from "./Popup";

export interface MenuProps {
  /** The button that opens the menu. */
  trigger: ReactElement;
  /** Accessible name. */
  label: string;
  children: ReactNode;
  onAction?: (id: string) => void;
  placement?: "bottom start" | "bottom end";
}

/** A dropdown menu of actions. Arrow keys move, Enter acts, Escape closes. */
export function Menu({
  trigger,
  label,
  children,
  onAction,
  placement = "bottom start",
}: MenuProps) {
  return (
    <MenuTrigger>
      {trigger}
      <PopoverSurface placement={placement}>
        <AriaMenu
          aria-label={label}
          onAction={(key) => onAction?.(String(key))}
          className="max-h-[inherit] overflow-auto p-1 outline-none"
        >
          {children}
        </AriaMenu>
      </PopoverSurface>
    </MenuTrigger>
  );
}

export interface MenuItemProps extends Omit<AriaMenuItemProps, "children"> {
  id: string;
  children: string;
  icon?: LucideIcon;
  shortcut?: string;
  destructive?: boolean;
}

export function MenuItem({
  children,
  icon: Icon,
  shortcut,
  destructive,
  className,
  ...props
}: MenuItemProps) {
  return (
    <AriaMenuItem
      {...props}
      textValue={children}
      className={cx(
        "group flex h-7 cursor-default items-center gap-2 rounded-[7px] px-2 text-body outline-none select-none data-disabled:text-ink-disabled data-focused:bg-accent data-focused:text-on-accent",
        destructive ? "text-negative-ink" : "text-ink",
        typeof className === "string" ? className : undefined,
      )}
    >
      {Icon && <Icon aria-hidden size={14} strokeWidth={1.9} />}
      <Text slot="label" className="flex-1">
        {children}
      </Text>
      {shortcut && (
        <Kbd
          keys={shortcut}
          className="bg-transparent shadow-none group-data-focused:text-on-accent"
        />
      )}
    </AriaMenuItem>
  );
}

export function MenuSeparator() {
  return <Separator className="mx-2 my-1 h-px border-0 bg-hairline-strong" />;
}
