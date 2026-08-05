import { Pulse } from "@phosphor-icons/react";
import { Badge, Button, Card } from "@techatlas/ui";
import type { ReactNode } from "react";
import { MetricGrid } from "./metric-grid";
import { QueueHealthPanel } from "./queue-health-panel";
import { RecentActivityPanel } from "./recent-activity-panel";
import { ThroughputPanel } from "./throughput-panel";
import { WorkerFleetPanel } from "./worker-fleet-panel";
import { OperationsOverviewError, useOperationsOverview } from "./operations-query";

export function OperationsOverview() {
  const overviewQuery = useOperationsOverview();

  if (overviewQuery.isLoading) {
    return <OperationsState title="Loading Operations Center" description="Fetching the current operational overview." />;
  }

  if (overviewQuery.isError) {
    const isUnavailable = overviewQuery.error instanceof OperationsOverviewError && overviewQuery.error.isUnavailable;
    return (
      <OperationsState
        title={isUnavailable ? "Operations data unavailable" : "Could not load Operations Center"}
        description={isUnavailable ? "Your administrator session could not access the latest operations data." : "The dashboard could not retrieve the latest operational data."}
        action={!isUnavailable ? <Button variant="secondary" onClick={() => void overviewQuery.refetch()}>Try again</Button> : undefined}
      />
    );
  }

  const overview = overviewQuery.data;
  if (!overview || (overview.domain_count === 0 && overview.workers.length === 0 && overview.activity.length === 0)) {
    return <OperationsState title="No operations data yet" description="Metrics will appear after domains are collected or a worker starts reporting." />;
  }

  return (
    <div className="space-y-8">
      <header className="flex flex-col justify-between gap-5 sm:flex-row sm:items-end">
        <div>
          <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">SYSTEM OPS</p>
          <h1 className="mt-2 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">Operations Center</h1>
          <p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">Real-time telemetry and infrastructure health for the TechAtlas intelligence platform.</p>
        </div>
        <Badge tone={overview.system_status === "online" ? "success" : "warning"} className="w-fit px-3 py-2 text-xs">
          <Pulse size={14} weight="fill" aria-hidden="true" />
          System {overview.system_status}
        </Badge>
      </header>

      <MetricGrid overview={overview} />

      <section className="grid grid-cols-1 gap-4 lg:grid-cols-3" aria-label="Operational detail">
        <ThroughputPanel overview={overview} />
        <QueueHealthPanel overview={overview} />
        <WorkerFleetPanel overview={overview} />
        <RecentActivityPanel overview={overview} />
      </section>
    </div>
  );
}

function OperationsState({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  return (
    <Card className="flex min-h-72 flex-col items-start justify-center gap-4" role="status">
      <div>
        <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">SYSTEM OPS</p>
        <h1 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h1>
        <p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p>
      </div>
      {action}
    </Card>
  );
}
