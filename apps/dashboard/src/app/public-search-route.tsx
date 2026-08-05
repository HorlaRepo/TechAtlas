import { useNavigate, useSearch } from "@tanstack/react-router";
import { DomainSearchPage } from "@/features/domain-search/domain-search-page";
import { PublicLayout } from "./public-layout";

export function PublicSearchRoute() {
  const search = useSearch({ from: "/search" });
  const navigate = useNavigate({ from: "/search" });

  return (
    <PublicLayout>
      <DomainSearchPage search={search} onSearchChange={(nextSearch) => void navigate({ search: nextSearch })} />
    </PublicLayout>
  );
}
