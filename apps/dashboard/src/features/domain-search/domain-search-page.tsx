import { ArrowLeft, ArrowRight, ArrowSquareOut, CaretDown, MagnifyingGlass } from "@phosphor-icons/react";
import { Badge, Button, Card, Input } from "@techatlas/ui";
import type { FormEvent } from "react";
import { useState } from "react";
import type { PublicDomainSearchHit, PublicDomainSearchPage } from "@techatlas/api-client";
import { DomainSearchFilters } from "./domain-search-filters";
import { DomainSearchError, useDomainSearch } from "./domain-search-query";
import type { DomainSearchSort, DomainSearchState } from "./search-state";
import { searchWithOffset, validateDomainSearchState } from "./search-state";

type DomainSearchPageProps = {
  search: DomainSearchState;
  onSearchChange: (search: DomainSearchState) => void;
  presentation?: {
    eyebrow: string;
    title: string;
    description: string;
  };
};

const selectClassName = "min-h-10 w-full appearance-none rounded-md bg-surface-container-low px-3 pr-10 font-sans text-ui text-on-surface outline-none focus-visible:ring-1 focus-visible:ring-primary";

const defaultPresentation = {
  eyebrow: "DOMAIN INTELLIGENCE",
  title: "Search the technology landscape",
  description: "Explore public technology observations across the TechAtlas corpus. Search state stays in the URL so a result view can be shared exactly.",
};

export function DomainSearchPage({ search, onSearchChange, presentation = defaultPresentation }: DomainSearchPageProps) {
  const searchQuery = useDomainSearch(search);

  return (
    <div className="space-y-8">
      <header className="max-w-3xl">
        <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">{presentation.eyebrow}</p>
        <h1 className="mt-3 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">{presentation.title}</h1>
        <p className="mt-3 text-sm leading-6 text-on-surface-variant">{presentation.description}</p>
      </header>

      <SearchForm key={JSON.stringify(search)} search={search} facets={searchQuery.data?.facets} onSearchChange={onSearchChange} />

      <SearchResults search={search} query={searchQuery} onPageChange={(offset) => onSearchChange(searchWithOffset(search, offset))} />
    </div>
  );
}

function SearchForm({ search, facets, onSearchChange }: DomainSearchPageProps & { facets: PublicDomainSearchPage["facets"] | undefined }) {
  const [query, setQuery] = useState(search.q ?? "");

  const submitSearch = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSearchChange(validateDomainSearchState({ ...search, q: query, offset: undefined }));
  };

  return (
    <Card className="p-4 sm:p-6">
      <form className="space-y-5" onSubmit={submitSearch}>
        <div className="flex flex-col gap-3 sm:flex-row">
          <label className="min-w-0 flex-1"><span className="sr-only">Search domains</span><span className="relative block"><MagnifyingGlass className="pointer-events-none absolute left-3 top-1/2 size-5 -translate-y-1/2 text-on-surface-variant" aria-hidden="true" /><Input value={query} className="pl-11" placeholder="Search domains or observed technologies" onChange={(event) => setQuery(event.target.value)} /></span></label>
          <Button type="submit" className="shrink-0"><MagnifyingGlass size={16} weight="bold" aria-hidden="true" />Search</Button>
        </div>
        <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_12rem] sm:items-end">
          <DomainSearchFilters search={search} facets={facets} onSearchChange={onSearchChange} />
          <label className="block">
            <span className="mb-2 block font-mono text-xs text-on-surface-variant">Sort results</span>
            <span className="relative block">
              <select aria-label="Sort results" className={selectClassName} value={search.sort ?? "relevance"} onChange={(event) => onSearchChange(validateDomainSearchState({ ...search, sort: event.target.value === "relevance" ? undefined : event.target.value as DomainSearchSort, offset: undefined }))}>
                <option value="relevance">Relevance</option>
                <option value="last_crawled_desc">Last successful crawl</option>
                <option value="updated_desc">Recently updated</option>
              </select>
              <CaretDown className="pointer-events-none absolute right-3 top-1/2 size-4 -translate-y-1/2 text-on-surface-variant" aria-hidden="true" />
            </span>
          </label>
        </div>
      </form>
    </Card>
  );
}

