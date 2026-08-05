import { CaretDown, TrendDown, TrendUp } from "@phosphor-icons/react";
import { Badge, Button, Card } from "@techatlas/ui";
import type { PublicTechnologyCategory, PublicTechnologyLibraryItem } from "@techatlas/api-client";
import { Link } from "@tanstack/react-router";
import { TechnologyLibraryError, useTechnologyLibrary } from "./technology-library-query";
import {
  currentTechnologyLibraryView,
  type TechnologyLibraryState,
  type TechnologyLibraryTrend,
  validateTechnologyLibraryState,
} from "./technology-library-state";

type TechnologyLibraryPageProps = {
  search: TechnologyLibraryState;
  onSearchChange: (search: TechnologyLibraryState) => void;
};

const selectClassName = "min-h-10 w-full appearance-none rounded-md bg-surface-container-low px-3 pr-10 font-sans text-ui text-on-surface outline-none focus-visible:ring-1 focus-visible:ring-primary";

export function TechnologyLibraryPage({ search, onSearchChange }: TechnologyLibraryPageProps) {
  const libraryQuery = useTechnologyLibrary(search);
  const categories = libraryQuery.data?.pages[0]?.categories ?? [];
  const view = currentTechnologyLibraryView(search);

  return (
    <div className="space-y-8">
      <header className="max-w-3xl">
        <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">TECHNOLOGY LIBRARY</p>
        <h1 className="mt-3 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">Browse the technology landscape</h1>
        <p className="mt-3 text-sm leading-6 text-on-surface-variant">Explore observed technologies by category and their net movement over the past 30 days. Filters and the display mode stay in the URL for a shareable view.</p>
      </header>

      <TechnologyControls
        categories={categories}
        search={search}
        view={view}
        onSearchChange={onSearchChange}
      />

      <TechnologyResults query={libraryQuery} search={search} view={view} />
    </div>
  );
}

function TechnologyControls({
  categories,
  search,
  view,
  onSearchChange,
}: TechnologyLibraryPageProps & { categories: PublicTechnologyCategory[]; view: "grid" | "list" }) {
  const update = (next: Record<string, unknown>) => onSearchChange(validateTechnologyLibraryState({ ...search, ...next }));

  return (
    <Card className="grid gap-4 p-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] sm:items-end sm:p-5">
      <SelectControl
        label="Category"
        value={search.category ?? ""}
        onChange={(category) => update({ category: category || undefined })}
      >
        <option value="">All categories</option>
        {categories.map((category) => <option key={category.slug} value={category.slug}>{category.display_name}</option>)}
      </SelectControl>
      <SelectControl
        label="Trend"
        value={search.trend ?? ""}
        onChange={(trend) => update({ trend: trend || undefined })}
      >
        <option value="">All trends</option>
        <option value="growing">Growing</option>
        <option value="declining">Declining</option>
        <option value="stable">Stable</option>
      </SelectControl>
      <div className="sm:justify-self-end">
        <span className="mb-2 block font-mono text-xs text-on-surface-variant">Display</span>
        <div className="flex rounded-md bg-surface-container-low p-1" role="group" aria-label="Technology library display">
          <Button variant={view === "grid" ? "secondary" : "ghost"} size="compact" aria-pressed={view === "grid"} onClick={() => update({ view: undefined })}>Grid</Button>
          <Button variant={view === "list" ? "secondary" : "ghost"} size="compact" aria-pressed={view === "list"} onClick={() => update({ view: "list" })}>List</Button>
        </div>
      </div>
    </Card>
  );
}

function SelectControl({ label, value, onChange, children }: { label: string; value: string; onChange: (value: string) => void; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="mb-2 block font-mono text-xs text-on-surface-variant">{label}</span>
      <span className="relative block">
        <select aria-label={label} className={selectClassName} value={value} onChange={(event) => onChange(event.target.value)}>{children}</select>
        <CaretDown className="pointer-events-none absolute right-3 top-1/2 size-4 -translate-y-1/2 text-on-surface-variant" aria-hidden="true" />
      </span>
    </label>
  );
}

