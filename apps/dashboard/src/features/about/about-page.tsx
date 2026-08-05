import { Card } from "@techatlas/ui";

const principles = [
  ["Public observations", "TechAtlas records technologies observed on publicly accessible sites. It does not authenticate to targets, submit forms, bypass access controls, or execute browser automation."],
  ["Evidence and confidence", "Each detection is produced by deterministic rules and retains its method, rule version, timestamp, and supporting evidence. A confidence score describes that observation, not a guarantee about the whole site."],
  ["Unknown is not absent", "A missing signal can mean a page was unavailable, the technology was not exposed in the crawlable response, or there was not enough evidence. It is not proof that a technology is absent."],
  ["Refresh cadence", "Domain observations refresh according to the published crawl policy. Historical adoption trends are daily aggregates from retained crawl snapshots and may contain gaps when data was not captured."],
];

export function AboutPage() {
  return (
    <div className="space-y-8">
      <header className="max-w-3xl">
        <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">ABOUT TECHATLAS</p>
        <h1 className="mt-3 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">A public map of observed web technology</h1>
        <p className="mt-3 text-sm leading-6 text-on-surface-variant">TechAtlas makes carefully bounded, evidence-backed technology observations easier to explore. It is an intelligence index, not a claim of complete coverage.</p>
      </header>

      <section className="grid gap-4 md:grid-cols-2" aria-label="TechAtlas methodology">
        {principles.map(([title, description]) => (
          <Card key={title}>
            <h2 className="font-sans text-xl font-semibold text-on-surface">{title}</h2>
            <p className="mt-3 text-sm leading-6 text-on-surface-variant">{description}</p>
          </Card>
        ))}
      </section>

      <Card>
        <h2 className="font-sans text-xl font-semibold text-on-surface">How to read the data</h2>
        <ul className="mt-4 list-disc space-y-2 pl-5 text-sm leading-6 text-on-surface-variant">
          <li>Domain profiles show the latest available observation and link to its evidence context.</li>
          <li>Analytics show public, rebuildable corpus aggregates; protected service health and crawl operations are never included.</li>
          <li>Technology changes describe differences between retained observations. They do not establish intent, ownership, or causality.</li>
        </ul>
      </Card>
    </div>
  );
}
