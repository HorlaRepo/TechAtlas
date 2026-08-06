import {
  CaretRight,
  ChartLine,
  CirclesFour,
  Cpu,
  Database,
  GearSix,
  GlobeHemisphereWest,
  HardHat,
  List,
  ListDashes,
  MagnifyingGlass,
  Pulse,
  RocketLaunch,
  Stack,
  X,
} from "@phosphor-icons/react";
import { useAuth0 } from "@auth0/auth0-react";
import { Link, useLocation, useNavigate } from "@tanstack/react-router";
import { Button, IconButton, Input } from "@techatlas/ui";
import { type ReactNode, useEffect, useId, useRef, useState } from "react";
import { adminBreadcrumbForPath } from "./admin-breadcrumbs";
import { QuickCrawlDialog } from "@/features/admin/quick-crawl-dialog";

type AppShellProps = {
  children: ReactNode;
};

type NavigationItem = {
  label: string;
  icon: ReactNode;
  to: "/admin/overview" | "/admin/analytics" | "/admin/monitoring" | "/admin/settings" | "/admin/domains" | "/admin/imports" | "/admin/audit" | "/admin/scheduler" | "/admin/queue" | "/admin/workers" | "/admin/rules" | "/search" | "/technologies";
};

const navigationItems: NavigationItem[] = [
  { label: "Overview", icon: <CirclesFour size={20} />, to: "/admin/overview" },
  { label: "Analytics", icon: <ChartLine size={20} />, to: "/admin/analytics" },
  { label: "Monitoring", icon: <Pulse size={20} />, to: "/admin/monitoring" },
  { label: "Search", icon: <MagnifyingGlass size={20} />, to: "/search" },
  { label: "Domains", icon: <GlobeHemisphereWest size={20} />, to: "/admin/domains" },
  { label: "Imports", icon: <Database size={20} />, to: "/admin/imports" },
  { label: "Audit", icon: <ListDashes size={20} />, to: "/admin/audit" },
  { label: "Technologies", icon: <Cpu size={20} />, to: "/technologies" },
  { label: "Scheduler", icon: <Stack size={20} />, to: "/admin/scheduler" },
  { label: "Workers", icon: <HardHat size={20} />, to: "/admin/workers" },
  { label: "Queue", icon: <ListDashes size={20} />, to: "/admin/queue" },
  { label: "Detection Rules", icon: <Database size={20} />, to: "/admin/rules" },
  { label: "Settings", icon: <GearSix size={20} />, to: "/admin/settings" },
];

export function AppShell({ children }: AppShellProps) {
  const [isMobileNavigationOpen, setIsMobileNavigationOpen] = useState(false);
  const [isQuickCrawlOpen, setIsQuickCrawlOpen] = useState(false);
  const [globalSearch, setGlobalSearch] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);
  const mobileNavigationTriggerRef = useRef<HTMLElement | null>(null);
  const quickCrawlTriggerRef = useRef<HTMLElement | null>(null);
  const navigate = useNavigate();
  const location = useLocation();
  const { logout, user } = useAuth0();

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchInputRef.current?.focus();
      }
    };

    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  const handleQuickCrawl = () => {
    quickCrawlTriggerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setIsQuickCrawlOpen(true);
    setIsMobileNavigationOpen(false);
  };
  const closeQuickCrawl = () => {
    setIsQuickCrawlOpen(false);
    requestAnimationFrame(() => quickCrawlTriggerRef.current?.focus());
  };
  const openMobileNavigation = () => {
    mobileNavigationTriggerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setIsMobileNavigationOpen(true);
  };
  const closeMobileNavigation = () => {
    setIsMobileNavigationOpen(false);
    requestAnimationFrame(() => mobileNavigationTriggerRef.current?.focus());
  };
  const breadcrumb = adminBreadcrumbForPath(location.pathname);

  return (
    <div className="min-h-screen bg-background text-on-surface">
      <aside className="fixed inset-y-0 left-0 z-40 hidden w-64 border-r border-outline-variant/30 bg-surface px-4 py-6 lg:flex lg:flex-col">
        <SidebarContent
          onQuickCrawl={handleQuickCrawl}
          userName={user?.name ?? user?.email ?? "Administrator"}
          userEmail={user?.email ?? "Administrator account"}
          onLogout={() => logout({ logoutParams: { returnTo: window.location.origin } })}
        />
      </aside>

      {isQuickCrawlOpen ? <QuickCrawlDialog onClose={closeQuickCrawl} /> : null}

      {isMobileNavigationOpen ? (
        <div className="fixed inset-0 z-50 lg:hidden" role="dialog" aria-modal="true" aria-label="Dashboard navigation">
          <button
            className="absolute inset-0 bg-background/80 backdrop-blur-sm"
            type="button"
            aria-label="Close navigation"
            onClick={closeMobileNavigation}
          />
          <aside className="relative flex h-full w-[18rem] max-w-[86vw] flex-col border-r border-outline-variant/30 bg-surface px-4 py-6 shadow-floating">
            <IconButton
              className="absolute right-4 top-4"
              icon={<X size={20} />}
              label="Close navigation"
              onClick={closeMobileNavigation}
            />
            <SidebarContent
              onQuickCrawl={handleQuickCrawl}
              userName={user?.name ?? user?.email ?? "Administrator"}
              userEmail={user?.email ?? "Administrator account"}
              onLogout={() => logout({ logoutParams: { returnTo: window.location.origin } })}
            />
          </aside>
        </div>
      ) : null}

      <div className="lg:pl-64">
        <header className="sticky top-0 z-30 border-b border-outline-variant/30 bg-surface/95 px-4 py-3 backdrop-blur lg:px-8">
          <div className="flex items-center justify-between gap-4">
            <div className="flex min-w-0 items-center gap-2 text-on-surface-variant">
              <IconButton
                className="lg:hidden"
                icon={<List size={22} />}
                label="Open navigation"
                onClick={openMobileNavigation}
              />
              <span className="hidden font-mono text-xs sm:inline">TechAtlas</span>
              <CaretRight className="hidden size-3 sm:block" aria-hidden="true" />
              <span className="font-mono text-xs text-on-surface">{breadcrumb}</span>
            </div>

            <div className="relative hidden md:block">
                <MagnifyingGlass
                  className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-on-surface-variant"
                  aria-hidden="true"
                />
                <Input
                  ref={searchInputRef}
                  className="w-72 pl-10 pr-12"
                  aria-label="Search TechAtlas"
                  placeholder="Search domains, tech, crawls..."
                  value={globalSearch}
                  onChange={(event) => setGlobalSearch(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void navigate({ to: "/search", search: globalSearch.trim() ? { q: globalSearch.trim() } : {} });
                    }
                  }}
                />
                <kbd className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 rounded-sm bg-surface-container-high px-1.5 py-0.5 font-mono text-[0.625rem] text-on-surface-variant">
                  ⌘ K
                </kbd>
            </div>
          </div>
        </header>

        <main className="w-full px-4 py-6 sm:px-6 lg:px-8 lg:py-8">{children}</main>
      </div>
    </div>
  );
}

