import { Button, Card, ChartFrame } from "@techatlas/ui";
import type { EChartsOption } from "echarts";
import { Link } from "@tanstack/react-router";
import { lazy, Suspense, useEffect, useState } from "react";
import type { PublicAdoptionHistory, PublicChangeTrend } from "@techatlas/api-client";
import { analyticsDays, analyticsSelectedTechnology, type AnalyticsState } from "./analytics-state";
import { useAnalytics } from "./analytics-query";

const chartPalette = ["#5bdacb", "#8b5cf6", "#ffb59d", "#7aa2f7", "#e9ddff", "#f6c177"];
const ECharts = lazy(() => import("echarts-for-react"));

export function AnalyticsPage({ search, onSearchChange }: { search: AnalyticsState; onSearchChange: (state: AnalyticsState) => void }) {
  const days = analyticsDays(search);
  const selectedTechnology = analyticsSelectedTechnology(search);
  const queries = useAnalytics(days, selectedTechnology);
  const loading = Object.values(queries).some((query) => query.isLoading);
  const failed = Object.values(queries).some((query) => query.isError);
  if (loading) return <State title="Loading intelligence analytics" description="Retrieving safe public corpus aggregates." />;
  if (failed || !queries.overview.data || !queries.rankings.data || !queries.movers.data || !queries.changes.data || !queries.history.data || !queries.discovery.data) return <State title="Could not load analytics" description="The public analytics service is currently unavailable." />;

  const { overview, rankings, movers, changes, history, discovery } = {
    overview: queries.overview.data,
    rankings: queries.rankings.data,
    movers: queries.movers.data,
    changes: queries.changes.data,
    history: queries.history.data,
    discovery: queries.discovery.data,
  };
  const selectedSeries = selectedTechnology ? history.series.find((series) => series.slug === selectedTechnology) : undefined;

  return (
    <div className="space-y-8">
      <header className="max-w-3xl">
        <p className="font-mono text-xs font-medium tracking-[.12em] text-primary">INTELLIGENCE ANALYTICS</p>
        <h1 className="mt-3 font-sans text-3xl font-semibold text-on-surface sm:text-4xl">Technology landscape</h1>
        <p className="mt-3 text-sm leading-6 text-on-surface-variant">Public, rebuildable adoption and discovery aggregates. Protected crawl operations and service telemetry are not published here.</p>
      </header>

      <div className="flex flex-wrap gap-2" role="group" aria-label="Analytics history window">
        {([30, 90, 365] as const).map((value) => <Button key={value} size="compact" variant={days === value ? "secondary" : "ghost"} aria-pressed={days === value} onClick={() => onSearchChange({ ...search, ...(value === 30 ? { since_days: undefined } : { since_days: value }) })}>{value} days</Button>)}
      </div>

      <section className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4" aria-label="Corpus summary">
        {[["Domains", overview.domain_count], ["Technologies", overview.technology_count], ["Current detections", overview.current_detection_count], ["Countries", overview.country_count]].map(([label, value]) => <Card key={String(label)}><p className="font-mono text-xs text-on-surface-variant">{label}</p><p className="mt-2 font-sans text-3xl font-semibold text-on-surface">{Number(value).toLocaleString()}</p></Card>)}
      </section>

      <AdoptionCharts history={history} selectedTechnology={selectedTechnology} selectedSeries={selectedSeries} technologies={rankings.technologies} onTechnologyChange={(technology) => onSearchChange({ ...search, technology: technology || undefined })} />
      <ChangeChart days={days} points={changes} />

      <section className="grid gap-4 lg:grid-cols-2"><Ranking title="Top technologies" items={rankings.technologies} link="technology" /><Ranking title="Top providers" items={rankings.providers} link="provider" /><Ranking title="Top categories" items={rankings.categories} /><Card><h2 className="font-sans text-lg font-semibold">Top countries</h2><ol className="mt-4 space-y-3">{rankings.countries.map((item) => <li key={item.country_code} className="flex justify-between"><span className="font-mono text-sm">{item.country_code}</span><span>{item.count.toLocaleString()} domains</span></li>)}</ol></Card></section>
      <section className="grid gap-4 lg:grid-cols-2"><Movers title="Growing technologies" items={movers.growing_technologies} /><Movers title="Declining technologies" items={movers.declining_technologies} /></section>
      <Discovery discovery={discovery} />
    </div>
  );
}

