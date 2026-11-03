'use client';

import { useEffect, useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { toast } from 'sonner';

/** One-click copy that says "Copied" for two seconds, as on the iPhone (plan PL6). `text` may be
 *  read when clicked, so it is always the latest. */
export function CopyButton({ text, what, className = '' }: { text: string | (() => Promise<string> | string); what: string; className?: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 2000);
    return () => clearTimeout(t);
  }, [copied]);

  const copy = async () => {
    try {
      const value = typeof text === 'function' ? await text() : text;
      if (!value.trim()) {
        toast.info(`There is nothing in the ${what} to copy yet`);
        return;
      }
      await navigator.clipboard.writeText(value);
      setCopied(true);
    } catch {
      toast.error(`The ${what} could not be copied`);
    }
  };

  return (
    <button
      type="button"
      onClick={copy}
      aria-label={copied ? `${what[0].toUpperCase()}${what.slice(1)} copied` : `Copy the ${what}`}
      className={`inline-flex min-h-6 items-center gap-1 text-xs text-ink hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${className}`}
    >
      {copied ? <Check className="h-3 w-3" aria-hidden="true" /> : <Copy className="h-3 w-3" aria-hidden="true" />}
      {copied ? 'Copied' : 'Copy'}
    </button>
  );
}
