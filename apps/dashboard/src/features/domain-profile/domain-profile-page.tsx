import { ArrowLeft, ArrowsClockwise, CaretDown, Clock, Cpu, GlobeHemisphereWest, MapPin, ShieldCheck } from "@phosphor-icons/react";
import { Badge, Button, Card } from "@techatlas/ui";
import { useState } from "react";
import type { PublicChange, PublicCrawl, PublicCrawlDetail, PublicDnsObservation, PublicDomainProfile, PublicTechnology, PublicTlsObservation } from "@techatlas/api-client";
import { DomainHistoryError, DomainProfileError, RefreshRequestError, useCrawlDetail, useDomainChanges, useDomainCrawls, useDomainProfile, useRefreshRequest } from "./domain-profile-query";

type DomainProfilePageProps = {
  domain: string;
};

export function DomainProfilePage({ domain }: DomainProfilePageProps) {
  const profileQuery = useDomainProfile(domain);

  if (profileQuery.isLoading) {
    return <ProfileState title="Loading domain profile" description="Retrieving current public technology observations." />;
  }

  if (profileQuery.isError) {
    const status = profileQuery.error instanceof DomainProfileError ? profileQuery.error.status : undefined;
    if (status === 404) {
      return <ProfileState title="Domain not found" description="This domain is not part of the public TechAtlas corpus." action={<BackToSearch />} />;
    }
    if (status === 422) {
      return <ProfileState title="Invalid domain" description="The requested domain is not a valid public domain name." action={<BackToSearch />} />;
    }
    return <ProfileState title="Could not load domain profile" description="The latest public domain data is currently unavailable. Please try again." action={<Button variant="secondary" onClick={() => void profileQuery.refetch()}>Try again</Button>} />;
  }

  if (!profileQuery.data) {
    return <ProfileState title="Could not load domain profile" description="The public profile service did not return any domain data. Please try again." action={<Button variant="secondary" onClick={() => void profileQuery.refetch()}>Try again</Button>} />;
  }

  return <Profile profile={profileQuery.data} />;
}

function Profile({ profile }: { profile: PublicDomainProfile }) {
  const categories = technologiesByCategory(profile.technologies);
  const crawlKnown = Boolean(profile.last_crawled_at);
  const refreshRequest = useRefreshRequest(profile.canonical_domain);

  return (
    <div className="space-y-8">
      <a href="/search" className="inline-flex items-center gap-2 font-mono text-xs text-on-surface-variant hover:text-primary focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"><ArrowLeft size={15} aria-hidden="true" />Back to search</a>

      <header className="border-b border-outline-variant/30 pb-6">
        <div className="flex flex-col gap-5 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0">
            <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">DOMAIN PROFILE</p>
            <div className="mt-3 flex flex-wrap items-center gap-3">
              <GlobeHemisphereWest size={28} className="shrink-0 text-primary" aria-hidden="true" />
              <h1 className="break-all font-mono text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">{profile.canonical_domain}</h1>
              <Badge tone={crawlKnown ? "success" : "warning"}>{crawlKnown ? "Crawled" : "Awaiting first crawl"}</Badge>
            </div>
          </div>
          <div className="max-w-sm"><p className="text-sm leading-6 text-on-surface-variant">Current public observations are derived from the latest completed crawl. Missing signals do not prove a technology is absent.</p><RefreshControl request={refreshRequest} /></div>
        </div>
        <dl className="mt-6 grid gap-4 sm:grid-cols-3">
          <ProfileFact icon={<Clock size={16} aria-hidden="true" />} label="Last successful crawl" value={profile.last_crawled_at ? formatTimestamp(profile.last_crawled_at) : "No successful crawl yet"} />
          <ProfileFact icon={<MapPin size={16} aria-hidden="true" />} label="Country" value={profile.country_code ?? "Country unavailable"} />
          <ProfileFact icon={<ShieldCheck size={16} aria-hidden="true" />} label="First indexed" value={formatTimestamp(profile.first_indexed_at)} />
        </dl>
      </header>

      <section aria-labelledby="current-stack-heading">
        <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
          <div>
            <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">CURRENT STACK</p>
            <h2 id="current-stack-heading" className="mt-2 font-sans text-2xl font-semibold text-on-surface">Observed technologies</h2>
          </div>
          <p className="font-mono text-xs text-on-surface-variant">{profile.technologies.length} current {profile.technologies.length === 1 ? "detection" : "detections"}</p>
        </div>

        {categories.length === 0 ? <EmptyStack /> : <div className="mt-5 space-y-5">{categories.map(([category, technologies]) => <CategoryStack key={category} category={category} technologies={technologies} />)}</div>}
      </section>

      <DomainChangesPanel domain={profile.canonical_domain} />
      <CrawlHistoryPanel domain={profile.canonical_domain} />
    </div>
  );
}

