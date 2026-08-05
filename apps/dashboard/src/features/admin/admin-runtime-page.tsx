import { crawlAttempts, retryCrawlAttempt, scheduleCountryEnrichmentRecrawl } from "@techatlas/api-client";
import { Button, Card } from "@techatlas/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useOperationsOverview } from "@/features/operations/operations-query";
import { AdminConfirmDialog } from "./admin-confirm-dialog";

export function AdminRuntimePage({ kind }: { kind: "scheduler" | "queue" | "workers" }) {
  const overview = useOperationsOverview();
  const attempts = useQuery({
    queryKey: ["admin", "crawl-attempts"],
    queryFn: async () => {
      const result = await crawlAttempts({ query: { limit: 50 } });
      if (result.error || !result.data) throw Error("crawl attempts unavailable");
      return result.data;
    },
    enabled: kind === "scheduler",
  });
  const client = useQueryClient();
  const [retryJobId, setRetryJobId] = useState<string | null>(null);
  const [countryRecrawlOpen, setCountryRecrawlOpen] = useState(false);
  const retry = useMutation({
    mutationFn: async (job_id: string) => {
      const result = await retryCrawlAttempt({ path: { job_id } });
      if (result.error || !result.data) throw Error("retry could not be requested");
      return result.data;
    },
    onSuccess: () => {
      setRetryJobId(null);
      void client.invalidateQueries({ queryKey: ["admin", "crawl-attempts"] });
      void client.invalidateQueries({ queryKey: ["admin", "operations"] });
    },
  });
  const countryRecrawl = useMutation({
    mutationFn: async () => {
      const result = await scheduleCountryEnrichmentRecrawl();
      if (result.error || !result.data) throw Error("country enrichment recrawl could not be scheduled");
      return result.data;
    },
    onSuccess: () => {
      setCountryRecrawlOpen(false);
      void client.invalidateQueries({ queryKey: ["admin", "crawl-attempts"] });
      void client.invalidateQueries({ queryKey: ["admin", "operations"] });
    },
  });

  if (overview.isLoading) return <Card role="status">Loading {kind} status…</Card>;
  if (overview.isError || !overview.data) return <Card role="status">Could not load {kind} status.</Card>;
  if (kind === "workers") return <section className="space-y-6"><RuntimeHeader label="ADMIN WORKERS" title="Worker fleet" /><Card><ul className="divide-y divide-outline-variant/30">{overview.data.workers.map((worker) => <li key={worker.name} className="grid gap-1 py-3 text-sm sm:grid-cols-4"><span className="font-mono">{worker.name}</span><span>{worker.region}</span><span>{worker.in_flight_work} in flight</span><span>{worker.status}</span></li>)}</ul>{overview.data.workers.length === 0 ? <p className="text-sm text-on-surface-variant">No worker heartbeats have been observed.</p> : null}</Card></section>;
  if (kind === "queue") return <section className="space-y-6"><RuntimeHeader label="ADMIN QUEUE" title="Queue health" /><div className="grid gap-4 sm:grid-cols-3">{[["Ready", overview.data.queue.ready_count], ["Processing", overview.data.queue.processing_count], ["Scheduled", overview.data.queue.scheduled_count]].map(([label, value]) => <Card key={String(label)}><p className="font-mono text-xs text-on-surface-variant">{label}</p><p className="mt-2 font-sans text-3xl font-semibold">{String(value)}</p></Card>)}</div></section>;

  return <>
    <section className="space-y-6">
      <RuntimeHeader label="ADMIN SCHEDULER" title="Crawl attempts" />
      <Card>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <p className="max-w-2xl text-sm leading-6 text-on-surface-variant">Retry status is derived by the scheduler. Safe terminal failures can be retried manually; active and automatically retrying work needs no action.</p>
          <Button size="compact" variant="secondary" onClick={() => setCountryRecrawlOpen(true)}>Recrawl domains missing country data</Button>
        </div>
        {countryRecrawl.data ? <p className="mt-4 text-sm text-primary" role="status">Scheduled {countryRecrawl.data.scheduled_domain_count.toLocaleString()} domain{countryRecrawl.data.scheduled_domain_count === 1 ? "" : "s"} for country enrichment.</p> : null}
        {countryRecrawl.isError ? <p className="mt-4 text-sm text-error" role="alert">{String(countryRecrawl.error.message)}</p> : null}
        {attempts.isLoading ? <p className="mt-6" role="status">Loading crawl attempts…</p> : null}
        {attempts.isError ? <p className="mt-6 text-error" role="alert">Could not load crawl attempts.</p> : null}
        {attempts.data ? <ul className="mt-6 divide-y divide-outline-variant/30">{attempts.data.attempts.map((attempt) => <li key={attempt.job_id} className="flex flex-wrap items-start justify-between gap-3 py-4"><div className="min-w-0"><p className="font-mono text-sm">{attempt.canonical_domain} · attempt {attempt.attempt_number}</p><p className="mt-1 text-xs text-on-surface-variant">{attempt.status}{attempt.failure_code ? ` · ${attempt.failure_code}` : ""} · queued {formatTimestamp(attempt.queued_at)}{attempt.finished_at ? ` · finished ${formatTimestamp(attempt.finished_at)}` : ""}</p>{attempt.failure_summary ? <p className="mt-2 max-w-3xl text-sm text-on-surface-variant">{attempt.failure_summary}</p> : null}</div><AttemptAction retryState={attempt.retry_state} retryEligible={attempt.retry_eligible} retryPending={retry.isPending} onRetry={() => setRetryJobId(attempt.job_id)} /></li>)}</ul> : null}
      </Card>
      {retry.isError ? <p className="text-sm text-error" role="alert">{String(retry.error.message)}</p> : null}
    </section>
    {retryJobId ? <AdminConfirmDialog title="Retry crawl attempt" description="A new queued crawl attempt will be created with the same correlation history. The original failure remains immutable." confirmLabel="Queue retry" isPending={retry.isPending} onCancel={() => setRetryJobId(null)} onConfirm={() => retry.mutate(retryJobId)} /> : null}
    {countryRecrawlOpen ? <AdminConfirmDialog title="Recrawl domains missing country data" description="Enabled domains with a successful crawl but no country observation will become eligible for the scheduler. This does not bypass robots, SSRF, or normal scheduler policy." confirmLabel="Schedule country recrawls" isPending={countryRecrawl.isPending} onCancel={() => setCountryRecrawlOpen(false)} onConfirm={() => countryRecrawl.mutate()} /> : null}
  </>;
}

function AttemptAction({ retryState, retryEligible, retryPending, onRetry }: { retryState: string; retryEligible: boolean; retryPending: boolean; onRetry: () => void }) {
  if (retryEligible) return <Button size="compact" variant="secondary" disabled={retryPending} onClick={onRetry}>Retry eligible failure</Button>;
  return <span className="font-mono text-xs text-on-surface-variant">{retryStateLabel(retryState)}</span>;
}

function retryStateLabel(state: string): string {
  switch (state) {
    case "in_progress": return "Crawl is queued or running";
    case "automatic_retry_scheduled": return "Automatic retry scheduled";
    case "manual_retry_blocked": return "Manual retry blocked by crawl policy";
    default: return "No retry action required";
  }
}

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.valueOf()) ? "Time unavailable" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(timestamp);
}

function RuntimeHeader({ label, title }: { label: string; title: string }) {
  return <header><p className="font-mono text-xs text-primary">{label}</p><h1 className="mt-2 font-sans text-3xl font-semibold">{title}</h1></header>;
}
