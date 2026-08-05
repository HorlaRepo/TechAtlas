import { HardHat } from "@phosphor-icons/react";
import { Badge, Card } from "@techatlas/ui";
import type { OperationsOverviewResponse } from "@techatlas/api-client";
import { workersFor } from "./data";

export function WorkerFleetPanel({ overview }: { overview: OperationsOverviewResponse }) {
  const workers = workersFor(overview);
  return (
    <Card className="min-h-80">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="font-mono text-[0.6875rem] font-medium tracking-[0.08em] text-on-surface-variant">WORKER FLEET</p>
          <h2 className="mt-2 font-sans text-xl font-medium tracking-tight text-on-surface">{workers.length} active workers</h2>
        </div>
        <HardHat className="size-5 text-primary" weight="duotone" aria-hidden="true" />
      </div>

      <ul className="mt-5 divide-y divide-outline-variant/30">
        {workers.map((worker) => (
          <li key={worker.id} className="flex items-center gap-3 py-3">
            <span className={`size-2 rounded-full ${worker.status === "Healthy" ? "bg-primary" : "bg-error"}`} aria-hidden="true" />
            <div className="min-w-0 flex-1">
              <p className="truncate font-mono text-xs text-on-surface">{worker.id}</p>
              <p className="mt-1 font-mono text-[0.625rem] text-on-surface-variant">{worker.region} · {worker.inFlightWork} in flight</p>
            </div>
            <div className="hidden text-right sm:block">
              <p className="font-mono text-xs text-on-surface">{worker.completed}</p>
              <p className="mt-1 font-mono text-[0.625rem] text-on-surface-variant">COMPLETED</p>
            </div>
            <Badge tone={worker.status === "Healthy" ? "success" : "warning"}>{worker.status}</Badge>
          </li>
        ))}
      </ul>
    </Card>
  );
}