function RefreshControl({ request }: { request: ReturnType<typeof useRefreshRequest> }) {
  const refreshError = request.error instanceof RefreshRequestError ? request.error : undefined;
  const rateLimited = refreshError?.status === 429;
  const retryAfter = rateLimited && refreshError.retryAfterSeconds ? formatDuration(refreshError.retryAfterSeconds) : undefined;

  return <div className="mt-4" aria-live="polite">
    <Button variant="secondary" size="compact" disabled={request.isPending || Boolean(request.data)} onClick={() => void request.mutateAsync()}><ArrowsClockwise size={15} aria-hidden="true" />{request.isPending ? "Requesting refresh" : request.data ? "Refresh requested" : "Request a refresh"}</Button>
    {request.data ? <p className="mt-2 text-sm leading-5 text-primary">Refresh recorded for scheduler processing. Another request may be accepted after {formatTimestamp(request.data.next_allowed_at)}.</p> : null}
    {rateLimited ? <p className="mt-2 text-sm leading-5 text-on-surface-variant">A refresh was requested recently. {retryAfter ? `Try again in ${retryAfter}.` : "Please try again later."}</p> : null}
    {refreshError?.status === 409 ? <p className="mt-2 text-sm leading-5 text-on-surface-variant">Refresh requests are not available for this domain.</p> : null}
    {request.isError && !rateLimited && refreshError?.status !== 409 ? <p className="mt-2 text-sm leading-5 text-error">The refresh request could not be recorded. Please try again.</p> : null}
  </div>;
}

function DomainChangesPanel({ domain }: { domain: string }) {
  const query = useDomainChanges(domain);
  const changes = query.data?.pages.flatMap((page) => page.items) ?? [];

  return <section aria-labelledby="domain-changes-heading">
    <SectionHeading eyebrow="CHANGE TIMELINE" id="domain-changes-heading" title="Technology history" description="Observed additions, removals, migrations, and reliable version changes from immutable records." />
    {query.isPending ? <HistoryState title="Loading technology history" description="Retrieving recorded technology changes." /> : null}
    {query.isError ? <HistoryError query={query} label="technology history" /> : null}
    {query.isSuccess && changes.length === 0 ? <HistoryState title="No technology changes recorded" description="Changes will appear after the domain has more than one comparable crawl snapshot." /> : null}
    {query.isSuccess && changes.length > 0 ? <Card className="mt-5 overflow-hidden p-0"><ol className="divide-y divide-outline-variant/30">{changes.map((change, index) => <ChangeItem change={change} key={`${change.observed_at}-${change.kind}-${index}`} />)}</ol><LoadMore query={query} label="changes" /></Card> : null}
  </section>;
}

