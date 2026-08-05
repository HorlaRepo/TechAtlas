import type { ReactNode } from "react";
import { cn } from "../utils";

type ChartFrameProps = {
  title: string;
  description: string;
  children: ReactNode;
  className?: string;
};

/** A presentational frame for application-owned chart implementations. */
export function ChartFrame({ title, description, children, className }: ChartFrameProps) {
  const headingId = `${title.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-")}-heading`;
  return (
    <section className={cn("rounded-xl border border-outline-variant/30 bg-transparent p-5 sm:p-6", className)} aria-labelledby={headingId}>
      <h2 id={headingId} className="font-sans text-lg font-semibold text-on-surface">{title}</h2>
      <p className="mt-2 text-sm leading-6 text-on-surface-variant">{description}</p>
      <div className="mt-5">{children}</div>
    </section>
  );
}
