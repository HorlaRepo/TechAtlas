import { AppShell } from "@/app/app-shell";
import { type ReactNode } from "react";
import { AdminRouteGuard } from "./admin-route-guard";

export function AdminRouteLayout({ children }: { children: ReactNode }) {
  return <AdminRouteGuard><AppShell>{children}</AppShell></AdminRouteGuard>;
}
