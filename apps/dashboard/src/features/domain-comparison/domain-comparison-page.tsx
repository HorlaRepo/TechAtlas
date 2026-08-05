import { ArrowSquareOut, MagnifyingGlass, X } from "@phosphor-icons/react";
import { Badge, Button, Card, Input } from "@techatlas/ui";
import { Link } from "@tanstack/react-router";
import type { FormEvent } from "react";
import { useState } from "react";
import type { PublicComparisonCell, PublicComparisonResponse } from "@techatlas/api-client";
import { ComparisonError, useComparisonDomainSuggestions, useDomainComparison } from "./comparison-query";
import { addComparisonDomain, canCompare, normalizeComparisonDomain, removeComparisonDomain, type ComparisonState } from "./comparison-state";

type DomainComparisonPageProps = {
  search: ComparisonState;
  onSearchChange: (search: ComparisonState) => void;
};

type ComparisonCategory = {
  slug: string;
  cellsByDomain: Map<string, PublicComparisonCell[]>;
};

export function DomainComparisonPage({ search, onSearchChange }: DomainComparisonPageProps) {
  const comparisonQuery = useDomainComparison(search);

  return (
    <div className="space-y-8">
      <header className="max-w-3xl"><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">DOMAIN COMPARISON</p><h1 className="mt-3 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">Compare technology stacks</h1><p className="mt-3 text-sm leading-6 text-on-surface-variant">Compare two to ten public domains by normalized technology category. Selections stay in the URL so a matrix can be shared exactly.</p></header>
      <DomainSelection search={search} onSearchChange={onSearchChange} />
      <ComparisonResults search={search} query={comparisonQuery} />
    </div>
  );
}

function DomainSelection({ search, onSearchChange }: DomainComparisonPageProps) {
  const [manualValue, setManualValue] = useState("");
  const [suggestionQuery, setSuggestionQuery] = useState("");
  const [entryError, setEntryError] = useState<string>();
  const suggestionsQuery = useComparisonDomainSuggestions(suggestionQuery);
  const domains = search.domain ?? [];

  const addDomain = (candidate: string) => {
    const normalized = normalizeComparisonDomain(candidate);
    if (!normalized) {
      setEntryError("Enter a valid domain, such as example.com.");
      return;
    }
    if (domains.includes(normalized)) {
      setEntryError("That domain is already selected.");
      return;
    }
    if (domains.length >= 10) {
      setEntryError("A comparison can include up to 10 domains.");
      return;
    }
    setEntryError(undefined);
    onSearchChange(addComparisonDomain(search, normalized));
  };

  const submitManualDomain = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    addDomain(manualValue);
    if (normalizeComparisonDomain(manualValue)) {
      setManualValue("");
    }
  };

  return (
    <Card className="p-4 sm:p-6">
      <div className="grid gap-6 lg:grid-cols-2">
        <form className="space-y-3" onSubmit={submitManualDomain}>
          <div><label htmlFor="comparison-domain" className="font-mono text-xs text-on-surface-variant">Add a domain</label><div className="mt-2 flex flex-col gap-2 sm:flex-row"><Input id="comparison-domain" value={manualValue} placeholder="example.com" onChange={(event) => setManualValue(event.target.value)} /><Button type="submit" className="shrink-0">Add domain</Button></div></div>
          {entryError ? <p className="text-sm text-tertiary-fixed" role="alert">{entryError}</p> : null}
        </form>
        <div className="space-y-3"><label htmlFor="comparison-search" className="font-mono text-xs text-on-surface-variant">Find an indexed domain</label><div className="relative"><MagnifyingGlass className="pointer-events-none absolute left-3 top-1/2 size-5 -translate-y-1/2 text-on-surface-variant" aria-hidden="true" /><Input id="comparison-search" className="pl-11" value={suggestionQuery} placeholder="Search indexed domains" onChange={(event) => setSuggestionQuery(event.target.value)} /></div><SuggestionList query={suggestionQuery} suggestionsQuery={suggestionsQuery} onSelect={addDomain} /></div>
      </div>
      <div className="mt-6 border-t border-outline-variant/30 pt-5"><div className="flex flex-wrap items-center justify-between gap-3"><p className="font-mono text-xs text-on-surface-variant">{domains.length}/10 domains selected</p><p className="text-sm text-on-surface-variant">Select at least two to compare.</p></div>{domains.length > 0 ? <ul className="mt-3 flex flex-wrap gap-2" aria-label="Selected comparison domains">{domains.map((domain) => <li key={domain}><Badge tone="neutral"><span>{domain}</span><button type="button" className="rounded-sm text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-primary" aria-label={`Remove ${domain} from comparison`} onClick={() => onSearchChange(removeComparisonDomain(search, domain))}><X size={14} aria-hidden="true" /></button></Badge></li>)}</ul> : null}</div>
    </Card>
  );
}

