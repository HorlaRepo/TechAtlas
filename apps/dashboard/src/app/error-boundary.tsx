import { Button, Card } from "@techatlas/ui";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { AppShell } from "./app-shell";

type ErrorBoundaryProps = { children: ReactNode };
type ErrorBoundaryState = { error: Error | null };

export class DashboardErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState { return { error }; }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    // Errors are intentionally kept local: telemetry is provided by the API and no error payload is exposed to users.
    void _error;
    void _info;
  }

  render() {
    if (!this.state.error) return this.props.children;
    return <AppShell><Card className="flex min-h-72 flex-col items-start justify-center"><p className="font-mono text-xs text-primary">APPLICATION ERROR</p><h1 className="mt-2 font-sans text-2xl font-semibold">This screen could not be displayed</h1><p className="mt-3 max-w-xl text-sm text-on-surface-variant">Your navigation and the rest of the dashboard remain available. Try loading this screen again.</p><Button className="mt-5" onClick={() => this.setState({ error: null })}>Try again</Button></Card></AppShell>;
  }
}

export function RouteErrorFallback() {
  return <AppShell><Card className="flex min-h-72 flex-col items-start justify-center"><p className="font-mono text-xs text-primary">ROUTE ERROR</p><h1 className="mt-2 font-sans text-2xl font-semibold">This page is unavailable</h1><p className="mt-3 max-w-xl text-sm text-on-surface-variant">The application shell is still available. Return to Operations Center or reload the page.</p><Button className="mt-5" onClick={() => window.location.assign("/admin/overview")}>Return to overview</Button></Card></AppShell>;
}
