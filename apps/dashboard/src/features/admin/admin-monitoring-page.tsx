import { ArrowSquareOut } from "@phosphor-icons/react";
import { Badge, Card } from "@techatlas/ui";
import { configuredObservabilityUrl } from "@/app/observability";
import { OperationsOverviewError, useOperationsOverview } from "@/features/operations/operations-query";
import { AdminOperationsSection, AdminOperationsState } from "./admin-operations-state";

export function AdminMonitoringPage() {
  const overview = useOperationsOverview();
  const observabilityUrl = configuredObservabilityUrl();

  if (overview.isLoading) {
    return <AdminOperationsState title="Loading monitoring" description="Checking protected dependency and workload telemetry." />;
  }
  if (overview.isError || !overview.data) {
    const unavailable = overview.error instanceof OperationsOverviewError && overview.error.isUnavailable;
    return (
      <AdminOperationsState
        title={unavailable ? "Monitoring data unavailable" : "Could not load monitoring"}
        description={unavailable ? "Your administrator session could not access the latest monitoring data." : "The monitoring summary could not be retrieved."}
        onRetry={unavailable ? undefined : () => void overview.refetch()}
      />
    );
  }

  const { dependencies, alertable_failures: alerts, queue, workers } = overview.data;
  const healthyWorkerCount = workers.filter((worker) => worker.status === "healthy").length;
  const staleWorkerCount = workers.filter((worker) => worker.status === "stale").length;

  return (
    <div className="space-y-8">
      <AdminOperationsSection
        title="Monitoring"
        description="Live protected status from TechAtlas services. Use the observability stack for detailed metrics, traces, and incident investigation."
      >
        {observabilityUrl ? (
          <div className="mt-5 flex flex-wrap gap-3">
            <a
              className="inline-flex min-h-10 items-center justify-center gap-2 rounded-md bg-surface-container px-4 py-2 font-mono text-label font-medium leading-4 tracking-wide text-on-surface transition-colors hover:bg-surface-container-high focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
              href={observabilityUrl}
              target="_blank"
              rel="noreferrer"
            >
              Open observability <ArrowSquareOut size={16} aria-hidden="true" />
            </a>
          </div>
        ) : null}
      </AdminOperationsSection>
      <section className="grid gap-4 md:grid-cols-3" aria-label="Dependency readiness">
        {dependencies.map((dependency) => (
          <Card key={dependency.name}>
            <div className="flex items-center justify-between gap-3">
              <p className="font-mono text-xs text-on-surface-variant">{dependency.name}</p>
              <Badge tone={dependency.status === "ready" ? "success" : "warning"}>{dependency.status}</Badge>
            </div>
            <p className="mt-4 text-sm text-on-surface-variant">
              {dependency.status === "ready"
                ? "The API dependency readiness probe is responding."
                : "The API dependency readiness probe is unavailable. Investigate through the monitoring runbook."}
            </p>
          </Card>
        ))}
      </section>
      <section className="grid gap-4 lg:grid-cols-2">
        <Card>
          <h2 className="font-sans text-xl font-semibold">Alertable conditions</h2>
          {alerts.length ? (
            <ul className="mt-5 space-y-3">
              {alerts.map((alert) => (
                <li key={`${alert.code}-${alert.message}`} className="rounded-md border border-error/30 bg-error/10 p-3">
                  <div className="flex items-center justify-between gap-3">
                    <p className="font-mono text-xs text-error">{alert.severity}</p>
                    <span className="font-mono text-[0.625rem] text-on-surface-variant">{alert.code}</span>
                  </div>
                  <p className="mt-2 text-sm text-on-surface">{alert.message}</p>
                </li>
              ))}
            </ul>
          ) : <p className="mt-4 text-sm text-on-surface-variant">No dependency or worker-heartbeat alert conditions are active.</p>}
        </Card>
        <Card>
          <h2 className="font-sans text-xl font-semibold">Workload health</h2>
          <dl className="mt-5 grid gap-4 sm:grid-cols-3">
            {[
              ["Ready", queue.ready_count],
              ["Processing", queue.processing_count],
              ["Scheduled", queue.scheduled_count],
            ].map(([label, value]) => (
              <div key={String(label)}>
                <dt className="font-mono text-xs text-on-surface-variant">{label}</dt>
                <dd className="mt-1 font-sans text-2xl font-semibold">{Number(value).toLocaleString()}</dd>
              </div>
            ))}
          </dl>
          <p className="mt-6 text-sm text-on-surface-variant">
            {healthyWorkerCount.toLocaleString()} healthy worker{healthyWorkerCount === 1 ? "" : "s"} · {staleWorkerCount.toLocaleString()} stale.
          </p>
        </Card>
      </section>
    </div>
  );
}
