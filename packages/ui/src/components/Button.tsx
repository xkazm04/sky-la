import type { LucideIcon } from "lucide-react";
import {
  Button as AriaButton,
  type ButtonProps as AriaButtonProps,
  composeRenderProps,
} from "react-aria-components";
import { cx } from "../cx";

export type ButtonVariant = "primary" | "glass" | "plain" | "destructive";

export interface ButtonProps extends AriaButtonProps {
  /** `primary`: the one accent action per view. `glass`: toolbar capsules. */
  variant?: ButtonVariant;
  size?: "regular" | "small";
  /** Leading icon. An icon-only button needs `aria-label`. */
  icon?: LucideIcon;
}

const variants: Record<ButtonVariant, string> = {
  primary: "bg-accent text-on-accent shadow-accent data-hovered:bg-accent-hover",
  glass: "glass-capsule text-ink data-hovered:bg-fill-hover",
  plain: "text-accent-ink data-hovered:bg-fill",
  destructive: "bg-negative-fill text-on-accent data-hovered:brightness-110",
};

export const buttonClasses = (
  variant: ButtonVariant,
  size: "regular" | "small",
  iconOnly: boolean,
) =>
  cx(
    "focus-ring inline-flex shrink-0 cursor-default items-center justify-center gap-1.5 rounded-full font-medium whitespace-nowrap transition-[background-color,transform,filter] duration-150 select-none data-disabled:opacity-40 data-pressed:scale-[0.97]",
    size === "regular" ? "h-7 text-body" : "h-6 text-subhead",
    iconOnly ? (size === "regular" ? "w-7" : "w-6") : size === "regular" ? "px-3.5" : "px-2.5",
    variants[variant],
  );

/** A button. Every action in the app is one of these four variants. */
export function Button({
  variant = "glass",
  size = "regular",
  icon: Icon,
  className,
  children,
  ...props
}: ButtonProps) {
  const iconOnly = children === undefined || children === null;
  return (
    <AriaButton
      {...props}
      className={composeRenderProps(className, (custom) =>
        cx(buttonClasses(variant, size, iconOnly), custom),
      )}
    >
      {composeRenderProps(children, (content) => (
        <>
          {Icon && <Icon aria-hidden size={size === "regular" ? 15 : 13} strokeWidth={1.9} />}
          {content}
        </>
      ))}
    </AriaButton>
  );
}
