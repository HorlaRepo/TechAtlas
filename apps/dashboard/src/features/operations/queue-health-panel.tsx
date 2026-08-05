import { ArrowDownRight, ClockCounterClockwise } from "@phosphor-icons/react";
import { Card } from "@techatlas/ui";
import type { OperationsOverviewResponse } from "@techatlas/api-client";
import { formatCount, queueBucketsFor } from "./data";

const toneClasses = {
  primary: "bg-primary",
  discovery: "bg-discovery",
  tertiary: "bg-tertiary",
};

export function QueueHealthPanel({ overview }: { overview: OperationsOverviewResponse }) {
  const queueBuckets = queueBucketsFor(overview);
  const queueSize = overview.queue.ready_count + overview.queue.processing_count + overview.queue.scheduled_count;
  return (
    <Card className="min-h-80">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="font-mono text-[0.6875rem] font-medium tracking-[0.08em] text-on-surface-variant">QUEUE HEALTH</p>
          <h2 className="mt-2 font-sans text-xl font-medium tracking-tight text-on-surface">{formatCount(queueSize)} jobs</h2>
        </div>
        <span className="inline-flex items-center gap-1 font-mono text-xs text-primary">
          <ArrowDownRight size={16} weight="bold" aria-hidden="true" />
          Live
        </span>
      </div>

      <div className="mt-8 space-y-5">
        {queueBuckets.map((bucket) => (
          <div key={bucket.label}>
            <div className="flex items-center justify-between gap-3 font-mono text-xs">
              <span className="text-on-surface">{bucket.label}</span>
              <span className="text-on-surface-variant">{bucket.detail}</span>
            </div>
            <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-container-highest">
              <div className={`h-full rounded-full ${toneClasses[bucket.tone]}`} style={{ width: `${bucket.value}%` }} />
            </div>
          </div>
        ))}
      </div>

      <div className="mt-7 flex items-center gap-2 border-t border-outline-variant/30 pt-4 font-mono text-[0.6875rem] text-on-surface-variant">
        <ClockCounterClockwise size={16} aria-hidden="true" />
        Queue values refresh every 15 seconds
      </div>
    </Card>
  );
}