function AdoptionCharts({ history, selectedTechnology, selectedSeries, technologies, onTechnologyChange }: { history: PublicAdoptionHistory; selectedTechnology?: string; selectedSeries?: PublicAdoptionHistory["series"][number]; technologies: { slug: string; display_name: string }[]; onTechnologyChange: (technology: string) => void }) {
  const insufficient = history.status === "insufficient_history";
  const comparisonSeries = history.series.slice(0, 5);
  return <section className="space-y-4" aria-label="Technology adoption history"><ChartFrame title="Technology adoption" description={insufficient ? "There are not yet two captured daily observations in this window. The chart will become available as daily snapshots accumulate." : `Daily domain counts from ${history.available_from ?? "the first available day"} through ${history.available_to ?? "the latest available day"}. Gaps indicate a day without a captured projection.`}>{insufficient ? <p role="status" className="text-sm text-on-surface-variant">Insufficient history for a trend. Current adoption rankings remain available below.</p> : <><AdoptionLineChart series={comparisonSeries} /><AdoptionTable series={comparisonSeries} caption="Daily adoption counts for the comparison technologies" /></>}</ChartFrame><ChartFrame title="Technology detail" description="Select a technology to retain a focused series in the URL while the comparison chart remains available."><label className="block max-w-sm"><span className="mb-2 block font-mono text-xs text-on-surface-variant">Technology</span><select className="min-h-10 w-full rounded-md bg-surface-container-low px-3 text-on-surface outline-none focus-visible:ring-1 focus-visible:ring-primary" value={selectedTechnology ?? ""} onChange={(event) => onTechnologyChange(event.target.value)}><option value="">Choose a technology</option>{technologies.map((technology) => <option key={technology.slug} value={technology.slug}>{technology.display_name}</option>)}</select></label>{selectedTechnology && !selectedSeries ? <p role="status" className="mt-5 text-sm text-on-surface-variant">No captured adoption series is available for the selected technology in this window.</p> : selectedSeries && !insufficient ? <div className="mt-5"><AdoptionLineChart series={[selectedSeries]} /><AdoptionTable series={[selectedSeries]} caption={`Daily adoption counts for ${selectedSeries.display_name}`} /></div> : <p className="mt-5 text-sm text-on-surface-variant">Choose a technology after daily history is available to view its detail.</p>}</ChartFrame></section>;
}

function AdoptionLineChart({ series }: { series: PublicAdoptionHistory["series"] }) {
  const reducedMotion = useReducedMotion();
  const days = [...new Set(series.flatMap((item) => item.points.map((point) => point.day)))].sort();
  const option: EChartsOption = { animation: !reducedMotion, animationDuration: 300, aria: { enabled: true, description: "Line chart showing daily technology adoption counts." }, color: chartPalette, tooltip: { trigger: "axis" }, grid: { left: 48, right: 20, top: 32, bottom: 42 }, legend: { textStyle: { color: "#bbc9c6" }, bottom: 0 }, xAxis: { type: "category", data: days, axisLabel: { color: "#bbc9c6" }, axisLine: { lineStyle: { color: "#3c4947" } } }, yAxis: { type: "value", minInterval: 1, axisLabel: { color: "#bbc9c6" }, splitLine: { lineStyle: { color: "#3c4947" } } }, series: series.map((item) => ({ name: item.display_name, type: "line", showSymbol: false, connectNulls: false, data: days.map((day) => item.points.find((point) => point.day === day)?.count ?? null), emphasis: { focus: "series" } })) };
  return <AnalyticsChart option={option} height={320} />;
}

