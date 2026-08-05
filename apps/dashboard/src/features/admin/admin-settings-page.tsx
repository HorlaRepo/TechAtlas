import { Card } from "@techatlas/ui";

const settingsBoundaries = [
  [
    "Runtime configuration",
    "Service endpoints, resource limits, retention, and release settings are managed through deployment environment configuration. They are intentionally not editable from the browser.",
  ],
  [
    "Access and authorization",
    "Administrator access is managed through the configured identity provider and permission claims. TechAtlas does not display or modify credentials here.",
  ],
  [
    "Observability",
    "Prometheus, Grafana, and Tempo are operated through the deployment observability stack. Use the Monitoring page for live service summaries and the monitoring runbook for diagnosis.",
  ],
];

export function AdminSettingsPage() {
  return (
    <div className="space-y-8">
      <header className="max-w-3xl">
        <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">ADMINISTRATION</p>
        <h1 className="mt-2 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">Settings</h1>
        <p className="mt-2 text-sm leading-6 text-on-surface-variant">
          This page records the operational configuration boundary. Runtime settings remain environment-managed unless a separately approved, audited configuration workflow is added.
        </p>
      </header>
      <section className="grid gap-4 md:grid-cols-3" aria-label="Configuration boundaries">
        {settingsBoundaries.map(([title, description]) => (
          <Card key={title}>
            <h2 className="font-sans text-xl font-semibold">{title}</h2>
            <p className="mt-3 text-sm leading-6 text-on-surface-variant">{description}</p>
          </Card>
        ))}
      </section>
      <Card>
        <h2 className="font-sans text-xl font-semibold">No browser-managed settings</h2>
        <p className="mt-3 text-sm leading-6 text-on-surface-variant">
          No values, secrets, or mutation controls are available here. Changes require the documented deployment configuration process and its normal review and audit controls.
        </p>
      </Card>
    </div>
  );
}
