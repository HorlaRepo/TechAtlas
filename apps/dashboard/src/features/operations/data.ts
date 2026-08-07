import type { OperationsOverviewResponse } from "@techatlas/api-client";

export type MetricIconName = "domains" | "crawls" | "technologies" | "throughput" | "queue" | "health";
export type MetricTone = "success" | "discovery" | "warning" | "neutral";
export type OperationsMetric = {
  id: string;
  label: string;
  value: string;
  trend: string;
  tone: MetricTone;
  icon: MetricIconName;
};

export type ThroughputPoint = { time: string; value: number };
export type QueueBucket = { label: string; value: number; detail: string; tone: "primary" | "discovery" | "tertiary" };
export type Worker = { id: string; region: string; completed: string; status: "Healthy" | "Stale"; inFlightWork: number };
export type ActivityEvent = { id: string; title: string; description: string; timestamp: string; kind: "discovery" | "success" | "system" };

export function formatCount(value: number) {
  return new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 }).format(value);
}

export function activeQueueSizeFor(overview: OperationsOverviewResponse): number {
  return overview.queue.ready_count + overview.queue.processing_count;
}

export function metricsFor(overview: OperationsOverviewResponse): OperationsMetric[] {
  const queueSize = activeQueueSizeFor(overview);
  const healthyWorkers = overview.workers.filter((worker) => worker.status === "healthy").length;
  const throughputTotal = overview.throughput.reduce((total, point) => total + point.completed_count, 0);
  return [
    { id: "domains", label: "TOTAL DOMAINS", value: formatCount(overview.domain_count), trend: "Live", tone: "success", icon: "domains" },
    { id: "crawls", label: "SUCCESSFUL CRAWLS", value: formatCount(overview.successful_crawl_count), trend: "Live", tone: "success", icon: "crawls" },
    { id: "technologies", label: "TECH DETECTED", value: formatCount(overview.current_detection_count), trend: "Live", tone: "neutral", icon: "technologies" },
    { id: "throughput", label: "24H CRAWLS", value: formatCount(throughputTotal), trend: "Live", tone: "success", icon: "throughput" },
    { id: "queue", label: "QUEUE SIZE", value: formatCount(queueSize), trend: "Live", tone: "warning", icon: "queue" },
    { id: "health", label: "WORKER HEALTH", value: `${healthyWorkers}/${overview.workers.length}`, trend: overview.system_status, tone: overview.system_status === "online" ? "success" : "warning", icon: "health" },
  ];
}

export function throughputFor(overview: OperationsOverviewResponse): ThroughputPoint[] {
  return overview.throughput.map((point) => ({
    time: new Intl.DateTimeFormat("en", { hour: "2-digit", minute: "2-digit", hour12: false }).format(new Date(point.observed_at)),
    value: point.completed_count,
  }));
}

export function queueBucketsFor(overview: OperationsOverviewResponse): QueueBucket[] {
  const values = [overview.queue.ready_count, overview.queue.processing_count, overview.queue.scheduled_count];
  const total = values.reduce((sum, value) => sum + value, 0);
  return [
    { label: "Ready", value: total ? (values[0] / total) * 100 : 0, detail: `${formatCount(values[0])} jobs`, tone: "primary" },
    { label: "Processing", value: total ? (values[1] / total) * 100 : 0, detail: `${formatCount(values[1])} jobs`, tone: "discovery" },
    { label: "Scheduled", value: total ? (values[2] / total) * 100 : 0, detail: `${formatCount(values[2])} jobs`, tone: "tertiary" },
  ];
}

export function workersFor(overview: OperationsOverviewResponse): Worker[] {
  return overview.workers.map((worker) => ({
    id: worker.name,
    region: worker.region,
    completed: formatCount(worker.completed_total),
    status: worker.status === "healthy" ? "Healthy" : "Stale",
    inFlightWork: worker.in_flight_work,
  }));
}

export function activityFor(overview: OperationsOverviewResponse): ActivityEvent[] {
  return overview.activity.map((event) => ({
    id: event.id,
    title: event.title,
    description: event.description,
    timestamp: new Intl.RelativeTimeFormat("en", { numeric: "auto" }).format(
      Math.round((new Date(event.occurred_at).getTime() - Date.now()) / 60_000),
      "minute",
    ),
    kind: event.kind === "discovery" || event.kind === "success" ? event.kind : "system",
  }));
}