function SidebarContent({
  onLogout,
  onQuickCrawl,
  userName,
  userEmail,
}: {
  onLogout: () => void;
  onQuickCrawl: () => void;
  userName: string;
  userEmail: string;
}) {
  return (
    <>
      <div className="mb-8 flex items-center gap-3 px-2">
        <div className="flex size-10 items-center justify-center rounded-md bg-primary text-on-primary">
          <Cpu size={22} weight="bold" aria-hidden="true" />
        </div>
        <div>
          <p className="font-sans text-lg font-semibold tracking-tight text-primary">TechAtlas</p>
          <p className="font-mono text-[0.625rem] font-medium tracking-[0.1em] text-on-surface-variant">
            ENTERPRISE INTELLIGENCE
          </p>
        </div>
      </div>

      <Button className="mb-8 w-full" onClick={onQuickCrawl}>
        <RocketLaunch size={16} weight="fill" aria-hidden="true" />
        Quick Crawl
      </Button>

      <nav className="flex flex-1 flex-col gap-1" aria-label="Operations navigation">
        {navigationItems.map((item) =>
            <Link
              key={item.label}
              to={item.to}
              activeProps={{ "aria-current": "page" }}
              className="flex min-h-10 items-center gap-3 rounded-md px-3 font-sans text-sm text-on-surface-variant transition-colors hover:bg-surface-container-high hover:text-on-surface [&[aria-current=page]]:bg-surface-container [&[aria-current=page]]:font-medium [&[aria-current=page]]:text-primary"
            >
              {item.icon}
              {item.label}
            </Link>
        )}
      </nav>

      <SidebarAccount userName={userName} userEmail={userEmail} onLogout={onLogout} />
    </>
  );
}

function SidebarAccount({
  onLogout,
  userEmail,
  userName,
}: {
  onLogout: () => void;
  userEmail: string;
  userName: string;
}) {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const popoverId = useId();
  const popoverRef = useRef<HTMLDivElement>(null);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!isSettingsOpen) {
      return undefined;
    }

    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setIsSettingsOpen(false);
        requestAnimationFrame(() => settingsButtonRef.current?.focus());
      }
    };
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (
        target instanceof Node
        && !popoverRef.current?.contains(target)
        && !settingsButtonRef.current?.contains(target)
      ) {
        setIsSettingsOpen(false);
      }
    };

    document.addEventListener("keydown", closeOnEscape);
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => {
      document.removeEventListener("keydown", closeOnEscape);
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
    };
  }, [isSettingsOpen]);

  return (
    <div className="mt-6 border-t border-outline-variant/30 pt-4">
      <div className="relative flex items-center gap-3 px-2">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-full bg-surface-container-high font-mono text-xs font-medium text-primary">
          {userName.slice(0, 2).toUpperCase()}
        </div>
        <div className="min-w-0 flex-1">
          <p className="truncate font-mono text-xs font-medium text-on-surface">{userName}</p>
          <p className="mt-0.5 truncate font-mono text-[0.625rem] text-on-surface-variant">{userEmail}</p>
        </div>
        <IconButton
          ref={settingsButtonRef}
          icon={<GearSix size={19} aria-hidden="true" />}
          label="Account settings"
          aria-controls={isSettingsOpen ? popoverId : undefined}
          aria-expanded={isSettingsOpen}
          aria-haspopup="dialog"
          onClick={() => setIsSettingsOpen((open) => !open)}
        />
        {isSettingsOpen ? (
          <div
            ref={popoverRef}
            id={popoverId}
            role="dialog"
            aria-label="Account settings"
            className="absolute left-full top-1/2 z-10 ml-2 w-48 -translate-y-1/2 rounded-xl border border-outline-variant/40 bg-surface-container p-1 shadow-floating"
          >
            <Button
              className="w-full justify-start text-error hover:bg-error/15 hover:text-error"
              variant="ghost"
              onClick={() => {
                setIsSettingsOpen(false);
                onLogout();
              }}
            >
              Sign out
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
