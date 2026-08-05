import { requestDomainCrawl } from "@techatlas/api-client";
import { Button, Input } from "@techatlas/ui";
import { useMutation } from "@tanstack/react-query";
import { type FormEvent, useEffect, useRef, useState } from "react";

class QuickCrawlError extends Error {
  constructor(readonly status: number | undefined) {
    super("Quick Crawl could not be requested.");
  }
}

export function QuickCrawlDialog({ onClose }: { onClose: () => void }) {
  const [domain, setDomain] = useState("");
  const [acceptedDomain, setAcceptedDomain] = useState<string>();
  const inputRef = useRef<HTMLInputElement>(null);
  const request = useMutation({
    mutationFn: async (canonicalDomain: string) => {
      const result = await requestDomainCrawl({ path: { canonical_domain: canonicalDomain } });
      if (result.error || !result.data) throw new QuickCrawlError(result.response?.status);
      return result.data;
    },
    onSuccess: (data) => setAcceptedDomain(data.canonical_domain),
  });

  useEffect(() => {
    inputRef.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !request.isPending) onClose();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose, request.isPending]);

  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const target = domain.trim();
    if (target) request.mutate(target);
  };
  const error = request.error instanceof QuickCrawlError ? request.error : undefined;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
      <button className="absolute inset-0 bg-background/80" type="button" aria-label="Cancel Quick Crawl" disabled={request.isPending} onClick={onClose} />
      <section className="relative w-full max-w-md rounded-xl border border-outline-variant/40 bg-surface-container p-6 shadow-floating" role="dialog" aria-modal="true" aria-labelledby="quick-crawl-title" aria-describedby="quick-crawl-description">
        <h2 id="quick-crawl-title" className="font-sans text-xl font-semibold text-on-surface">Request Quick Crawl</h2>
        <p id="quick-crawl-description" className="mt-3 text-sm leading-6 text-on-surface-variant">Request the scheduler’s next eligible crawl for an existing enabled domain. Normal crawl safety and policy checks still apply.</p>
        {acceptedDomain ? (
          <div className="mt-5" role="status">
            <p className="text-sm leading-6 text-primary">{acceptedDomain} is eligible for scheduler processing.</p>
            <Button className="mt-6" onClick={onClose}>Done</Button>
          </div>
        ) : (
          <form className="mt-5" onSubmit={submit}>
            <label className="block">
              <span className="font-mono text-xs text-on-surface-variant">Existing enabled domain</span>
              <Input ref={inputRef} className="mt-2 w-full" aria-label="Existing enabled domain" value={domain} onChange={(event) => setDomain(event.target.value)} placeholder="example.com" required disabled={request.isPending} />
            </label>
            {error ? <p className="mt-3 text-sm leading-6 text-error" role="alert">{errorMessage(error.status)}</p> : null}
            <div className="mt-6 flex justify-end gap-3">
              <Button type="button" variant="ghost" disabled={request.isPending} onClick={onClose}>Cancel</Button>
              <Button type="submit" disabled={request.isPending || !domain.trim()}>{request.isPending ? "Requesting…" : "Request crawl"}</Button>
            </div>
          </form>
        )}
      </section>
    </div>
  );
}

function errorMessage(status: number | undefined): string {
  if (status === 404) return "This domain is not in the active corpus. Add it from Domains before requesting a crawl.";
  if (status === 409) return "Crawls are disabled for this domain. Enable its crawl policy before requesting a crawl.";
  if (status === 403) return "Your administrator role cannot request crawls.";
  if (status === 429) return "Too many administrator actions were requested. Please try again shortly.";
  return "The crawl request could not be recorded. Please try again.";
}