function SuggestionList({ query, suggestionsQuery, onSelect }: { query: string; suggestionsQuery: ReturnType<typeof useComparisonDomainSuggestions>; onSelect: (domain: string) => void }) {
  if (query.trim().length < 2) return <p className="text-sm text-on-surface-variant">Type at least two characters to search the public index.</p>;
  if (suggestionsQuery.isError) return <p className="text-sm text-tertiary-fixed">Domain suggestions are unavailable; you can still add a domain manually.</p>;
  const suggestions = suggestionsQuery.data?.results ?? [];
  if (suggestionsQuery.isLoading) return <p className="text-sm text-on-surface-variant" role="status">Searching indexed domains…</p>;
  if (suggestions.length === 0) return <p className="text-sm text-on-surface-variant">No indexed domains match this search.</p>;
  return <ul className="divide-y divide-outline-variant/30 rounded-md border border-outline-variant/30" aria-label="Domain suggestions">{suggestions.map((suggestion) => <li key={suggestion.canonical_domain}><button type="button" className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left hover:bg-surface-container-low focus-visible:outline-2 focus-visible:outline-primary" onClick={() => onSelect(suggestion.canonical_domain)}><span className="font-mono text-sm text-on-surface">{suggestion.canonical_domain}</span><span className="font-mono text-[0.6875rem] text-on-surface-variant">{suggestion.technology_slugs.slice(0, 2).join(" · ") || "No tags"}</span></button></li>)}</ul>;
}

function ComparisonResults({ search, query }: { search: ComparisonState; query: ReturnType<typeof useDomainComparison> }) {
  if (!canCompare(search)) return <ComparisonStatePanel title="Choose domains to compare" description="Add at least two public domains to build a normalized technology matrix." />;
  if (query.isLoading) return <ComparisonStatePanel title="Building comparison matrix" description="Retrieving current technology observations for the selected domains." />;
  if (query.isError) {
    const validation = query.error instanceof ComparisonError && query.error.status === 422;
    return <ComparisonStatePanel title={validation ? "Check the selected domains" : "Could not load comparison"} description={validation ? "One or more selected domains are invalid or cannot be compared together." : "The public comparison service is currently unavailable. Please try again."} action={<Button variant="secondary" onClick={() => void query.refetch()}>Try again</Button>} />;
  }
  if (!query.data) return <ComparisonStatePanel title="Could not load comparison" description="The public comparison service did not return a category matrix." action={<Button variant="secondary" onClick={() => void query.refetch()}>Try again</Button>} />;
  return <ComparisonMatrix comparison={query.data} />;
}

function ComparisonMatrix({ comparison }: { comparison: PublicComparisonResponse }) {
  const categories = groupComparisonCategories(comparison);
  if (categories.length === 0) return <ComparisonStatePanel title="No comparison categories" description="No normalized categories are currently available for these domains." />;
  return <section aria-labelledby="comparison-matrix-heading"><div className="mb-4 flex flex-col justify-between gap-3 sm:flex-row sm:items-end"><div><p className="font-mono text-xs font-medium tracking-[0.1em] text-primary">NORMALIZED MATRIX</p><h2 id="comparison-matrix-heading" className="mt-2 font-sans text-2xl font-semibold text-on-surface">Technology comparison</h2></div><p className="font-mono text-[0.6875rem] text-on-surface-variant">Current, stale, and unknown observations are shown explicitly.</p></div><div className="hidden overflow-x-auto rounded-xl border border-outline-variant/30 lg:block"><table className="w-full min-w-[56rem] text-left"><caption className="sr-only">Normalized technology category comparison</caption><thead className="bg-surface-container-low"><tr><th className="w-48 px-5 py-3 font-mono text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-on-surface-variant">Category</th>{comparison.domains.map((domain) => <th key={domain} className="px-5 py-3 font-mono text-[0.6875rem] font-medium uppercase tracking-[0.08em] text-on-surface-variant"><DomainProfileLink domain={domain} /></th>)}</tr></thead><tbody className="divide-y divide-outline-variant/30">{categories.map((category) => <tr key={category.slug} className="align-top"><th scope="row" className="bg-surface-container-low/40 px-5 py-5 font-sans text-base font-semibold text-on-surface">{formatCategory(category.slug)}</th>{comparison.domains.map((domain) => <td key={domain} className="px-5 py-5"><MatrixCell cells={category.cellsByDomain.get(domain) ?? []} occurrence={technologyOccurrences(category, comparison.domains)} domainCount={comparison.domains.length} /></td>)}</tr>)}</tbody></table></div><div className="space-y-4 lg:hidden">{categories.map((category) => <CategoryCards key={category.slug} category={category} domains={comparison.domains} />)}</div></section>;
}

function CategoryCards({ category, domains }: { category: ComparisonCategory; domains: string[] }) {
  const occurrence = technologyOccurrences(category, domains);
  return <Card className="p-5"><h3 className="font-sans text-lg font-semibold text-on-surface">{formatCategory(category.slug)}</h3><div className="mt-4 divide-y divide-outline-variant/30">{domains.map((domain) => <div key={domain} className="py-4 first:pt-0 last:pb-0"><DomainProfileLink domain={domain} /><div className="mt-3"><MatrixCell cells={category.cellsByDomain.get(domain) ?? []} occurrence={occurrence} domainCount={domains.length} /></div></div>)}</div></Card>;
}

function MatrixCell({ cells, occurrence, domainCount }: { cells: PublicComparisonCell[]; occurrence: Map<string, number>; domainCount: number }) {
  const technologies = cells.filter((cell): cell is PublicComparisonCell & { technology_slug: string } => typeof cell.technology_slug === "string");
  if (technologies.length === 0) return <ObservationStatus state="unknown" />;
  return <div className="space-y-3">{technologies.map((cell) => <div key={`${cell.technology_slug}-${cell.last_observed_at ?? "unknown"}`}><div className="flex flex-wrap items-center gap-2"><Badge tone={cell.state === "stale" ? "warning" : "discovery"}>{cell.technology_slug}</Badge><ScopeBadge occurrences={occurrence.get(cell.technology_slug) ?? 0} domainCount={domainCount} /></div><div className="mt-2 flex flex-wrap gap-x-3 gap-y-1"><ObservationStatus state={cell.state} /><span className="font-mono text-[0.6875rem] text-on-surface-variant">Observed {formatTimestamp(cell.last_observed_at)}</span></div></div>)}</div>;
}

function ScopeBadge({ occurrences, domainCount }: { occurrences: number; domainCount: number }) {
  if (occurrences === domainCount) return <Badge tone="success">Common</Badge>;
  if (occurrences === 1) return <Badge tone="neutral">Unique</Badge>;
  return <Badge tone="neutral">Shared</Badge>;
}

function ObservationStatus({ state }: { state: string }) {
  const label = state === "stale" ? "Stale" : state === "current" ? "Current" : "Unknown";
  return <span className="font-mono text-[0.6875rem] text-on-surface-variant">{label}</span>;
}

function DomainProfileLink({ domain }: { domain: string }) {
  return <Link to="/domains/$domain" params={{ domain }} className="inline-flex items-center gap-1 font-mono text-xs font-medium text-primary hover:text-primary-fixed focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">{domain}<ArrowSquareOut size={14} aria-hidden="true" /></Link>;
}

function ComparisonStatePanel({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <Card className="flex min-h-56 flex-col items-start justify-center gap-4" role="status"><div><p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">DOMAIN COMPARISON</p><h2 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h2><p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p></div>{action}</Card>;
}

function groupComparisonCategories(comparison: PublicComparisonResponse): ComparisonCategory[] {
  const categories = new Map<string, ComparisonCategory>();
  for (const cell of comparison.cells) {
    const category = categories.get(cell.category_slug) ?? { slug: cell.category_slug, cellsByDomain: new Map<string, PublicComparisonCell[]>() };
    const cells = category.cellsByDomain.get(cell.canonical_domain) ?? [];
    cells.push(cell);
    category.cellsByDomain.set(cell.canonical_domain, cells);
    categories.set(cell.category_slug, category);
  }
  return [...categories.values()].sort((left, right) => left.slug.localeCompare(right.slug));
}

function technologyOccurrences(category: ComparisonCategory, domains: string[]): Map<string, number> {
  const occurrence = new Map<string, number>();
  for (const domain of domains) {
    const slugs = new Set((category.cellsByDomain.get(domain) ?? []).flatMap((cell) => cell.technology_slug ? [cell.technology_slug] : []));
    for (const slug of slugs) occurrence.set(slug, (occurrence.get(slug) ?? 0) + 1);
  }
  return occurrence;
}

function formatCategory(slug: string): string {
  return slug.split("-").map((part) => `${part.slice(0, 1).toUpperCase()}${part.slice(1)}`).join(" ");
}

function formatTimestamp(value: string | null | undefined): string {
  if (!value) return "Unavailable";
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.valueOf()) ? "Unavailable" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(timestamp);
}
