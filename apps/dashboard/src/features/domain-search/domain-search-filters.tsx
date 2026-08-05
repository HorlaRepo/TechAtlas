import { CaretDown, Funnel, X } from "@phosphor-icons/react";
import type { PublicDomainSearchPage } from "@techatlas/api-client";
import { Button } from "@techatlas/ui";
import { useRef, useState } from "react";
import type { DomainSearchState } from "./search-state";
import { validateDomainSearchState } from "./search-state";

type FacetKey = "technology" | "category" | "country";
type FacetCounts = PublicDomainSearchPage["facets"];

type DomainSearchFiltersProps = {
  search: DomainSearchState;
  facets: FacetCounts | undefined;
  onSearchChange: (search: DomainSearchState) => void;
};

type FacetMenuProps = {
  facet: FacetKey;
  label: string;
  counts: Record<string, number>;
  selected: string[];
  open: boolean;
  onClose: () => void;
  onToggleOpen: () => void;
  onToggleValue: (value: string) => void;
  triggerRef: (element: HTMLButtonElement | null) => void;
};

const recencyOptions = [
  { value: "", label: "Any time" },
  { value: "7", label: "Last 7 days" },
  { value: "30", label: "Last 30 days" },
  { value: "90", label: "Last 90 days" },
] as const;

const confidenceOptions = [
  { value: "", label: "Any confidence" },
  { value: "50", label: "50% or higher" },
  { value: "70", label: "70% or higher" },
  { value: "90", label: "90% or higher" },
] as const;

export function DomainSearchFilters({ search, facets, onSearchChange }: DomainSearchFiltersProps) {
  const [openFacet, setOpenFacet] = useState<FacetKey | undefined>();
  const triggers = useRef<Partial<Record<FacetKey, HTMLButtonElement | null>>>({});

  const update = (updates: Partial<DomainSearchState>) => {
    onSearchChange(validateDomainSearchState({ ...search, ...updates, offset: undefined }));
  };

  const toggleValue = (facet: FacetKey, value: string) => {
    const selected = search[facet] ?? [];
    update({ [facet]: selected.includes(value) ? selected.filter((item) => item !== value) : [...selected, value] });
  };

  const closeFacet = (facet: FacetKey) => {
    setOpenFacet(undefined);
    triggers.current[facet]?.focus();
  };

  const removeValue = (facet: FacetKey, value: string) => {
    update({ [facet]: (search[facet] ?? []).filter((item) => item !== value) });
  };

  const clearFilters = () => {
    onSearchChange(validateDomainSearchState({ q: search.q, sort: search.sort, limit: search.limit }));
  };

  const selectedCount = (search.technology?.length ?? 0) + (search.category?.length ?? 0) + (search.country?.length ?? 0);
  const hasFilters = selectedCount > 0 || search.crawled_since !== undefined || search.min_confidence !== undefined;

  return (
    <section className="border-t border-outline-variant/30 pt-5" aria-labelledby="search-filters-heading">
      <div className="flex flex-wrap items-center gap-3">
        <p id="search-filters-heading" className="flex items-center gap-2 font-mono text-xs font-medium tracking-[0.1em] text-on-surface-variant"><Funnel size={15} aria-hidden="true" />FILTERS</p>
        <FacetMenu facet="technology" label="Technology" counts={facets?.technology ?? {}} selected={search.technology ?? []} open={openFacet === "technology"} onClose={() => closeFacet("technology")} onToggleOpen={() => setOpenFacet(openFacet === "technology" ? undefined : "technology")} onToggleValue={(value) => toggleValue("technology", value)} triggerRef={(element) => { triggers.current.technology = element; }} />
        <FacetMenu facet="category" label="Category" counts={facets?.category ?? {}} selected={search.category ?? []} open={openFacet === "category"} onClose={() => closeFacet("category")} onToggleOpen={() => setOpenFacet(openFacet === "category" ? undefined : "category")} onToggleValue={(value) => toggleValue("category", value)} triggerRef={(element) => { triggers.current.category = element; }} />
        <FacetMenu facet="country" label="Country" counts={facets?.country ?? {}} selected={search.country ?? []} open={openFacet === "country"} onClose={() => closeFacet("country")} onToggleOpen={() => setOpenFacet(openFacet === "country" ? undefined : "country")} onToggleValue={(value) => toggleValue("country", value)} triggerRef={(element) => { triggers.current.country = element; }} />
        <CompactSelect label="Crawl recency" value={recencyValue(search.crawled_since)} options={recencyOptions} customLabel={search.crawled_since === undefined ? undefined : `Since ${formatDate(search.crawled_since)}`} onChange={(value) => update({ crawled_since: value ? Math.floor(Date.now() / 1000) - Number(value) * 86_400 : undefined })} />
        <CompactSelect label="Minimum confidence" value={confidenceValue(search.min_confidence)} options={confidenceOptions} customLabel={search.min_confidence === undefined ? undefined : `${search.min_confidence}% or higher`} onChange={(value) => update({ min_confidence: value ? Number(value) : undefined })} />
      </div>

      {hasFilters ? (
        <div className="mt-4 flex flex-wrap items-center gap-2" aria-label="Applied filters">
          <span className="font-mono text-[0.6875rem] text-on-surface-variant">APPLIED</span>
          {(search.technology ?? []).map((value) => <AppliedFilter key={`technology-${value}`} label={`Technology: ${value}`} onRemove={() => removeValue("technology", value)} />)}
          {(search.category ?? []).map((value) => <AppliedFilter key={`category-${value}`} label={`Category: ${value}`} onRemove={() => removeValue("category", value)} />)}
          {(search.country ?? []).map((value) => <AppliedFilter key={`country-${value}`} label={`Country: ${value}`} onRemove={() => removeValue("country", value)} />)}
          {search.crawled_since !== undefined ? <AppliedFilter label={`Crawled since ${formatDate(search.crawled_since)}`} onRemove={() => update({ crawled_since: undefined })} /> : null}
          {search.min_confidence !== undefined ? <AppliedFilter label={`Confidence ${search.min_confidence}%+`} onRemove={() => update({ min_confidence: undefined })} /> : null}
          <Button type="button" variant="ghost" size="compact" onClick={clearFilters}>Clear all filters</Button>
        </div>
      ) : null}
    </section>
  );
}

