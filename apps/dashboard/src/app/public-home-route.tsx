import { LandingPage } from "@/features/landing/landing-page";
import { PublicLayout } from "./public-layout";

export function PublicHomeRoute() {
  return <PublicLayout><LandingPage /></PublicLayout>;
}
