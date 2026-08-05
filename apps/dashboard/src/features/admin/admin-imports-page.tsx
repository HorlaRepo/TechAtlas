import { importCsv } from "@techatlas/api-client";
import { Button, Card, Input } from "@techatlas/ui";
import { useMutation } from "@tanstack/react-query";
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
  const fileInputRef = useRef<HTMLInputElement>(null);

  const mutation = useMutation({
    mutationFn: async () => {
      const response = await importCsv({ body: { source_name: source, csv } });
      if (response.error || !response.data) throw Error("import failed");
      return response.data;
    },
    onSuccess: () => setConfirm(false),
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
    </>
  );
}
