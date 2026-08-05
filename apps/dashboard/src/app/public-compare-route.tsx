import { useNavigate, useSearch } from "@tanstack/react-router";
import { DomainComparisonPage } from "@/features/domain-comparison/domain-comparison-page";
import { PublicLayout } from "./public-layout";

export function PublicCompareRoute() {
  const search = useSearch({ from: "/compare" });
  const navigate = useNavigate({ from: "/compare" });
  return <PublicLayout><DomainComparisonPage search={search} onSearchChange={(nextSearch) => void navigate({ search: nextSearch })} /></PublicLayout>;
}
