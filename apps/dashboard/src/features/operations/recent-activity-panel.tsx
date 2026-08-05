import { CheckCircle, Sparkle, TerminalWindow } from "@phosphor-icons/react";
import { Card } from "@techatlas/ui";
import type { ReactNode } from "react";
import type { OperationsOverviewResponse } from "@techatlas/api-client";
import { activityFor, type ActivityEvent } from "./data";

const activityIcons: Record<ActivityEvent["kind"], ReactNode> = {
  discovery: <Sparkle size={16} weight="fill" />,
  success: <CheckCircle size={16} weight="fill" />,
  system: <TerminalWindow size={16} weight="bold" />,
};

const activityTones: Record<ActivityEvent["kind"], string> = {
  discovery: "bg-discovery/15 text-secondary-fixed",
  success: "bg-primary-container/15 text-primary-fixed",
  system: "bg-surface-container-high text-on-surface-variant",
};

export function RecentActivityPanel({ overview }: { overview: OperationsOverviewResponse }) {
  const recentActivity = activityFor(overview);
  return (
    <Card className="min-h-80 lg:col-span-2">
      <div>
        <p className="font-mono text-[0.6875rem] font-medium tracking-[0.08em] text-on-surface-variant">LIVE ACTIVITY</p>
        <h2 className="mt-2 font-sans text-xl font-medium tracking-tight text-on-surface">Recent system events</h2>
      </div>

      {recentActivity.length === 0 ? <p className="mt-6 text-sm text-on-surface-variant">No operational events have been recorded yet.</p> : <ol className="mt-6 space-y-5">
        {recentActivity.map((event) => (
          <li key={event.id} className="flex gap-3">
            <span className={`flex size-8 shrink-0 items-center justify-center rounded-full ${activityTones[event.kind]}`} aria-hidden="true">
              {activityIcons[event.kind]}
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                <h3 className="font-sans text-sm font-medium text-on-surface">{event.title}</h3>
                <time className="font-mono text-[0.625rem] text-on-surface-variant">{event.timestamp}</time>
              </div>
              <p className="mt-1 text-sm leading-5 text-on-surface-variant">{event.description}</p>
            </div>
          </li>
        ))}
      </ol>}
    </Card>
  );
}
