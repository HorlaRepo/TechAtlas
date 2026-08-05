import { ArrowLeft, ArrowSquareOut, TrendDown, TrendUp } from "@phosphor-icons/react";
import { Badge, Button, Card } from "@techatlas/ui";
import { Link } from "@tanstack/react-router";
import type { PublicTechnologyHistoryPoint, PublicTechnologyProfile, PublicTechnologyTrend } from "@techatlas/api-client";
import { TechnologyProfileError, useTechnologyProfile } from "./technology-profile-query";

type TechnologyProfilePageProps = {
  technology: string;
};

export function TechnologyProfilePage({ technology }: TechnologyProfilePageProps) {
  const profileQuery = useTechnologyProfile(technology);

  if (profileQuery.isLoading) {
    return <TechnologyState title="Loading technology profile" description="Retrieving current adoption and historical observations." />;
  }
  if (profileQuery.isError) {
    const missing = profileQuery.error instanceof TechnologyProfileError && profileQuery.error.status === 404;
    return <TechnologyState title={missing ? "Technology not found" : "Could not load technology profile"} description={missing ? "This technology is not in the public catalogue." : "The public technology service is currently unavailable. Please try again."} action={missing ? <BackToLibrary /> : <Button variant="secondary" onClick={() => void profileQuery.refetch()}>Try again</Button>} />;
  }
  const profilePages = profileQuery.data?.pages ?? [];
  const profile = profilePages[0];
  if (!profile) {
    return <TechnologyState title="Could not load technology profile" description="The public technology service did not return a profile." action={<Button variant="secondary" onClick={() => void profileQuery.refetch()}>Try again</Button>} />;
  }
  const domains = profilePages.flatMap((page) => page.domains.items);

  return (
    <div className="space-y-8">
      <header>
        <BackToLibrary />
        <div className="mt-5 flex flex-col justify-between gap-5 sm:flex-row sm:items-end">
          <div>
            <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">TECHNOLOGY PROFILE</p>
            <div className="mt-3 flex flex-wrap items-center gap-3"><h1 className="font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">{profile.display_name}</h1><Badge tone="discovery">{profile.category_name}</Badge></div>
            <p className="mt-2 font-mono text-sm text-on-surface-variant">{profile.slug}</p>
          </div>
          <TrendSummary profile={profile} />
        </div>
      </header>

      <section className="grid gap-4 sm:grid-cols-2" aria-label="Technology adoption summary">
        <MetricPanel label="Current adoption" value={`${formatNumber(profile.adoption_count)} domains`} description="Domains currently observed with this technology." />
        <MetricPanel label="30-day net change" value={formatChange(profile.net_change)} description="Additions, removals, and migrations over the last 30 days." />
      </section>

      <section className="grid gap-6 xl:grid-cols-[minmax(0,1.35fr)_minmax(20rem,0.65fr)]">
        <HistoryPanel history={profile.history} />
        <RelatedTechnologies profile={profile} />
      </section>

      <DomainPanel domains={domains} hasNextPage={profileQuery.hasNextPage} isFetchingNextPage={profileQuery.isFetchingNextPage} onLoadMore={() => void profileQuery.fetchNextPage()} />
    </div>
  );
}

function BackToLibrary() {
  return <Link to="/technologies" className="inline-flex items-center gap-2 font-mono text-xs text-primary hover:text-primary-fixed focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"><ArrowLeft size={15} aria-hidden="true" />Technology library</Link>;
}

function TrendSummary({ profile }: { profile: PublicTechnologyProfile }) {
  if (profile.trend === "growing") {
    return <Badge tone="success"><TrendUp size={14} aria-hidden="true" />Growing in the last 30 days</Badge>;
  }
  if (profile.trend === "declining") {
    return <Badge tone="warning"><TrendDown size={14} aria-hidden="true" />Declining in the last 30 days</Badge>;
  }
  return <Badge tone="neutral">Stable in the last 30 days</Badge>;
}

function MetricPanel({ label, value, description }: { label: string; value: string; description: string }) {
  return <Card className="p-5"><p className="font-mono text-xs text-on-surface-variant">{label}</p><p className="mt-3 font-sans text-2xl font-semibold text-on-surface">{value}</p><p className="mt-2 text-sm leading-6 text-on-surface-variant">{description}</p></Card>;
}

function HistoryPanel({ history }: { history: PublicTechnologyHistoryPoint[] }) {
  return (
    <Card className="p-5 sm:p-6">
      <div><p className="font-mono text-xs font-medium tracking-[0.1em] text-primary">HISTORICAL TREND</p><h2 className="mt-2 font-sans text-xl font-semibold text-on-surface">30-day net change history</h2><p className="mt-2 text-sm leading-6 text-on-surface-variant">Each point reflects the net observed additions and removals on that day.</p></div>
      {history.length === 0 ? <PanelEmpty description="No technology change events were recorded in the last 30 days." /> : <div className="mt-5 overflow-x-auto"><table className="w-full min-w-80 text-left"><caption className="sr-only">Daily technology net changes for the last 30 days</caption><thead className="border-b border-outline-variant/30 font-mono text-[0.6875rem] uppercase tracking-[0.08em] text-on-surface-variant"><tr><th className="pb-3 font-medium">Date</th><th className="pb-3 text-right font-medium">Net change</th><th className="pb-3 pl-5 text-right font-medium">State</th></tr></thead><tbody className="divide-y divide-outline-variant/30">{history.map((point) => <HistoryRow key={point.day} point={point} />)}</tbody></table></div>}
    </Card>
  );
}

