'use client';

import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { X } from 'lucide-react';
import { REMINDER_EVENT, setReminderEnabled } from '@/lib/consent';

/** A small, non-blocking card near the record controls when a recording starts (spec F-CON-1 to
 *  F-CON-4). Closing it without choosing only dismisses it for this recording. */
export function ConsentReminder() {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [never, setNever] = useState(false);

  useEffect(() => {
    const show = () => {
      setNever(false);
      setOpen(true);
    };
    window.addEventListener(REMINDER_EVENT, show);
    return () => window.removeEventListener(REMINDER_EVENT, show);
  }, []);

  if (!open) return null;

  const gotIt = async () => {
    if (never) {
      try {
        await setReminderEnabled(false);
      } catch (error) {
        console.error('Could not save the reminder choice:', error);
      }
    }
    setOpen(false);
  };

  return (
    <aside
      role="dialog"
      aria-modal="false"
      aria-labelledby="consent-reminder-title"
      className="fixed bottom-36 left-1/2 -translate-x-1/2 z-40 w-[min(26rem,calc(100vw-2rem))] bg-paper border border-line-strong rounded-card p-4"
    >
      <div className="flex items-start justify-between gap-2">
        <h2 id="consent-reminder-title" className="text-sm font-semibold text-ink">Tell people you&apos;re recording</h2>
        <button type="button" aria-label="Close" onClick={() => setOpen(false)} className="min-h-6 min-w-6 rounded-control hover:bg-surface-hover inline-flex items-center justify-center">
          <X className="w-4 h-4" aria-hidden="true" />
        </button>
      </div>
      <p className="mt-1 text-sm text-ink-muted">
        Rules about recording and transcribing conversations differ by place, and some can apply even when no audio is
        saved. A good habit: tell everyone, get a clear yes, and follow your organization&apos;s policy. This is general
        guidance, not legal advice.
      </p>
      <label className="mt-3 flex items-center gap-2 text-sm text-ink">
        <input type="checkbox" checked={never} onChange={(e) => setNever(e.target.checked)} className="h-4 w-4" />
        Never show this again
      </label>
      <div className="mt-3 flex items-center justify-between gap-2">
        <button
          type="button"
          onClick={() => {
            setOpen(false);
            router.push('/consent');
          }}
          className="text-sm text-accent-strong underline underline-offset-2"
        >
          Read more
        </button>
        <button type="button" onClick={gotIt} className="min-h-9 px-4 rounded-control border text-sm bg-accent-strong border-accent-strong text-on-accent hover:opacity-90">
          Got it
        </button>
      </div>
    </aside>
  );
}