function TechnologyResults({ query, search, view }: { query: ReturnType<typeof useTechnologyLibrary>; search: TechnologyLibraryState; view: "grid" | "list" }) {
  if (query.isLoading) {
    return <TechnologyState title="Loading the technology library" description="Retrieving the latest public technology observations." />;
  }
  if (query.isError) {
    const validation = query.error instanceof TechnologyLibraryError && query.error.status === 422;
    return <TechnologyState title={validation ? "Check the library filters" : "Could not load the technology library"} description={validation ? "One or more URL filters are not supported." : "The public technology service is currently unavailable. Please try again."} action={<Button variant="secondary" onClick={() => void query.refetch()}>Try again</Button>} />;
  }
  const pages = query.data?.pages ?? [];
  const technologies = pages.flatMap((page) => page.items);
  if (technologies.length === 0) {
    return <TechnologyState title="No matching technologies" description={search.category || search.trend ? "Try removing a filter to see more technologies." : "No technologies have been observed yet."} />;
  }

  return (
    <section aria-labelledby="technology-results-heading">
      <div className="mb-4 flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="font-mono text-xs font-medium tracking-[0.1em] text-primary">OBSERVED TECHNOLOGIES</p>
          <h2 id="technology-results-heading" className="mt-2 font-sans text-2xl font-semibold text-on-surface">{formatNumber(technologies.length)} technologies</h2>
        </div>
        <p className="font-mono text-[0.6875rem] text-on-surface-variant">Trend: net changes in the last 30 days</p>
      </div>
      {view === "grid" ? <TechnologyGrid technologies={technologies} /> : <TechnologyList technologies={technologies} />}
      {query.hasNextPage ? <div className="mt-5 flex justify-center"><Button variant="secondary" disabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{query.isFetchingNextPage ? "Loading technologies…" : "Load more"}</Button></div> : null}
    </section>
  );
}

function TechnologyGrid({ technologies }: { technologies: PublicTechnologyLibraryItem[] }) {
  return <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">{technologies.map((technology) => <TechnologyCard key={technology.slug} technology={technology} />)}</div>;
}

function TechnologyCard({ technology }: { technology: PublicTechnologyLibraryItem }) {
  return (
    <Card className="flex min-h-48 flex-col p-5">
      <div className="flex items-start justify-between gap-3"><div><Link to="/technologies/$technology" params={{ technology: technology.slug }} className="font-sans text-xl font-semibold text-on-surface hover:text-primary focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{technology.display_name}</Link><p className="mt-1 font-mono text-xs text-on-surface-variant">{technology.slug}</p></div><Badge tone="discovery">{technology.category_name}</Badge></div>
      <TechnologyMetrics technology={technology} className="mt-auto pt-6" />
    </Card>
  );
}

function TechnologyList({ technologies }: { technologies: PublicTechnologyLibraryItem[] }) {
  return (
    <Card className="overflow-hidden">
      <ul className="divide-y divide-outline-variant/30" aria-label="Technology results">
        {technologies.map((technology) => <li key={technology.slug} className="grid gap-4 p-5 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center"><div><Link to="/technologies/$technology" params={{ technology: technology.slug }} className="font-sans text-lg font-semibold text-on-surface hover:text-primary focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{technology.display_name}</Link><p className="mt-1 font-mono text-xs text-on-surface-variant">{technology.category_name} · {technology.slug}</p></div><TechnologyMetrics technology={technology} className="sm:justify-end" /></li>)}
      </ul>
    </Card>
  );
}

function TechnologyMetrics({ technology, className }: { technology: PublicTechnologyLibraryItem; className?: string }) {
  return (
    <dl className={`flex flex-wrap gap-x-6 gap-y-3 ${className ?? ""}`}>
      <div><dt className="font-mono text-[0.6875rem] text-on-surface-variant">Adoption</dt><dd className="mt-1 font-mono text-sm text-on-surface">{formatNumber(technology.adoption_count)} domains</dd></div>
      <div><dt className="font-mono text-[0.6875rem] text-on-surface-variant">30-day net change</dt><dd className="mt-1 flex items-center gap-2"><TrendBadge trend={technology.trend} /><span className="font-mono text-sm text-on-surface">{formatChange(technology.net_change)}</span></dd></div>
    </dl>
  );
}

function TrendBadge({ trend }: { trend: TechnologyLibraryTrend }) {
  if (trend === "growing") {
    return <Badge tone="success"><TrendUp size={13} aria-hidden="true" />Growing</Badge>;
  }
  if (trend === "declining") {
    return <Badge tone="warning"><TrendDown size={13} aria-hidden="true" />Declining</Badge>;
  }
  return <Badge tone="neutral">Stable</Badge>;
}

function TechnologyState({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <Card className="flex min-h-64 flex-col items-start justify-center gap-4" role="status"><div><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">TECHNOLOGY LIBRARY</p><h2 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h2><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></div>{action}</Card>;
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}

function formatChange(value: number): string {
  return value > 0 ? `+${value}` : String(value);
}
