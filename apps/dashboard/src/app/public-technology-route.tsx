import { useParams } from "@tanstack/react-router";
import { TechnologyProfilePage } from "@/features/technology-profile/technology-profile-page";
import { PublicLayout } from "./public-layout";

export function PublicTechnologyRoute() {
  const { technology } = useParams({ from: "/technologies/$technology" });
  return <PublicLayout><TechnologyProfilePage technology={technology} /></PublicLayout>;
}
