import { AboutPage } from "@/features/about/about-page";
import { PublicLayout } from "./public-layout";

export function PublicAboutRoute() {
  return <PublicLayout><AboutPage /></PublicLayout>;
}