function CrawlHistoryPanel({ domain }: { domain: string }) {
  const query = useDomainCrawls(domain);
  const crawls = query.data?.pages.flatMap((page) => page.items) ?? [];

  return <section aria-labelledby="crawl-history-heading">
    <SectionHeading eyebrow="CRAWL HISTORY" id="crawl-history-heading" title="Captured responses" description="Open a crawl to inspect the safely published response and network metadata." />
    {query.isPending ? <HistoryState title="Loading crawl history" description="Retrieving immutable crawl snapshots." /> : null}
    {query.isError ? <HistoryError query={query} label="crawl history" /> : null}
    {query.isSuccess && crawls.length === 0 ? <HistoryState title="No crawls recorded" description="Crawl response metadata will appear after the first successful collection." /> : null}
    {query.isSuccess && crawls.length > 0 ? <Card className="mt-5 overflow-hidden p-0"><ol className="divide-y divide-outline-variant/30">{crawls.map((crawl) => <CrawlDisclosure domain={domain} crawl={crawl} key={crawl.id} />)}</ol><LoadMore query={query} label="crawls" /></Card> : null}
  </section>;
}

function SectionHeading({ eyebrow, id, title, description }: { eyebrow: string; id: string; title: string; description: string }) {
  return <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between"><div><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">{eyebrow}</p><h2 id={id} className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h2></div><p className="max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></div>;
}

function HistoryState({ title, description }: { title: string; description: string }) {
  return <Card className="mt-5 flex min-h-40 flex-col items-start justify-center"><h3 className="font-sans text-lg font-semibold text-on-surface">{title}</h3><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></Card>;
}

function HistoryError({ query, label }: { query: { error: unknown; refetch: () => Promise<unknown> }; label: string }) {
  const status = query.error instanceof DomainHistoryError ? query.error.status : undefined;
  const description = status === 404 ? "This domain is no longer available in the public corpus." : `The ${label} service is currently unavailable.`;
  return <Card className="mt-5 flex min-h-40 flex-col items-start justify-center gap-4"><div><h3 className="font-sans text-lg font-semibold text-on-surface">Could not load {label}</h3><p className="mt-2 text-sm leading-6 text-on-surface-variant">{description}</p></div><Button variant="secondary" onClick={() => void query.refetch()}>Try again</Button></Card>;
}

function LoadMore({ query, label }: { query: { hasNextPage: boolean; isFetchingNextPage: boolean; fetchNextPage: () => Promise<unknown> }; label: string }) {
  if (!query.hasNextPage) return null;
  return <div className="border-t border-outline-variant/30 px-5 py-4 sm:px-6"><Button variant="ghost" size="compact" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{query.isFetchingNextPage ? `Loading ${label}` : `Load more ${label}`}</Button></div>;
}

function ChangeItem({ change }: { change: PublicChange }) {
  return <li className="flex gap-4 px-5 py-4 sm:px-6"><span className="mt-1.5 size-2 shrink-0 rounded-full bg-discovery" aria-hidden="true" /><div className="min-w-0 flex-1"><div className="flex flex-col gap-2 sm:flex-row sm:items-start sm:justify-between"><p className="font-medium text-on-surface">{changeSummary(change)}</p><time className="shrink-0 font-mono text-xs text-on-surface-variant" dateTime={change.observed_at}>{formatTimestamp(change.observed_at)}</time></div><p className="mt-1 font-mono text-xs text-on-surface-variant">{change.category_slug} · {change.kind}</p></div></li>;
}

function changeSummary(change: PublicChange): string {
  if (change.kind === "version_changed" && change.technology_slug && change.from_version && change.to_version) return `${change.technology_slug} changed from ${change.from_version} to ${change.to_version}`;
  if (change.kind === "migrated" && change.from_technology_slug && change.to_technology_slug) return `${change.from_technology_slug} migrated to ${change.to_technology_slug}`;
  if (change.kind === "added" && change.to_technology_slug) return `${change.to_technology_slug} was added`;
  if (change.kind === "removed" && change.from_technology_slug) return `${change.from_technology_slug} was removed`;
  return "Technology observation changed";
}