function HistoryRow({ point }: { point: PublicTechnologyHistoryPoint }) {
  const trend = trendForChange(point.net_change);
  return <tr><td className="py-3 font-mono text-xs text-on-surface">{formatDate(point.day)}</td><td className="py-3 text-right font-mono text-sm text-on-surface">{formatChange(point.net_change)}</td><td className="py-3 pl-5 text-right"><TrendBadge trend={trend} /></td></tr>;
}

function RelatedTechnologies({ profile }: { profile: PublicTechnologyProfile }) {
  return (
    <Card className="p-5 sm:p-6">
      <p className="font-mono text-xs font-medium tracking-[0.1em] text-primary">RELATED TECHNOLOGIES</p><h2 className="mt-2 font-sans text-xl font-semibold text-on-surface">Often observed together</h2><p className="mt-2 text-sm leading-6 text-on-surface-variant">Ranked by shared currently observed domains.</p>
      {profile.related_technologies.length === 0 ? <PanelEmpty description="No co-occurring technologies have been observed yet." /> : <ul className="mt-5 divide-y divide-outline-variant/30">{profile.related_technologies.map((related) => <li key={related.slug} className="py-3 first:pt-0"><Link to="/technologies/$technology" params={{ technology: related.slug }} className="group flex items-start justify-between gap-3 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"><span><span className="block font-sans text-base font-semibold text-on-surface group-hover:text-primary">{related.display_name}</span><span className="mt-1 block font-mono text-[0.6875rem] text-on-surface-variant">{related.category_name}</span></span><span className="font-mono text-xs text-on-surface-variant">{formatNumber(related.shared_domain_count)} shared</span></Link></li>)}</ul>}
    </Card>
  );
}

function DomainPanel({ domains, hasNextPage, isFetchingNextPage, onLoadMore }: { domains: PublicTechnologyProfile["domains"]["items"]; hasNextPage: boolean; isFetchingNextPage: boolean; onLoadMore: () => void }) {
  return (
    <section aria-labelledby="technology-domains-heading"><div className="mb-4 flex flex-wrap items-end justify-between gap-3"><div><p className="font-mono text-xs font-medium tracking-[0.1em] text-primary">OBSERVED DOMAINS</p><h2 id="technology-domains-heading" className="mt-2 font-sans text-2xl font-semibold text-on-surface">Domains using this technology</h2></div><p className="font-mono text-[0.6875rem] text-on-surface-variant">{formatNumber(domains.length)} loaded</p></div><Card className="overflow-hidden">{domains.length === 0 ? <PanelEmpty className="p-5 sm:p-6" description="No currently observed domains use this technology." /> : <ul className="divide-y divide-outline-variant/30">{domains.map((domain) => <li key={domain.canonical_domain} className="flex flex-col gap-2 p-5 sm:flex-row sm:items-center sm:justify-between"><Link to="/domains/$domain" params={{ domain: domain.canonical_domain }} className="inline-flex w-fit items-center gap-1 font-mono text-sm font-medium text-primary hover:text-primary-fixed focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{domain.canonical_domain}<ArrowSquareOut size={15} aria-hidden="true" /></Link><span className="font-mono text-xs text-on-surface-variant">Last crawled {formatTimestamp(domain.last_crawled_at)}</span></li>)}</ul>}</Card>{hasNextPage ? <div className="mt-5 flex justify-center"><Button variant="secondary" disabled={isFetchingNextPage} onClick={onLoadMore}>{isFetchingNextPage ? "Loading domains…" : "Load more domains"}</Button></div> : null}</section>
  );
}

function TrendBadge({ trend }: { trend: PublicTechnologyTrend }) {
  if (trend === "growing") return <Badge tone="success">Growing</Badge>;
  if (trend === "declining") return <Badge tone="warning">Declining</Badge>;
  return <Badge tone="neutral">Stable</Badge>;
}

function PanelEmpty({ description, className = "" }: { description: string; className?: string }) {
  return <p className={`mt-5 rounded-md bg-surface-container-low p-4 text-sm leading-6 text-on-surface-variant ${className}`}>{description}</p>;
}

function TechnologyState({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <div className="space-y-6"><BackToLibrary /><Card className="flex min-h-64 flex-col items-start justify-center gap-4" role="status"><div><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">TECHNOLOGY PROFILE</p><h1 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h1><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></div>{action}</Card></div>;
}

function trendForChange(netChange: number): PublicTechnologyTrend {
  return netChange > 0 ? "growing" : netChange < 0 ? "declining" : "stable";
}

function formatChange(value: number): string {
  return value > 0 ? `+${value}` : String(value);
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function formatDate(value: string): string {
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", timeZone: "UTC" }).format(date);
}

function formatTimestamp(value: string | null | undefined): string {
  if (!value) return "Unknown";
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.valueOf()) ? "Unknown" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(timestamp);
}