function FacetMenu({ facet, label, counts, selected, open, onClose, onToggleOpen, onToggleValue, triggerRef }: FacetMenuProps) {
  const options = facetOptions(counts, selected);
  const menuId = `search-${facet}-facet`;

  return (
    <div className="relative">
      <button ref={triggerRef} type="button" aria-expanded={open} aria-controls={menuId} className="inline-flex min-h-9 items-center gap-2 rounded-full border border-outline-variant/50 bg-surface-container px-3 font-mono text-xs text-on-surface transition-colors hover:bg-surface-container-high focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary" onClick={onToggleOpen}>
        {label}{selected.length > 0 ? <span className="text-primary">{selected.length}</span> : null}<CaretDown size={14} aria-hidden="true" />
      </button>
      {open ? (
        <div id={menuId} role="group" aria-label={`${label} filters`} className="absolute left-0 z-20 mt-2 max-h-72 w-72 overflow-y-auto rounded-xl border border-outline-variant/50 bg-surface-container p-2 shadow-2xl" onKeyDown={(event) => { if (event.key === "Escape") { event.preventDefault(); onClose(); } }}>
          {options.length > 0 ? options.map((option) => {
            const checked = selected.includes(option.value);
            return <label key={option.value} className="flex cursor-pointer items-center gap-3 rounded-md px-2 py-2 text-sm text-on-surface hover:bg-surface-container-high"><input type="checkbox" checked={checked} onChange={() => onToggleValue(option.value)} className="size-4 accent-primary" /><span className="min-w-0 flex-1 break-all">{option.value}</span><span className="font-mono text-xs text-on-surface-variant">{formatNumber(option.count)}</span></label>;
          }) : <p className="px-2 py-3 text-sm text-on-surface-variant">No values match the current search.</p>}
        </div>
      ) : null}
    </div>
  );
}

function CompactSelect({ label, value, options, customLabel, onChange }: { label: string; value: string; options: readonly { value: string; label: string }[]; customLabel: string | undefined; onChange: (value: string) => void }) {
  const hasCustomValue = value === "custom" && customLabel;
  return (
    <label className="relative">
      <span className="sr-only">{label}</span>
      <select aria-label={label} value={value} onChange={(event) => onChange(event.target.value)} className="min-h-9 appearance-none rounded-full border border-outline-variant/50 bg-surface-container py-1.5 pl-3 pr-9 font-mono text-xs text-on-surface outline-none hover:bg-surface-container-high focus-visible:ring-1 focus-visible:ring-primary">
        {hasCustomValue ? <option value="custom" disabled>{customLabel}</option> : null}
        {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
      </select>
      <CaretDown className="pointer-events-none absolute right-3 top-1/2 size-3.5 -translate-y-1/2 text-on-surface-variant" aria-hidden="true" />
    </label>
  );
}

function AppliedFilter({ label, onRemove }: { label: string; onRemove: () => void }) {
  return <button type="button" onClick={onRemove} aria-label={`Remove ${label} filter`} className="inline-flex min-h-8 items-center gap-1 rounded-full bg-primary-container/15 px-2.5 font-mono text-[0.6875rem] text-primary-fixed transition-colors hover:bg-primary-container/25 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary">{label}<X size={13} aria-hidden="true" /></button>;
}

function facetOptions(counts: Record<string, number>, selected: string[]) {
  const options = new Map(Object.entries(counts));
  for (const value of selected) {
    if (!options.has(value)) {
      options.set(value, 0);
    }
  }
  return [...options].map(([value, count]) => ({ value, count })).sort((left, right) => right.count - left.count || left.value.localeCompare(right.value));
}

function recencyValue(timestamp: number | undefined): string {
  if (timestamp === undefined) {
    return "";
  }
  const ageInDays = (Math.floor(Date.now() / 1000) - timestamp) / 86_400;
  return [7, 30, 90].some((days) => Math.abs(ageInDays - days) < 1 / 24) ? String(Math.round(ageInDays)) : "custom";
}

function confidenceValue(confidence: number | undefined): string {
  if (confidence === undefined) {
    return "";
  }
  return [50, 70, 90].includes(confidence) ? String(confidence) : "custom";
}

function formatDate(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(timestamp * 1000));
}

function formatNumber(value: number): string {
  return new Intl.NumberFormat().format(value);
}
