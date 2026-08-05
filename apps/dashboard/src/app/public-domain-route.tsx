import { useParams } from "@tanstack/react-router";
import { DomainProfilePage } from "@/features/domain-profile/domain-profile-page";
import { PublicLayout } from "./public-layout";

export function PublicDomainRoute() {
  const { domain } = useParams({ from: "/domains/$domain" });

  return <PublicLayout><DomainProfilePage domain={domain} /></PublicLayout>;
}
