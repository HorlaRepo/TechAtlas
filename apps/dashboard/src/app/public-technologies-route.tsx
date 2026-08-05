import { useNavigate, useSearch } from "@tanstack/react-router";
import { TechnologyLibraryPage } from "@/features/technology-library/technology-library-page";
import { PublicLayout } from "./public-layout";

export function PublicTechnologiesRoute() {
  const search = useSearch({ from: "/technologies" });
  const navigate = useNavigate({ from: "/technologies" });

  return (
    <PublicLayout>
      <TechnologyLibraryPage search={search} onSearchChange={(nextSearch) => void navigate({ search: nextSearch })} />
    </PublicLayout>
  );
}
