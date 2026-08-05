import { Card } from "@techatlas/ui";
import { OperationsOverviewError, useOperationsOverview } from "@/features/operations/operations-query";
import { AdminOperationsSection, AdminOperationsState } from "./admin-operations-state";

export function AdminOperationalAnalyticsPage() {
  const overview = useOperationsOverview();

  if (overview.isLoading) {
    return <AdminOperationsState title="Loading operational analytics" description="Retrieving protected collection and queue telemetry." />;
  }
  if (overview.isError || !overview.data) {
    const unavailable = overview.error instanceof OperationsOverviewError && overview.error.isUnavailable;
    return (
      <AdminOperationsState
        title={unavailable ? "Operational analytics unavailable" : "Could not load operational analytics"}
        description={unavailable ? "Your administrator session could not access current operations data." : "The latest operational analytics could not be retrieved."}
        onRetry={unavailable ? undefined : () => void overview.refetch()}
      />
    );
  }

  const { queue, throughput, workers } = overview.data;
  const totalThroughput = throughput.reduce((sum, point) => sum + point.completed_count, 0);
  const queueTotal = queue.ready_count + queue.processing_count + queue.scheduled_count;

  return (
    <div className="space-y-8">
      <AdminOperationsSection
        title="Operational analytics"
        description="Protected collection performance, queue distribution, and worker capacity. Public intelligence analytics remain separate."
      />
      <section className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4" aria-label="Operational analytics summary">
        {[
          ["Successful crawls", overview.data.successful_crawl_count],
          ["Recorded throughput", totalThroughput],
          ["Active workers", workers.length],
          ["Queued work", queueTotal],
        ].map(([label, value]) => (
          <Card key={String(label)}>
            <p className="font-mono text-xs text-on-surface-variant">{label}</p>
            <p className="mt-2 font-sans text-3xl font-semibold text-on-surface">{Number(value).toLocaleString()}</p>
          </Card>
        ))}
      </section>
      <section className="grid gap-4 lg:grid-cols-2">
        <Card>
          <h2 className="font-sans text-xl font-semibold">Throughput history</h2>
          <p className="mt-2 text-sm text-on-surface-variant">Completed crawl attempts in the latest available collection intervals.</p>
          <ThroughputTable points={throughput} />
        </Card>
        <Card>
          <h2 className="font-sans text-xl font-semibold">Work distribution</h2>
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
          <h3 className="mt-8 font-sans text-lg font-semibold">Worker capacity</h3>
          <ul className="mt-4 divide-y divide-outline-variant/30">
            {workers.map((worker) => (
              <li key={worker.name} className="flex items-center justify-between gap-4 py-3">
                <span className="font-mono text-sm">{worker.name}</span>
                <span className="text-sm text-on-surface-variant">{worker.in_flight_work.toLocaleString()} in flight · {worker.status}</span>
              </li>
            ))}
            {workers.length === 0 ? <li className="py-3 text-sm text-on-surface-variant">No worker heartbeats have been observed.</li> : null}
          </ul>
        </Card>
      </section>
    </div>
  );
}

function ThroughputTable({ points }: { points: { observed_at: string; completed_count: number }[] }) {
  if (!points.length) {
    return <p className="mt-5 text-sm text-on-surface-variant">No completed crawl intervals are available yet.</p>;
  }

  return (
    <div className="mt-5 overflow-x-auto">
      <table className="w-full min-w-[28rem] text-left text-sm">
        <caption className="sr-only">Completed crawl attempts by observation time</caption>
        <thead className="border-b border-outline-variant/30 font-mono text-xs text-on-surface-variant">
          <tr><th className="px-2 py-2">Observed</th><th className="px-2 py-2">Completed crawls</th></tr>
        </thead>
        <tbody>
          {points.map((point) => (
            <tr key={point.observed_at} className="border-b border-outline-variant/20">
              <td className="px-2 py-2">{formatTimestamp(point.observed_at)}</td>
              <td className="px-2 py-2">{point.completed_count.toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  if (Number.isNaN(timestamp.valueOf())) {
    return "Time unavailable";
  }
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(timestamp);
}
