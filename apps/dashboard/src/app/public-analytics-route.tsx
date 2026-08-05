import { useNavigate, useSearch } from "@tanstack/react-router";
import { AnalyticsPage } from "@/features/analytics/analytics-page";
import { PublicLayout } from "./public-layout";
export function PublicAnalyticsRoute() { const search = useSearch({ from: "/analytics" }); const navigate = useNavigate({ from: "/analytics" }); return <PublicLayout><AnalyticsPage search={search} onSearchChange={(next) => void navigate({search:next})}/></PublicLayout>; }
