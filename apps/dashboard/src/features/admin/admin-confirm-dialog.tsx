import { Button } from "@techatlas/ui";
import { useEffect, useRef } from "react";

type AdminConfirmDialogProps = {
  title: string;
  description: string;
  confirmLabel: string;
  isPending?: boolean;
  onCancel: () => void;
  onConfirm: () => void;
};

export function AdminConfirmDialog({
  title,
  description,
  confirmLabel,
  isPending = false,
  onCancel,
  onConfirm,
}: AdminConfirmDialogProps) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    cancelRef.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !isPending) onCancel();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [isPending, onCancel]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4" role="presentation">
      <button className="absolute inset-0 bg-background/80" type="button" aria-label="Cancel confirmation" disabled={isPending} onClick={onCancel} />
      <section className="relative w-full max-w-md rounded-xl border border-outline-variant/40 bg-surface-container p-6 shadow-floating" role="dialog" aria-modal="true" aria-labelledby="admin-confirm-title" aria-describedby="admin-confirm-description">
        <h2 id="admin-confirm-title" className="font-sans text-xl font-semibold text-on-surface">{title}</h2>
        <p id="admin-confirm-description" className="mt-3 text-sm leading-6 text-on-surface-variant">{description}</p>
        <div className="mt-6 flex justify-end gap-3">
          <Button ref={cancelRef} variant="ghost" disabled={isPending} onClick={onCancel}>Cancel</Button>
          <Button disabled={isPending} onClick={onConfirm}>{isPending ? "Working…" : confirmLabel}</Button>
        </div>
      </section>
    </div>
  );
}
