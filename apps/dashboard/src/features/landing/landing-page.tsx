import { ArrowRight, ChartLine, MagnifyingGlass, Stack } from "@phosphor-icons/react";
import { Button, Card } from "@techatlas/ui";
import { Link } from "@tanstack/react-router";
import { analyticsMovers, analyticsOverview, analyticsRankings, type PublicAnalyticsRank } from "@techatlas/api-client";
import { useQuery } from "@tanstack/react-query";

class LandingDataError extends Error {}

async function read<T>(request: Promise<{ data?: T; error?: unknown }>): Promise<T> {
  const result = await request;
  if (result.error || !result.data) throw new LandingDataError();
  return result.data;
}

export function LandingPage() {
  const overview = useQuery({ queryKey: ["public", "landing", "overview"], queryFn: () => read(analyticsOverview()) });
  const rankings = useQuery({ queryKey: ["public", "landing", "rankings"], queryFn: () => read(analyticsRankings()) });
  const movers = useQuery({ queryKey: ["public", "landing", "movers"], queryFn: () => read(analyticsMovers({ query: { since_days: 30 } })) });
  const loading = overview.isLoading || rankings.isLoading || movers.isLoading;
  const unavailable = overview.isError || rankings.isError || movers.isError || !overview.data || !rankings.data || !movers.data;

  return (
    <div className="space-y-8">
      <section className="grid gap-6 rounded-xl border border-outline-variant/30 bg-surface-container-low p-6 lg:grid-cols-[minmax(0,1.4fr)_minmax(18rem,0.6fr)] lg:p-8">
        <div className="max-w-3xl">
          <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">PUBLIC TECHNOLOGY INTELLIGENCE</p>
          <h1 className="mt-3 font-sans text-4xl font-semibold tracking-[-0.03em] text-on-surface sm:text-5xl">See the technology landscape as it changes.</h1>
          <p className="mt-4 max-w-2xl text-base leading-7 text-on-surface-variant">Search evidence-backed observations, compare domains, and follow public adoption trends built from immutable crawl snapshots.</p>
          <div className="mt-6 flex flex-wrap gap-3">
            <Link to="/domains"><Button><MagnifyingGlass size={17} aria-hidden="true" />Explore domains</Button></Link>
            <Link to="/analytics"><Button variant="secondary"><ChartLine size={17} aria-hidden="true" />View analytics</Button></Link>
          </div>
        </div>
        <Card className="border-primary/25 bg-background/60">
          <p className="font-mono text-xs text-primary">HOW TO USE THE INDEX</p>
          <ol className="mt-4 space-y-3 text-sm leading-6 text-on-surface-variant">
            <li><span className="font-mono text-primary">01</span> Search a domain or technology.</li>
            <li><span className="font-mono text-primary">02</span> Inspect the latest public evidence.</li>
            <li><span className="font-mono text-primary">03</span> Compare only what the data supports.</li>
          </ol>
        </Card>
      </section>

      {loading ? <LandingState title="Loading public intelligence" description="Retrieving the latest public corpus aggregates." /> : unavailable ? <LandingState title="Public intelligence is temporarily unavailable" description="The public aggregate service could not be reached. Domain search remains available." /> : <LandingData overview={overview.data} rankings={rankings.data.technologies} movers={movers.data.growing_technologies} />}
    </div>
  );
}

function LandingData({ overview, rankings, movers }: { overview: { domain_count: number; technology_count: number; current_detection_count: number; country_count: number }; rankings: PublicAnalyticsRank[]; movers: { slug: string; display_name: string; net_change: number }[] }) {
  return <><section className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4" aria-label="Public corpus summary">{[["Observed domains", overview.domain_count], ["Technologies", overview.technology_count], ["Current detections", overview.current_detection_count], ["Countries", overview.country_count]].map(([label, value]) => <Card key={String(label)}><p className="font-mono text-xs text-on-surface-variant">{label}</p><p className="mt-2 font-sans text-3xl font-semibold text-on-surface">{Number(value).toLocaleString()}</p></Card>)}</section><section className="grid gap-4 lg:grid-cols-2"><Card><div className="flex items-center justify-between gap-3"><div><p className="font-mono text-xs text-primary">SELECTED DISCOVERIES</p><h2 className="mt-2 font-sans text-xl font-semibold">Top observed technologies</h2></div><Stack size={22} className="text-discovery" aria-hidden="true" /></div><ol className="mt-5 space-y-3">{rankings.slice(0, 5).map((item) => <li key={item.slug} className="flex items-center justify-between gap-4"><Link to="/technologies/$technology" params={{ technology: item.slug }} className="text-primary hover:text-primary-fixed focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{item.display_name}</Link><span className="font-mono text-xs text-on-surface-variant">{item.count.toLocaleString()}</span></li>)}</ol><Link to="/technologies" className="mt-5 inline-flex items-center gap-2 font-mono text-xs text-primary hover:text-primary-fixed">Browse technologies <ArrowRight size={14} aria-hidden="true" /></Link></Card><Card><div className="flex items-center justify-between gap-3"><div><p className="font-mono text-xs text-primary">RECENT TRENDS</p><h2 className="mt-2 font-sans text-xl font-semibold">Growing in 30 days</h2></div><ChartLine size={22} className="text-primary" aria-hidden="true" /></div><ol className="mt-5 space-y-3">{movers.length ? movers.slice(0, 5).map((item) => <li key={item.slug} className="flex items-center justify-between gap-4"><Link to="/technologies/$technology" params={{ technology: item.slug }} className="text-primary hover:text-primary-fixed focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{item.display_name}</Link><span className="font-mono text-xs text-on-surface-variant">+{item.net_change.toLocaleString()}</span></li>) : <li className="text-sm text-on-surface-variant">No technology changes were observed in the last 30 days.</li>}</ol><Link to="/analytics" className="mt-5 inline-flex items-center gap-2 font-mono text-xs text-primary hover:text-primary-fixed">Explore analytics <ArrowRight size={14} aria-hidden="true" /></Link></Card></section></>;
}

function LandingState({ title, description }: { title: string; description: string }) {
  return <Card role="status"><h2 className="font-sans text-xl font-semibold text-on-surface">{title}</h2><p className="mt-2 text-sm leading-6 text-on-surface-variant">{description}</p></Card>;
}
