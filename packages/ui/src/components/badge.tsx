import type { HTMLAttributes } from "react";
import { cn } from "../utils";

export type BadgeTone = "success" | "discovery" | "warning" | "neutral";

const toneClasses: Record<BadgeTone, string> = {
  success: "bg-primary-container/15 text-primary-fixed",
  discovery: "bg-discovery/15 text-secondary-fixed",
  warning: "bg-tertiary-container/20 text-tertiary-fixed",
  neutral: "bg-surface-container-high text-on-surface-variant",
};

export function Badge({
  className,
  children,
  tone = "neutral",
  ...props
}: HTMLAttributes<HTMLSpanElement> & { tone?: BadgeTone }) {

  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full px-2 py-1 font-mono text-label font-medium leading-4 tracking-wide",
        toneClasses[tone],
        className,
      )}
      {...props}
    >
      {children}
    </span>
  );
}
