'use client';

import { useEffect, useState } from 'react';
import { BRAND } from '@/lib/brand.generated';
import { FIRST_RUN_NOTICE, acknowledgeFirstRun, firstRunAcknowledged } from '@/lib/consent';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from './ui/dialog';

/** Shown once, after onboarding, and readable again in Settings under Privacy (spec F-CON-6).
 *  A reminder, not enforcement. */
export function FirstRunNotice({ ready }: { ready: boolean }) {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!ready) return;
    firstRunAcknowledged().then((done) => setOpen(!done));
  }, [ready]);

  const accept = async () => {
    try {
      await acknowledgeFirstRun();
    } catch (error) {
      console.error('Could not save the notice:', error);
    }
    setOpen(false);
  };

  return (
    <Dialog open={open} onOpenChange={(next) => !next && accept()}>
      <DialogContent>
        <DialogTitle>Before you use {BRAND.name}</DialogTitle>
        <DialogDescription>{FIRST_RUN_NOTICE}</DialogDescription>
        <p className="text-sm text-ink-muted">You can read this again in Settings, under Privacy.</p>
        <div className="flex justify-end">
          <button type="button" onClick={accept} className="min-h-9 px-4 rounded-control border text-sm bg-accent-strong border-accent-strong text-on-accent hover:opacity-90">
            Continue
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
