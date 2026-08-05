import type { ReactNode } from "react";
import { Card } from "./card";
import { Badge, type BadgeTone } from "./badge";

type MetricCardProps = {
  label: string;
  value: string;
  icon: ReactNode;
  trend: string;
  tone?: BadgeTone;
};

export function MetricCard({ label, value, icon, trend, tone = "success" }: MetricCardProps) {
  return (
    <Card className="flex min-h-36 flex-col justify-between gap-6 transition-colors hover:bg-surface-container-high">
      <div className="flex items-start justify-between gap-3">
        <span className="text-primary" aria-hidden="true">
          {icon}
        </span>
        <Badge tone={tone}>{trend}</Badge>
      </div>
      <div>
        <p className="font-mono text-label font-medium leading-4 tracking-[0.08em] text-on-surface-variant">
          {label}
        </p>
        <p className="mt-1 font-sans text-metric font-semibold leading-8 tracking-tight text-on-surface">{value}</p>
      </div>
    </Card>
  );
}