function AdoptionTable({ series, caption }: { series: PublicAdoptionHistory["series"]; caption: string }) {
  const days = [...new Set(series.flatMap((item) => item.points.map((point) => point.day)))].sort();
  return <div className="mt-5 overflow-x-auto" tabIndex={0} aria-label="Scrollable daily adoption counts table"><table className="w-full min-w-[32rem] text-left text-sm"><caption className="sr-only">{caption}</caption><thead className="border-b border-outline-variant/30 font-mono text-xs text-on-surface-variant"><tr><th className="px-2 py-2 font-medium">Day</th>{series.map((item) => <th key={item.slug} className="px-2 py-2 font-medium">{item.display_name}</th>)}</tr></thead><tbody>{days.map((day) => <tr key={day} className="border-b border-outline-variant/20"><td className="px-2 py-2 font-mono text-xs">{day}</td>{series.map((item) => <td key={item.slug} className="px-2 py-2">{item.points.find((point) => point.day === day)?.count.toLocaleString() ?? "No capture"}</td>)}</tr>)}</tbody></table></div>;
}

function ChangeChart({ days, points }: { days: number; points: PublicChangeTrend[] }) {
  const reducedMotion = useReducedMotion();
  const total = points.reduce((sum, point) => sum + point.added + point.removed + point.migrated, 0);
  const option: EChartsOption = { animation: !reducedMotion, animationDuration: 300, aria: { enabled: true, description: "Stacked bar chart showing daily additions, migrations, and removals." }, color: ["#5bdacb", "#ffb59d", "#ffb4ab"], tooltip: { trigger: "axis", axisPointer: { type: "shadow" } }, legend: { textStyle: { color: "#bbc9c6" }, bottom: 0 }, grid: { left: 48, right: 20, top: 24, bottom: 42 }, xAxis: { type: "category", data: points.map((point) => point.day), axisLabel: { color: "#bbc9c6", hideOverlap: true }, axisLine: { lineStyle: { color: "#3c4947" } } }, yAxis: { type: "value", minInterval: 1, axisLabel: { color: "#bbc9c6" }, splitLine: { lineStyle: { color: "#3c4947" } } }, series: [{ name: "Added", type: "bar", stack: "changes", data: points.map((point) => point.added) }, { name: "Migrated", type: "bar", stack: "changes", data: points.map((point) => point.migrated) }, { name: "Removed", type: "bar", stack: "changes", data: points.map((point) => point.removed) }] };
  return <ChartFrame title="Stack changes" description={`${total.toLocaleString()} additions, removals, or migrations observed in the last ${days} days.`}>{points.length ? <><AnalyticsChart option={option} height={300} /><div className="mt-5 overflow-x-auto" tabIndex={0} aria-label="Scrollable daily stack change counts table"><table className="w-full min-w-[34rem] text-left text-sm"><caption className="sr-only">Daily stack change counts</caption><thead className="border-b border-outline-variant/30 font-mono text-xs text-on-surface-variant"><tr><th className="px-2 py-2">Day</th><th className="px-2 py-2">Added</th><th className="px-2 py-2">Migrated</th><th className="px-2 py-2">Removed</th></tr></thead><tbody>{points.map((point) => <tr key={point.day} className="border-b border-outline-variant/20"><td className="px-2 py-2 font-mono text-xs">{point.day}</td><td className="px-2 py-2">{point.added.toLocaleString()}</td><td className="px-2 py-2">{point.migrated.toLocaleString()}</td><td className="px-2 py-2">{point.removed.toLocaleString()}</td></tr>)}</tbody></table></div></> : <p role="status" className="text-sm text-on-surface-variant">No changes were observed in this window.</p>}</ChartFrame>;
}

function AnalyticsChart({ option, height }: { option: EChartsOption; height: number }) {
  return <Suspense fallback={<div role="status" className="flex items-center justify-center text-sm text-on-surface-variant" style={{ height }}>Loading chart…</div>}><ECharts option={option} style={{ height, width: "100%" }} opts={{ renderer: "svg" }} /></Suspense>;
}

