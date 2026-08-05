import { TrendUp } from "@phosphor-icons/react";
import { Badge, Card } from "@techatlas/ui";
import type { OperationsOverviewResponse } from "@techatlas/api-client";
import { throughputFor } from "./data";

const chartWidth = 520;
const chartHeight = 176;
const chartPadding = 12;

function pointsFor(values: number[]) {
  const minimum = Math.min(...values);
  const maximum = Math.max(...values);
  const range = maximum - minimum || 1;

  return values
    .map((value, index) => {
      const x = chartPadding + (index / Math.max(values.length - 1, 1)) * (chartWidth - chartPadding * 2);
      const y = chartHeight - chartPadding - ((value - minimum) / range) * (chartHeight - chartPadding * 2);
      return `${x},${y}`;
    })
    .join(" ");
}

export function ThroughputPanel({ overview }: { overview: OperationsOverviewResponse }) {
  const throughput = throughputFor(overview);
  if (throughput.length === 0) {
    return <Card className="col-span-1 min-h-80 lg:col-span-2"><PanelEmpty label="Crawl throughput" /></Card>;
  }
  const values = throughput.map((point) => point.value);
  const points = pointsFor(values);

  return (
    <Card className="col-span-1 min-h-80 lg:col-span-2">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="font-mono text-[0.6875rem] font-medium tracking-[0.08em] text-on-surface-variant">CRAWL THROUGHPUT</p>
          <h2 className="mt-2 font-sans text-xl font-medium tracking-tight text-on-surface">Steady collection velocity</h2>
        </div>
        <Badge tone="success">
          <TrendUp size={13} weight="bold" aria-hidden="true" />
          Live
        </Badge>
      </div>

      <div className="mt-7" role="img" aria-label="Successful crawl completions across the last 24 hours.">
        <svg className="h-44 w-full overflow-visible" viewBox={`0 0 ${chartWidth} ${chartHeight}`} preserveAspectRatio="none" aria-hidden="true">
          {[0.2, 0.5, 0.8].map((position) => (
            <line
              key={position}
              x1={chartPadding}
              x2={chartWidth - chartPadding}
              y1={chartHeight * position}
              y2={chartHeight * position}
              stroke="currentColor"
              strokeDasharray="3 5"
              className="text-outline-variant/50"
            />
          ))}
          <polyline points={points} fill="none" stroke="currentColor" strokeWidth="3" className="text-primary" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <div className="mt-1 flex justify-between font-mono text-[0.625rem] text-on-surface-variant">
          <span>{throughput[0]?.time}</span>
          <span>{throughput[Math.floor(throughput.length * 0.25)]?.time}</span>
          <span>{throughput[Math.floor(throughput.length * 0.5)]?.time}</span>
          <span>{throughput[Math.floor(throughput.length * 0.75)]?.time}</span>
          <span>{throughput.at(-1)?.time}</span>
        </div>
      </div>
    </Card>
  );
}

function PanelEmpty({ label }: { label: string }) {
  return <p className="font-mono text-sm text-on-surface-variant">{label} will appear after the first successful crawl.</p>;
}
