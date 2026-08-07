import { completedImportBatches, importCsv, scheduleImportBatchRecrawl, type ImportBatchResponse } from "@techatlas/api-client";
import { Button, Card, Input } from "@techatlas/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ChangeEvent, type DragEvent, type FormEvent, useRef, useState } from "react";
import { AdminConfirmDialog } from "./admin-confirm-dialog";

const MAX_CSV_BYTES = 1_048_576;

export function AdminImportsPage() {
  const [source, setSource] = useState("");
  const [csv, setCsv] = useState("");
  const [confirm, setConfirm] = useState(false);
  const [isDragActive, setIsDragActive] = useState(false);
  const [isReadingFile, setIsReadingFile] = useState(false);
  const [fileError, setFileError] = useState<string>();
  const [selectedFileName, setSelectedFileName] = useState<string>();
  const [recrawlBatch, setRecrawlBatch] = useState<ImportBatchResponse>();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const client = useQueryClient();
  const batches = useQuery({
    queryKey: ["admin", "completed-import-batches"],
    queryFn: async () => {
      const response = await completedImportBatches({ query: { limit: 50 } });
      if (response.error || !response.data) throw Error("completed import batches unavailable");
      return response.data.imports;
    },
  });

  const mutation = useMutation({
    mutationFn: async () => {
      const response = await importCsv({ body: { source_name: source, csv } });
      if (response.error || !response.data) throw Error("import failed");
      return response.data;
    },
    onSuccess: () => {
      setConfirm(false);
      void client.invalidateQueries({ queryKey: ["admin", "completed-import-batches"] });
    },
  });
  const recrawl = useMutation({
    mutationFn: async (import_id: string) => {
      const response = await scheduleImportBatchRecrawl({ path: { import_id } });
      if (response.error || !response.data) throw Error("batch recrawl could not be scheduled");
      return response.data;
    },
    onSuccess: () => {
      setRecrawlBatch(undefined);
      void client.invalidateQueries({ queryKey: ["admin", "crawl-attempts"] });
      void client.invalidateQueries({ queryKey: ["admin", "operations"] });
    },
  });

  const loadCsvFile = async (file: File | undefined) => {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".csv") && file.type !== "text/csv") {
      setFileError("Choose a CSV file.");
      return;
    }
    if (file.size > MAX_CSV_BYTES) {
      setFileError("CSV files must be 1 MB or smaller.");
      return;
    }

    setFileError(undefined);
    setIsReadingFile(true);
    try {
      setCsv(await file.text());
      setSelectedFileName(file.name);
    } catch {
      setFileError("The CSV file could not be read. Try another file.");
    } finally {
      setIsReadingFile(false);
    }
  };

  const handleFileSelection = (event: ChangeEvent<HTMLInputElement>) => {
    void loadCsvFile(event.target.files?.[0]);
    event.target.value = "";
  };

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    setIsDragActive(false);
    void loadCsvFile(event.dataTransfer.files[0]);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    setConfirm(true);
  };

  return (
    <>
      <div className="space-y-6">
        <header>
          <p className="font-mono text-xs text-primary">ADMIN IMPORTS</p>
          <h1 className="mt-2 font-sans text-3xl font-semibold">Import a domain list</h1>
        </header>
        <Card>
          <form className="space-y-4" onSubmit={submit}>
            <Input aria-label="Source name" value={source} onChange={(event) => setSource(event.target.value)} placeholder="Partner list" required />

            <div
              className={`rounded-xl border border-dashed p-5 text-center transition-colors ${isDragActive ? "border-primary bg-primary/10" : "border-outline-variant/50 bg-surface-container-low"}`}
              onDragEnter={(event) => {
                event.preventDefault();
                setIsDragActive(true);
              }}
              onDragOver={(event) => event.preventDefault()}
              onDragLeave={(event) => {
                if (event.currentTarget === event.target) setIsDragActive(false);
              }}
              onDrop={handleDrop}
            >
              <input ref={fileInputRef} className="sr-only" type="file" accept=".csv,text/csv" aria-label="Choose CSV file" onChange={handleFileSelection} />
              <p className="text-sm text-on-surface">Drop a CSV file here, or choose one from your device.</p>
              <Button className="mt-3" type="button" variant="secondary" disabled={isReadingFile || mutation.isPending} onClick={() => fileInputRef.current?.click()}>
                {isReadingFile ? "Reading CSV…" : "Choose CSV file"}
              </Button>
              {selectedFileName ? <p className="mt-3 font-mono text-xs text-primary" role="status">Loaded {selectedFileName}</p> : null}
              {fileError ? <p className="mt-3 text-sm text-error" role="alert">{fileError}</p> : null}
            </div>

            <label className="block">
              <span className="font-mono text-xs text-on-surface-variant">CSV content</span>
              <textarea
                aria-label="CSV content"
                className="mt-2 min-h-56 w-full rounded-md border border-outline-variant/30 bg-surface-container-low p-3 font-mono text-sm focus-visible:outline-2 focus-visible:outline-primary"
                value={csv}
                onChange={(event) => {
                  setCsv(event.target.value);
                  setSelectedFileName(undefined);
                }}
                placeholder={"domain\nexample.com"}
                required
              />
            </label>
            <Button type="submit" disabled={isReadingFile || mutation.isPending}>Import CSV</Button>
          </form>
          {mutation.data ? (
            <dl className="mt-5 grid grid-cols-3 gap-3 text-sm">
              <div><dt>Accepted</dt><dd>{mutation.data.accepted_row_count}</dd></div>
              <div><dt>Duplicates</dt><dd>{mutation.data.duplicate_row_count}</dd></div>
              <div><dt>Rejected</dt><dd>{mutation.data.rejected_row_count}</dd></div>
            </dl>
          ) : null}
          {mutation.isError ? <p className="mt-4 text-error" role="alert">{String(mutation.error.message)}</p> : null}
        </Card>
        <Card>
          <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
            <div>
              <h2 className="font-sans text-xl font-semibold">Recrawl an imported batch</h2>
              <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">Schedule an existing completed import through the normal crawl queue. Domains with disabled policies or active work are skipped.</p>
            </div>
          </div>
          {recrawl.data ? <p className="mt-4 text-sm text-primary" role="status">Scheduled {recrawl.data.scheduled_domain_count.toLocaleString()} of {recrawl.data.requested_domain_count.toLocaleString()} domains from {recrawl.data.source_name}; {recrawl.data.skipped_domain_count.toLocaleString()} skipped.</p> : null}
          {recrawl.isError ? <p className="mt-4 text-sm text-error" role="alert">{String(recrawl.error.message)}</p> : null}
          {batches.isLoading ? <p className="mt-5 text-sm text-on-surface-variant" role="status">Loading completed imports…</p> : null}
          {batches.isError ? <p className="mt-5 text-sm text-error" role="alert">Could not load completed imports.</p> : null}
          {batches.data?.length === 0 ? <p className="mt-5 text-sm text-on-surface-variant">No completed import batches are available.</p> : null}
          {batches.data?.length ? <ul className="mt-5 divide-y divide-outline-variant/30">{batches.data.map((batch) => <li key={batch.import_id} className="flex flex-wrap items-center justify-between gap-4 py-4"><div><p className="font-mono text-sm text-on-surface">{batch.source_name}</p><p className="mt-1 text-xs text-on-surface-variant">{batch.domain_count.toLocaleString()} domain{batch.domain_count === 1 ? "" : "s"} · imported {formatTimestamp(batch.completed_at)}</p></div><Button size="compact" variant="secondary" disabled={recrawl.isPending} onClick={() => setRecrawlBatch(batch)}>Recrawl batch</Button></li>)}</ul> : null}
        </Card>
      </div>
      {confirm ? (
        <AdminConfirmDialog
          title="Import domain list"
          description="Valid rows will be added and duplicate or invalid rows will be recorded for auditability."
          confirmLabel="Import domains"
          isPending={mutation.isPending}
          onCancel={() => setConfirm(false)}
          onConfirm={() => mutation.mutate()}
        />
      ) : null}
      {recrawlBatch ? (
        <AdminConfirmDialog
          title="Recrawl imported batch"
          description={`Up to ${recrawlBatch.domain_count.toLocaleString()} domains from ${recrawlBatch.source_name} will become eligible for the scheduler. This does not bypass robots, SSRF, crawl policy, or active-work safeguards.`}
          confirmLabel="Schedule batch recrawl"
          isPending={recrawl.isPending}
          onCancel={() => setRecrawlBatch(undefined)}
          onConfirm={() => recrawl.mutate(recrawlBatch.import_id)}
        />
      ) : null}
    </>
  );
}

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.valueOf()) ? "Time unavailable" : new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(timestamp);
}
