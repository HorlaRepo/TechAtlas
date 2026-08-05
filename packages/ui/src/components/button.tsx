import type { ButtonHTMLAttributes } from "react";
import { forwardRef } from "react";
import { cn } from "../utils";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost";
  size?: "default" | "compact";
};

const variantClasses = {
  primary:
    "bg-primary text-on-primary hover:bg-primary-fixed-dim focus-visible:outline-primary disabled:bg-surface-container-high disabled:text-on-surface-variant",
  secondary:
    "bg-surface-container text-on-surface hover:bg-surface-container-high focus-visible:outline-primary disabled:text-on-surface-variant",
  ghost:
    "text-on-surface-variant hover:bg-surface-container hover:text-on-surface focus-visible:outline-primary disabled:text-on-surface-variant",
};

const sizeClasses = {
  default: "min-h-10 px-4 py-2",
  compact: "min-h-8 px-3 py-1.5",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant = "primary", size = "default", type = "button", ...props },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cn(
        "inline-flex items-center justify-center gap-2 rounded-md font-mono text-label font-medium leading-4 tracking-wide transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 disabled:pointer-events-none",
        variantClasses[variant],
        sizeClasses[size],
        className,
      )}
      {...props}
    />
  );
});