function CrawlDisclosure({ domain, crawl }: { domain: string; crawl: PublicCrawl }) {
  const [open, setOpen] = useState(false);
  const detail = useCrawlDetail(domain, crawl.id, open);

  return <li><details className="group" onToggle={(event) => setOpen(event.currentTarget.open)}><summary className="flex cursor-pointer list-none flex-col gap-3 px-5 py-4 hover:bg-surface-container-low/60 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-primary sm:flex-row sm:items-center sm:justify-between sm:px-6"><span className="min-w-0"><span className="block truncate font-mono text-sm font-medium text-on-surface">{crawl.final_url}</span><span className="mt-1 block font-mono text-xs text-on-surface-variant">Requested {crawl.requested_url}</span></span><span className="flex shrink-0 items-center gap-3"><Badge tone={crawl.response_status >= 200 && crawl.response_status < 400 ? "success" : "warning"}>HTTP {crawl.response_status}</Badge><time className="font-mono text-xs text-on-surface-variant" dateTime={crawl.captured_at}>{formatTimestamp(crawl.captured_at)}</time><CaretDown className="size-4 text-on-surface-variant transition-transform group-open:rotate-180" aria-hidden="true" /></span></summary>{open ? <CrawlDetailBody detail={detail} /> : null}</details></li>;
}

function CrawlDetailBody({ detail }: { detail: ReturnType<typeof useCrawlDetail> }) {
  if (detail.isPending) return <div className="border-t border-outline-variant/30 px-5 py-5 text-sm text-on-surface-variant sm:px-6">Loading response metadata…</div>;
  if (detail.isError || !detail.data) return <div className="border-t border-outline-variant/30 px-5 py-5 sm:px-6"><p className="text-sm text-on-surface-variant">Response metadata is currently unavailable.</p><Button className="mt-3" variant="secondary" size="compact" onClick={() => void detail.refetch()}>Try again</Button></div>;
  return <CrawlMetadata detail={detail.data} />;
}

function CrawlMetadata({ detail }: { detail: PublicCrawlDetail }) {
  const headers = Object.entries(detail.response_headers).sort(([left], [right]) => left.localeCompare(right));
  return <div className="border-t border-outline-variant/30 bg-surface-container-low/30 px-5 py-5 sm:px-6"><dl className="grid gap-4 sm:grid-cols-3"><Detail label="Requested URL" value={detail.crawl.requested_url} /><Detail label="Final URL" value={detail.crawl.final_url} /><Detail label="Country" value={detail.country_code ?? "Country unavailable"} /></dl><MetadataList title="Redirect chain" empty="No redirects were recorded." items={detail.redirect_chain.map((redirect) => ({ label: `HTTP ${redirect.status}`, value: `${redirect.from_url} → ${redirect.to_url}` }))} /><DnsMetadata dns={detail.dns} /><TlsMetadata tls={detail.tls} /><MetadataList title="Response headers (sanitized)" empty="No publishable response headers were recorded." items={headers.map(([key, value]) => ({ label: key, value }))} /></div>;
}

function DnsMetadata({ dns }: { dns: PublicDnsObservation }) {
  if (dns.availability !== "available") return <UnavailableNetworkMetadata title="DNS" reason={dns.unavailable_reason} />;
  return <MetadataList title="DNS" empty="No public DNS addresses were recorded." items={[...(dns.queried_name ? [{ label: "Queried name", value: dns.queried_name }] : []), ...dns.addresses.map((address) => ({ label: "Address", value: address }))]} />;
}