function SearchResults({ search, query, onPageChange }: { search: DomainSearchState; query: ReturnType<typeof useDomainSearch>; onPageChange: (offset: number) => void }) {
  if (query.isLoading) {
    return <SearchState title="Searching the corpus" description="Retrieving current domain observations." />;
  }

  if (query.isError) {
    const validation = query.error instanceof DomainSearchError && query.error.status === 422;
    return <SearchState title={validation ? "Check the search filters" : "Could not load domain search"} description={validation ? "One or more filters are not supported by the search index." : "The public search index is currently unavailable. Please try again."} action={<Button variant="secondary" onClick={() => void query.refetch()}>Try again</Button>} />;
  }

  if (!query.data) {
    return <SearchState title="Could not load domain search" description="The public search index did not return a result page. Please try again." action={<Button variant="secondary" onClick={() => void query.refetch()}>Try again</Button>} />;
  }

  const page = query.data;
  if (page.results.length === 0) {
    return <SearchState title="No matching domains" description={hasQuery(search) ? "Try broadening the search or removing a filter." : "No domains have been indexed yet."} />;
  }

  return <ResultPage page={page} onPageChange={onPageChange} />;
}

function ResultPage({ page, onPageChange }: { page: PublicDomainSearchPage; onPageChange: (offset: number) => void }) {
  const firstResult = page.offset + 1;
  const finalResult = page.offset + page.results.length;
  const canGoBack = page.offset > 0;
  const canGoForward = finalResult < page.estimated_total_hits;

  return (
    <section aria-labelledby="search-results-heading">
      <div className="mb-4 flex flex-col justify-between gap-3 sm:flex-row sm:items-end">
        <div>
          <p className="font-mono text-xs font-medium tracking-[0.1em] text-primary">SEARCH RESULTS</p>
          <h2 id="search-results-heading" className="mt-2 font-sans text-2xl font-semibold text-on-surface">{formatNumber(page.estimated_total_hits)} estimated domains</h2>
        </div>
        <p className="font-mono text-[0.6875rem] text-on-surface-variant">Queried {formatTimestamp(page.query_at)}</p>
      </div>

      <div className="hidden overflow-hidden rounded-xl border border-outline-variant/30 md:block">
        <table className="w-full text-left">
          <caption className="sr-only">Matching domain intelligence records</caption>
          <thead className="bg-surface-container-low font-mono text-[0.6875rem] uppercase tracking-[0.08em] text-on-surface-variant">
            <tr>
              <th className="px-5 py-3 font-medium">Domain</th>
              <th className="px-5 py-3 font-medium">Technologies</th>
              <th className="px-5 py-3 font-medium">Highest confidence</th>
              <th className="px-5 py-3 font-medium">Location</th>
              <th className="px-5 py-3 font-medium">Last successful crawl</th>
              <th className="px-5 py-3 font-medium">Updated</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-outline-variant/30">
            {page.results.map((result) => <DomainResultRow key={result.canonical_domain} result={result} />)}
          </tbody>
        </table>
      </div>

      <div className="space-y-3 md:hidden">
        {page.results.map((result) => <DomainResultCard key={result.canonical_domain} result={result} />)}
      </div>

      <nav className="mt-5 flex flex-wrap items-center justify-between gap-3" aria-label="Search result pagination">
        <p className="font-mono text-xs text-on-surface-variant">Showing {formatNumber(firstResult)}–{formatNumber(finalResult)} of {formatNumber(page.estimated_total_hits)}</p>
        <div className="flex gap-2">
          <Button variant="secondary" size="compact" disabled={!canGoBack} onClick={() => onPageChange(Math.max(0, page.offset - page.limit))}>
            <ArrowLeft size={15} aria-hidden="true" />
            Previous
          </Button>
          <Button variant="secondary" size="compact" disabled={!canGoForward} onClick={() => onPageChange(page.offset + page.limit)}>
            Next
            <ArrowRight size={15} aria-hidden="true" />
          </Button>
        </div>
      </nav>
    </section>
  );
}