function Discovery({ discovery }: { discovery: { large_migrations: { from_slug: string; from_display_name: string; to_slug: string; to_display_name: string; domain_count: number }[]; newest_domains: { canonical_domain: string; first_indexed_at: string }[]; frequently_crawled_domains: { canonical_domain: string; crawl_count: number; last_crawled_at: string }[] } }) {
  return <section className="grid gap-4 lg:grid-cols-3" aria-label="Public discovery aggregates"><DiscoveryCard title="Largest migrations">{discovery.large_migrations.length ? <ol className="space-y-3">{discovery.large_migrations.map((item) => <li key={`${item.from_slug}-${item.to_slug}`}><p><Link to="/technologies/$technology" params={{ technology: item.from_slug }} className="text-primary">{item.from_display_name}</Link><span className="text-on-surface-variant"> → </span><Link to="/technologies/$technology" params={{ technology: item.to_slug }} className="text-primary">{item.to_display_name}</Link></p><p className="mt-1 font-mono text-xs text-on-surface-variant">{item.domain_count.toLocaleString()} observed domains</p></li>)}</ol> : <p className="text-sm text-on-surface-variant">No migrations were observed in this window.</p>}</DiscoveryCard><DiscoveryCard title="Newest indexed domains">{discovery.newest_domains.length ? <ol className="space-y-3">{discovery.newest_domains.map((item) => <li key={item.canonical_domain}><Link to="/domains/$domain" params={{ domain: item.canonical_domain }} className="font-mono text-sm text-primary">{item.canonical_domain}</Link><p className="mt-1 text-xs text-on-surface-variant">First observed {formatDate(item.first_indexed_at)}</p></li>)}</ol> : <p className="text-sm text-on-surface-variant">No public crawl snapshots are available yet.</p>}</DiscoveryCard><DiscoveryCard title="Frequently crawled domains">{discovery.frequently_crawled_domains.length ? <ol className="space-y-3">{discovery.frequently_crawled_domains.map((item) => <li key={item.canonical_domain}><Link to="/domains/$domain" params={{ domain: item.canonical_domain }} className="font-mono text-sm text-primary">{item.canonical_domain}</Link><p className="mt-1 text-xs text-on-surface-variant">{item.crawl_count.toLocaleString()} public snapshots · latest {formatDate(item.last_crawled_at)}</p></li>)}</ol> : <p className="text-sm text-on-surface-variant">No public crawl history is available in this window.</p>}</DiscoveryCard></section>;
}

function DiscoveryCard({ title, children }: { title: string; children: React.ReactNode }) { return <Card><h2 className="font-sans text-lg font-semibold">{title}</h2><div className="mt-4">{children}</div></Card>; }
function Ranking({ title, items, link }: { title: string; items: { slug: string; display_name: string; count: number }[]; link?: "technology" | "provider" }) { return <Card><h2 className="font-sans text-lg font-semibold">{title}</h2><ol className="mt-4 space-y-3">{items.map((item) => <li key={item.slug} className="flex justify-between gap-3"><span>{link === "technology" ? <Link to="/technologies/$technology" params={{ technology: item.slug }} className="text-primary">{item.display_name}</Link> : link === "provider" ? <Link to="/providers/$provider" params={{ provider: item.slug }} className="text-primary">{item.display_name}</Link> : item.display_name}</span><span className="font-mono text-xs text-on-surface-variant">{item.count.toLocaleString()}</span></li>)}</ol></Card>; }
function Movers({ title, items }: { title: string; items: { slug: string; display_name: string; net_change: number; adoption_count: number }[] }) { return <Card><h2 className="font-sans text-lg font-semibold">{title}</h2><ol className="mt-4 space-y-3">{items.length ? items.map((item) => <li key={item.slug} className="flex justify-between"><Link to="/technologies/$technology" params={{ technology: item.slug }} className="text-primary">{item.display_name}</Link><span className="font-mono text-sm">{item.net_change > 0 ? "+" : ""}{item.net_change}</span></li>) : <li className="text-sm text-on-surface-variant">No observed movement in this window.</li>}</ol></Card>; }
function State({ title, description }: { title: string; description: string }) { return <Card role="status" className="min-h-64"><h1 className="font-sans text-2xl font-semibold">{title}</h1><p className="mt-2 text-on-surface-variant">{description}</p></Card>; }
function formatDate(value: string): string { const date = new Date(value); return Number.isNaN(date.valueOf()) ? value : new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(date); }
function useReducedMotion(): boolean { const [reduced, setReduced] = useState(false); useEffect(() => { const query = window.matchMedia("(prefers-reduced-motion: reduce)"); const update = () => setReduced(query.matches); update(); query.addEventListener("change", update); return () => query.removeEventListener("change", update); }, []); return reduced; }