function TlsMetadata({ tls }: { tls: PublicTlsObservation }) {
  if (tls.availability !== "available") return <UnavailableNetworkMetadata title="TLS certificate" reason={tls.unavailable_reason} />;
  const validation = tls.validation_status === "verified" ? "Verified" : "Validation failed — certificate facts were observed without trusting the chain.";
  return <MetadataList title="TLS certificate" empty="No publishable TLS certificate facts were recorded." items={[{ label: "Validation", value: validation }, ...(tls.protocol ? [{ label: "Protocol", value: tls.protocol }] : []), ...(tls.cipher_suite ? [{ label: "Cipher suite", value: tls.cipher_suite }] : []), ...(tls.certificate_subject ? [{ label: "Subject", value: tls.certificate_subject }] : []), ...(tls.certificate_issuer ? [{ label: "Issuer", value: tls.certificate_issuer }] : []), ...(tls.certificate_not_before ? [{ label: "Valid from", value: formatTimestamp(tls.certificate_not_before) }] : []), ...(tls.certificate_not_after ? [{ label: "Valid until", value: formatTimestamp(tls.certificate_not_after) }] : []), ...tls.subject_alternative_names.map((name) => ({ label: "Subject alternative name", value: name }))]} />;
}

function UnavailableNetworkMetadata({ title, reason }: { title: string; reason?: string | null }) {
  const message = reason === "not_applicable" ? `${title} does not apply to this HTTP response.` : reason === "not_captured" ? `${title} was not captured for this historical crawl.` : `${title} was unavailable during this crawl${reason ? ` (${reason.replaceAll("_", " ")})` : ""}.`;
  return <div className="mt-5"><p className="font-mono text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-on-surface-variant">{title}</p><p className="mt-2 text-sm text-on-surface-variant">{message}</p></div>;
}

function MetadataList({ title, empty, items }: { title: string; empty: string; items: { label: string; value: string }[] }) {
  return <div className="mt-5"><p className="font-mono text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-on-surface-variant">{title}</p>{items.length === 0 ? <p className="mt-2 text-sm text-on-surface-variant">{empty}</p> : <dl className="mt-3 divide-y divide-outline-variant/30 rounded-lg border border-outline-variant/30">{items.map((item) => <div className="grid gap-2 px-4 py-3 sm:grid-cols-[10rem_minmax(0,1fr)]" key={`${item.label}-${item.value}`}><dt className="break-all font-mono text-xs text-primary">{item.label}</dt><dd className="break-all font-mono text-xs leading-5 text-on-surface-variant">{item.value}</dd></div>)}</dl>}</div>;
}

