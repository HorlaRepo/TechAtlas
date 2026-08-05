import { Button, Card } from "@techatlas/ui";
import type { ReactNode } from "react";

type AdminOperationsStateProps = {
  title: string;
  description: string;
  onRetry?: () => void;
};

export function AdminOperationsState({ title, description, onRetry }: AdminOperationsStateProps) {
  return (
    <Card className="flex min-h-72 flex-col items-start justify-center gap-4" role="status">
      <div>
        <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">SYSTEM OPS</p>
        <h1 className="mt-2 font-sans text-2xl font-semibold text-on-surface">{title}</h1>
        <p className="mt-2 max-w-xl text-sm leading-6 text-on-surface-variant">{description}</p>
      </div>
      {onRetry ? <Button variant="secondary" onClick={onRetry}>Try again</Button> : null}
    </Card>
  );
}

type AdminOperationsSectionProps = {
  children?: ReactNode;
  title: string;
  description: string;
};

export function AdminOperationsSection({ children, title, description }: AdminOperationsSectionProps) {
  return (
    <header>
      <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">SYSTEM OPS</p>
      <h1 className="mt-2 font-sans text-3xl font-semibold tracking-[-0.02em] text-on-surface sm:text-4xl">{title}</h1>
      <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">{description}</p>
      {children}
    </header>
  );
}
