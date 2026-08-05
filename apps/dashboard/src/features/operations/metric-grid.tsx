import { ChartLineUp, CheckCircle, Cpu, GlobeHemisphereWest, Heartbeat, ListDashes } from "@phosphor-icons/react";
import { MetricCard } from "@techatlas/ui";
import type { ReactNode } from "react";
import type { OperationsOverviewResponse } from "@techatlas/api-client";
import { metricsFor, type MetricIconName } from "./data";

const metricIcons: Record<MetricIconName, ReactNode> = {
  domains: <GlobeHemisphereWest size={22} weight="duotone" />,
  crawls: <CheckCircle size={22} weight="duotone" />,
  technologies: <Cpu size={22} weight="duotone" />,
  throughput: <ChartLineUp size={22} weight="duotone" />,
  queue: <ListDashes size={22} weight="duotone" />,
  health: <Heartbeat size={22} weight="duotone" />,
};

export function MetricGrid({ overview }: { overview: OperationsOverviewResponse }) {
  const operationsMetrics = metricsFor(overview);
  return (
    <section className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-6" aria-label="Operations metrics">
      {operationsMetrics.map((metric) => (
        <MetricCard
          key={metric.id}
          label={metric.label}
          value={metric.value}
          trend={metric.trend}
          tone={metric.tone}
          icon={metricIcons[metric.icon]}
        />
      ))}
    </section>
  );
}
