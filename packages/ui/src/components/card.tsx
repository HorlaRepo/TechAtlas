import type { HTMLAttributes } from "react";
import { cn } from "../utils";

export function Card({ className, ...props }: HTMLAttributes<HTMLElement>) {
  return (
    <section
      className={cn(
        "rounded-xl border border-outline-variant/30 bg-transparent p-5 shadow-none sm:p-6",
        className,
      )}
      {...props}
    />
  );
}
