'use client';

/** The longer explanation of a Settings section, closed until asked for (plan PN5). Never used
 *  for a warning that needs action. */
export function MoreAbout({ children, className = '' }: { children: React.ReactNode; className?: string }) {
  return (
    <details className={`group text-sm ${className}`}>
      <summary className="inline-flex min-h-6 cursor-pointer list-none items-center gap-1 text-accent-strong hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden">
        <span aria-hidden="true" className="inline-block transition-transform group-open:rotate-90">›</span>
        More about this
      </summary>
      <div className="mt-1 space-y-1 text-ink-muted">{children}</div>
    </details>
  );
}
