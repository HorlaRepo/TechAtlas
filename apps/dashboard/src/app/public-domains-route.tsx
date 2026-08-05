import { useNavigate, useSearch } from "@tanstack/react-router";
import { DomainSearchPage } from "@/features/domain-search/domain-search-page";
import { PublicLayout } from "./public-layout";

export function PublicDomainsRoute() {
  const search = useSearch({ from: "/domains" });
  const navigate = useNavigate({ from: "/domains" });

  return (
    <PublicLayout>
      <DomainSearchPage
        search={search}
        onSearchChange={(nextSearch) => void navigate({ search: nextSearch })}
        presentation={{
          eyebrow: "DOMAIN CATALOGUE",
          title: "Browse observed domains",
          description: "Filter public domain observations by technology, category, country, confidence, and recent crawl activity. Every result view has a shareable URL.",
        }}
      />
    </PublicLayout>
  );
}