function DomainResultRow({ result }: { result: PublicDomainSearchHit }) {
  return (
    <tr className="bg-surface transition-colors hover:bg-surface-container-low">
      <td className="px-5 py-4 align-top"><DomainLink domain={result.canonical_domain} /></td>
      <td className="max-w-sm px-5 py-4 align-top"><TechnologyBadges slugs={result.technology_slugs} /><CategorySummary slugs={result.category_slugs} /></td>
      <td className="px-5 py-4 align-top"><Confidence confidence={result.max_confidence} /></td>
      <td className="px-5 py-4 align-top font-mono text-xs text-on-surface-variant">{result.country_code ?? "Country unavailable"}</td>
      <td className="px-5 py-4 align-top font-mono text-xs text-on-surface-variant">{formatCrawlTimestamp(result.last_crawled_at)}</td>
      <td className="px-5 py-4 align-top font-mono text-xs text-on-surface-variant">{formatTimestamp(result.updated_at)}</td>
    </tr>
  );
}

function DomainResultCard({ result }: { result: PublicDomainSearchHit }) {
  return (
    <Card className="p-4">
      <DomainLink domain={result.canonical_domain} />
      <div className="mt-4"><TechnologyBadges slugs={result.technology_slugs} /><CategorySummary slugs={result.category_slugs} /></div>
      <dl className="mt-4 grid grid-cols-2 gap-3 border-t border-outline-variant/30 pt-4 font-mono text-xs">
        <div><dt className="text-on-surface-variant">Highest confidence</dt><dd className="mt-1"><Confidence confidence={result.max_confidence} /></dd></div>
        <div><dt className="text-on-surface-variant">Location</dt><dd className="mt-1 text-on-surface">{result.country_code ?? "Country unavailable"}</dd></div>
        <div><dt className="text-on-surface-variant">Last successful crawl</dt><dd className="mt-1 text-on-surface">{formatCrawlTimestamp(result.last_crawled_at)}</dd></div>
        <div className="col-span-2"><dt className="text-on-surface-variant">Updated</dt><dd className="mt-1 text-on-surface">{formatTimestamp(result.updated_at)}</dd></div>
      </dl>
    </Card>
  );
}

function DomainLink({ domain }: { domain: string }) {
  return <a href={`/domains/${encodeURIComponent(domain)}`} className="inline-flex items-center gap-1 font-mono text-sm font-medium text-primary hover:text-primary-fixed focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{domain}<ArrowSquareOut size={15} aria-hidden="true" /></a>;
}

function TechnologyBadges({ slugs }: { slugs: string[] }) {
  if (slugs.length === 0) {
    return <span className="font-mono text-xs text-on-surface-variant">No current technology tags</span>;
  }
  return <div className="flex flex-wrap gap-1.5">{slugs.slice(0, 5).map((slug) => <Badge key={slug} tone="discovery">{slug}</Badge>)}{slugs.length > 5 ? <Badge tone="neutral">+{slugs.length - 5}</Badge> : null}</div>;
}

function CategorySummary({ slugs }: { slugs: string[] }) {
  return slugs.length > 0 ? <p className="mt-2 font-mono text-[0.625rem] text-on-surface-variant">{slugs.join(" · ")}</p> : null;
}

function Confidence({ confidence }: { confidence: number }) {
  const value = Math.max(0, Math.min(100, confidence));
  return <span className="font-mono text-xs text-primary" aria-label={`Highest confidence ${value}%`}>{value}%</span>;
}

function SearchState({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <Card className="flex min-h-64 flex-col items-start justify-center gap-4" role="status"><div><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">DOMAIN INTELLIGENCE</p><h2 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h2><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></div>{action}</Card>;
}

function hasQuery(search: DomainSearchState): boolean {
  return Boolean(search.q || search.technology?.length || search.category?.length || search.country?.length || search.crawled_since || search.min_confidence !== undefined);
}

function formatTimestamp(value: string | null | undefined): string {
  if (!value) {
    return "Unknown";
  }
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.valueOf()) ? "Unknown" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(timestamp);
}

function formatCrawlTimestamp(value: string | null | undefined): string {
  return value ? formatTimestamp(value) : "No successful crawl yet";
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}
