import {
  detectionRules,
  publishRuleDraft,
  reprocessingRuns,
  saveRuleDraft,
  setActiveRuleVersion,
  startReprocessing,
  testRuleDraft,
} from "@techatlas/api-client";
import { Button, Card } from "@techatlas/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { AdminConfirmDialog } from "./admin-confirm-dialog";

const fixture = JSON.stringify([{ source: "header", key: "x-powered-by", value: "Next.js" }], null, 2);

export function AdminRulesPage() {
  const client = useQueryClient();
  const rules = useQuery({
    queryKey: ["admin", "detection-rules"],
    queryFn: async () => {
      const result = await detectionRules();
      if (result.error || !result.data) throw Error("rules unavailable");
      return result.data.rules;
    },
  });
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  const [editingSlug, setEditingSlug] = useState<string | null>(null);
  const [definition, setDefinition] = useState("");
  const [fixtureSignals, setFixtureSignals] = useState(fixture);
  const [publishSlug, setPublishSlug] = useState<string | null>(null);
  const selected = rules.data?.find((rule) => rule.rule_slug === selectedSlug) ?? rules.data?.[0];
  const runs = useQuery({
    queryKey: ["admin", "detection-reprocessing-runs", selected?.rule_slug],
    queryFn: async () => {
      const result = await reprocessingRuns({ query: { rule_slug: selected?.rule_slug, limit: 20 } });
      if (result.error || !result.data) throw Error("reprocessing runs unavailable");
      return result.data.runs;
    },
  });
  const selectedDefinition = selected
    ? editingSlug === selected.rule_slug
      ? definition
      : selected.draft_definition ?? selected.versions.find((version) => version.is_active)?.definition ?? ""
    : "";
  const refresh = () => {
    void client.invalidateQueries({ queryKey: ["admin", "detection-rules"] });
    void client.invalidateQueries({ queryKey: ["admin", "detection-reprocessing-runs"] });
  };
  const save = useMutation({
    mutationFn: async () => {
      if (!selected) throw Error("Select a rule first");
      const result = await saveRuleDraft({ path: { rule_slug: selected.rule_slug }, body: { definition: selectedDefinition } });
      if (result.error || !result.data) throw Error("draft could not be saved");
      return result.data;
    },
    onSuccess: refresh,
  });
  const test = useMutation({
    mutationFn: async () => {
      if (!selected) throw Error("Select a rule first");
      const signals: unknown = JSON.parse(fixtureSignals);
      if (!Array.isArray(signals)) throw Error("Fixture signals must be a JSON array");
      const result = await testRuleDraft({ path: { rule_slug: selected.rule_slug }, body: { signals } });
      if (result.error || !result.data) throw Error("draft test could not run");
      return result.data;
    },
    onSuccess: refresh,
  });
  const publish = useMutation({
    mutationFn: async (rule_slug: string) => {
      const result = await publishRuleDraft({ path: { rule_slug } });
      if (result.error || !result.data) throw Error("publish requires a passing draft test");
      return result.data;
    },
    onSuccess: () => {
      setPublishSlug(null);
      refresh();
    },
  });
  const activate = useMutation({
    mutationFn: async ({ rule_slug, version }: { rule_slug: string; version: number | null }) => {
      const result = await setActiveRuleVersion({ path: { rule_slug }, body: { version } });
      if (result.error) throw Error("activation could not be updated");
    },
    onSuccess: refresh,
  });
  const reprocess = useMutation({
    mutationFn: async ({ rule_slug, rule_version }: { rule_slug: string; rule_version: number }) => {
      const result = await startReprocessing({
        path: { rule_slug, rule_version },
        body: { idempotency_key: crypto.randomUUID() },
      });
      if (result.error || !result.data) throw Error("reprocessing could not be started");
      return result.data;
    },
    onSuccess: refresh,
  });

  if (rules.isLoading) return <Card role="status">Loading detection rules…</Card>;
  if (rules.isError || !rules.data) return <Card role="alert">Could not load detection rules.</Card>;

  return <>
    <div className="grid gap-6 xl:grid-cols-[18rem_minmax(0,1fr)]">
      <aside>
        <p className="font-mono text-xs text-primary">ADMIN RULES</p>
        <h1 className="mt-2 font-sans text-3xl font-semibold">Detection rules</h1>
        <nav className="mt-5 space-y-1" aria-label="Detection rules">
          {rules.data.map((rule) => <button key={rule.rule_slug} className={`w-full rounded-md px-3 py-3 text-left ${selected?.rule_slug === rule.rule_slug ? "bg-surface-container text-primary" : "text-on-surface-variant hover:bg-surface-container"}`} type="button" onClick={() => { setSelectedSlug(rule.rule_slug); setEditingSlug(rule.rule_slug); setDefinition(rule.draft_definition ?? rule.versions.find((version) => version.is_active)?.definition ?? ""); }}>
            <span className="block font-mono text-sm">{rule.technology_name}</span>
            <span className="mt-1 block font-mono text-[0.625rem]">{rule.active_version ? `Active v${rule.active_version}` : "Disabled"}</span>
          </button>)}
        </nav>
      </aside>
      {selected ? <section className="space-y-5">
        <Card>
          <div className="flex flex-wrap items-start justify-between gap-3"><div><p className="font-mono text-xs text-primary">{selected.rule_slug}</p><h2 className="mt-2 font-sans text-2xl font-semibold">{selected.technology_name}</h2></div><Button size="compact" variant="secondary" disabled={activate.isPending} onClick={() => activate.mutate({ rule_slug: selected.rule_slug, version: null })}>Disable</Button></div>
          <label className="mt-5 block"><span className="font-mono text-xs text-on-surface-variant">Draft definition (JSON)</span><textarea className="mt-2 min-h-72 w-full rounded-md border border-outline-variant/30 bg-surface-container-low p-3 font-mono text-xs focus-visible:outline-2 focus-visible:outline-primary" value={selectedDefinition} onChange={(event) => { setEditingSlug(selected.rule_slug); setDefinition(event.target.value); }} /></label>
          <div className="mt-4 flex flex-wrap gap-3"><Button variant="secondary" disabled={save.isPending} onClick={() => save.mutate()}>Save draft</Button><Button variant="secondary" disabled={test.isPending || !selected.draft_definition} onClick={() => test.mutate()}>Test saved draft</Button><Button disabled={publish.isPending || !selected.draft_definition} onClick={() => setPublishSlug(selected.rule_slug)}>Publish tested draft</Button></div>
          {save.isError || test.isError || publish.isError || activate.isError ? <p role="alert" className="mt-3 text-sm text-error">{String(save.error?.message ?? test.error?.message ?? publish.error?.message ?? activate.error?.message)}</p> : null}
          {test.data ? <p className={`mt-3 text-sm ${test.data.passed ? "text-primary" : "text-error"}`} role="status">Fixture test {test.data.passed ? "passed" : "did not pass"}: {test.data.confidence}/{test.data.threshold} confidence.</p> : null}
        </Card>
        <Card><h3 className="font-sans text-lg font-semibold">Fixture signals</h3><p className="mt-2 text-sm text-on-surface-variant">Supply a deterministic JSON array of source, key, and value signals before publishing.</p><textarea className="mt-3 min-h-32 w-full rounded-md border border-outline-variant/30 bg-surface-container-low p-3 font-mono text-xs focus-visible:outline-2 focus-visible:outline-primary" value={fixtureSignals} onChange={(event) => setFixtureSignals(event.target.value)} /></Card>
        <Card><h3 className="font-sans text-lg font-semibold">Published versions</h3><p className="mt-2 text-sm text-on-surface-variant">Historical replay creates separate immutable results and never changes the current stack.</p><ul className="mt-4 divide-y divide-outline-variant/30">{selected.versions.map((version) => <li key={version.version} className="flex flex-wrap items-center justify-between gap-3 py-3"><span className="font-mono text-sm">v{version.version}{version.is_active ? " · active" : ""}</span><span className="flex flex-wrap gap-2"><Button size="compact" variant="secondary" disabled={version.is_active || activate.isPending} onClick={() => activate.mutate({ rule_slug: selected.rule_slug, version: version.version })}>{selected.active_version ? "Activate / rollback" : "Activate"}</Button><Button size="compact" variant="secondary" disabled={reprocess.isPending} onClick={() => reprocess.mutate({ rule_slug: selected.rule_slug, rule_version: version.version })}>Reprocess history</Button></span></li>)}</ul>{reprocess.isError ? <p role="alert" className="mt-3 text-sm text-error">{String(reprocess.error.message)}</p> : null}</Card>
        <Card><h3 className="font-sans text-lg font-semibold">Reprocessing runs</h3>{runs.isLoading ? <p role="status" className="mt-3 text-sm text-on-surface-variant">Loading runs…</p> : null}{runs.isError ? <p role="alert" className="mt-3 text-sm text-error">Could not load replay status.</p> : null}{runs.data?.length === 0 ? <p className="mt-3 text-sm text-on-surface-variant">No historical reprocessing runs for this rule.</p> : null}<ul className="mt-3 divide-y divide-outline-variant/30">{runs.data?.map((run) => <li key={run.id} className="py-3"><p className="font-mono text-sm">v{run.rule_version} · {run.status}</p><p className="mt-1 text-sm text-on-surface-variant">{run.succeeded_snapshot_count}/{run.total_snapshot_count} succeeded{run.failed_snapshot_count ? ` · ${run.failed_snapshot_count} dead-lettered` : ""}</p></li>)}</ul></Card>
      </section> : null}
    </div>
    {publishSlug ? <AdminConfirmDialog title="Publish tested rule draft" description="Publishing creates an immutable version. Activate it afterwards when you are ready for it to apply to new crawls." confirmLabel="Publish version" isPending={publish.isPending} onCancel={() => setPublishSlug(null)} onConfirm={() => publish.mutate(publishSlug)} /> : null}
  </>;
}