function ProfileFact({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return <div className="min-w-0 rounded-lg border border-outline-variant/30 bg-surface-container-low/40 p-4"><dt className="flex items-center gap-2 font-mono text-[0.6875rem] uppercase tracking-[0.08em] text-on-surface-variant">{icon}{label}</dt><dd className="mt-2 break-words text-sm font-medium text-on-surface">{value}</dd></div>;
}

function CategoryStack({ category, technologies }: { category: string; technologies: PublicTechnology[] }) {
  return (
    <Card className="overflow-hidden p-0">
      <div className="border-b border-outline-variant/30 px-5 py-4 sm:px-6"><h3 className="font-sans text-lg font-semibold text-on-surface">{category}</h3><p className="mt-1 font-mono text-xs text-on-surface-variant">{technologies.length} current {technologies.length === 1 ? "detection" : "detections"}</p></div>
      <div className="divide-y divide-outline-variant/30">{technologies.map((technology) => <TechnologyDisclosure key={technology.slug} technology={technology} />)}</div>
    </Card>
  );
}

function TechnologyDisclosure({ technology }: { technology: PublicTechnology }) {
  return (
    <details className="group">
      <summary className="flex cursor-pointer list-none flex-col gap-4 px-5 py-4 hover:bg-surface-container-low/60 focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-primary sm:flex-row sm:items-center sm:justify-between sm:px-6">
        <div className="flex min-w-0 items-center gap-3"><span className="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary-container/15 font-mono text-sm font-semibold text-primary-fixed" aria-hidden="true">{technology.display_name.slice(0, 1).toUpperCase()}</span><span className="min-w-0"><span className="block truncate font-medium text-on-surface">{technology.display_name}</span><span className="mt-1 block font-mono text-xs text-on-surface-variant">{technology.method} · rule v{technology.rule_version}</span></span></div>
        <div className="flex items-center gap-3"><Confidence confidence={technology.confidence} /><CaretDown className="size-4 text-on-surface-variant transition-transform group-open:rotate-180" aria-hidden="true" /></div>
      </summary>
      <div className="border-t border-outline-variant/30 bg-surface-container-low/30 px-5 py-5 sm:px-6">
        <dl className="grid gap-4 sm:grid-cols-2"><Detail label="First observed" value={formatTimestamp(technology.first_observed_at)} /><Detail label="Last observed" value={formatTimestamp(technology.last_observed_at)} /></dl>
        <div className="mt-5"><p className="font-mono text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-on-surface-variant">Evidence (redacted)</p>{technology.evidence.length === 0 ? <p className="mt-2 text-sm text-on-surface-variant">No publishable evidence is stored for this detection.</p> : <dl className="mt-3 divide-y divide-outline-variant/30 rounded-lg border border-outline-variant/30">{technology.evidence.map((evidence) => <div key={`${evidence.source}-${evidence.key}-${evidence.value}`} className="grid gap-2 px-4 py-3 sm:grid-cols-[9rem_minmax(0,1fr)]"><dt className="font-mono text-xs text-primary">{evidence.source} · {evidence.key}</dt><dd className="break-all font-mono text-xs leading-5 text-on-surface-variant">{evidence.value}</dd></div>)}</dl>}</div>
      </div>
    </details>
  );
}

function Confidence({ confidence }: { confidence: number }) {
  const value = Math.max(0, Math.min(100, confidence));
  return <span className="inline-flex items-center gap-2 font-mono text-xs text-on-surface" aria-label={`Confidence ${value}%`}><span className="h-1.5 w-16 overflow-hidden rounded-full bg-surface-container-high"><span className="block h-full rounded-full bg-primary" style={{ width: `${value}%` }} /></span>{value}%</span>;
}

function Detail({ label, value }: { label: string; value: string }) {
  return <div><dt className="font-mono text-[0.6875rem] uppercase tracking-[0.08em] text-on-surface-variant">{label}</dt><dd className="mt-1 text-sm text-on-surface">{value}</dd></div>;
}

function EmptyStack() {
  return <Card className="mt-5 flex min-h-48 flex-col items-start justify-center gap-3"><Cpu size={24} className="text-on-surface-variant" aria-hidden="true" /><div><h3 className="font-sans text-xl font-semibold text-on-surface">No current technology detections</h3><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">No technologies are currently detected for this domain. This is not proof that the domain does not use any technology.</p></div></Card>;
}

function ProfileState({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <Card className="flex min-h-80 flex-col items-start justify-center gap-4" role="status"><div><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">DOMAIN PROFILE</p><h1 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h1><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></div>{action}</Card>;
}

function BackToSearch() {
  return <a href="/search" className="inline-flex min-h-10 items-center justify-center rounded-md bg-surface-container px-4 py-2 font-mono text-label font-medium text-on-surface transition-colors hover:bg-surface-container-high focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary">Back to search</a>;
}

function technologiesByCategory(technologies: PublicTechnology[]): [string, PublicTechnology[]][] {
  const categories = new Map<string, PublicTechnology[]>();
  for (const technology of technologies) {
    const current = categories.get(technology.category_name) ?? [];
    current.push(technology);
    categories.set(technology.category_name, current);
  }
  return [...categories.entries()];
}

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.valueOf()) ? "Unknown" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(timestamp);
}

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.ceil((seconds % 3_600) / 60);
  if (hours > 0) return `${hours} hour${hours === 1 ? "" : "s"}${minutes > 0 ? ` ${minutes} minute${minutes === 1 ? "" : "s"}` : ""}`;
  return `${Math.max(1, minutes)} minute${minutes === 1 ? "" : "s"}`;
}
