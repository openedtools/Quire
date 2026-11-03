'use client';

import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle } from '@/components/ui/dialog';

export interface ConfirmRequest {
  title: string;
  body: string;
  confirmLabel: string;
}

type Pending = ConfirmRequest & { resolve: (yes: boolean) => void };
let show: ((request: Pending) => void) | null = null;

/** Asks the person one question in a dialog; resolves true only on the confirm button. */
export function askConfirm(request: ConfirmRequest): Promise<boolean> {
  return new Promise((resolve) => {
    if (!show) {
      resolve(false);
      return;
    }
    show({ ...request, resolve });
  });
}

const buttonClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover';
const primaryClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-accent-strong border-accent-strong text-on-accent hover:opacity-90';

/** Mounted once in the layout; shows the questions `askConfirm` asks. */
export function ConfirmHost() {
  const [pending, setPending] = useState<Pending | null>(null);
  useEffect(() => {
    show = setPending;
    return () => {
      show = null;
    };
  }, []);
  const answer = (yes: boolean) => {
    pending?.resolve(yes);
    setPending(null);
  };
  return (
    <Dialog open={!!pending} onOpenChange={(open) => !open && answer(false)}>
      <DialogContent>
        <DialogTitle>{pending?.title}</DialogTitle>
        <DialogDescription>{pending?.body}</DialogDescription>
        <DialogFooter>
          <button type="button" className={buttonClass} onClick={() => answer(false)}>
            Cancel
          </button>
          <button type="button" className={primaryClass} onClick={() => answer(true)}>
            {pending?.confirmLabel}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
