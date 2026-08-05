import { ChartLine, Cpu, House, MagnifyingGlass, Stack } from "@phosphor-icons/react";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";

export function PublicLayout({ children }: { children: ReactNode }) {
  return (
    <div className="min-h-screen bg-background text-on-surface">
      <header className="border-b border-outline-variant/30 bg-surface">
        <div className="mx-auto flex min-h-16 max-w-7xl flex-wrap items-center justify-between gap-4 px-4 py-3 sm:px-6 lg:px-8">
          <Link to="/" className="flex items-center gap-2 text-primary focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">
            <span className="flex size-8 items-center justify-center rounded-md bg-primary text-on-primary" aria-hidden="true">
              <Cpu size={18} weight="bold" />
            </span>
            <span className="font-sans text-lg font-semibold tracking-tight">TechAtlas</span>
          </Link>
          <nav className="flex flex-wrap items-center gap-x-4 gap-y-2 font-mono text-xs" aria-label="Public navigation">
            <Link to="/" activeProps={{ "aria-current": "page" }} className="inline-flex items-center gap-2 text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">
              <House size={15} aria-hidden="true" />
              Home
            </Link>
            <Link to="/domains" activeProps={{ "aria-current": "page" }} className="inline-flex items-center gap-2 text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">
              <MagnifyingGlass size={15} aria-hidden="true" />
              Domains
            </Link>
            <Link to="/technologies" activeProps={{ "aria-current": "page" }} className="inline-flex items-center gap-2 text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">
              <Stack size={15} aria-hidden="true" />
              Technologies
            </Link>
            <Link to="/compare" activeProps={{ "aria-current": "page" }} className="text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">
              Compare
            </Link>
            <Link to="/analytics" activeProps={{ "aria-current": "page" }} className="inline-flex items-center gap-2 text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary"><ChartLine size={15} aria-hidden="true" />Analytics</Link>
            <Link to="/about" activeProps={{ "aria-current": "page" }} className="text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">About</Link>
            <Link to="/admin/overview" className="text-on-surface-variant hover:text-on-surface focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-primary">
              Operations
            </Link>
          </nav>
        </div>
      </header>
      <main className="mx-auto w-full max-w-7xl px-4 py-8 sm:px-6 lg:px-8 lg:py-10">{children}</main>
    </div>
  );
}
