'use client';

import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';

/** App-wide notices from the retention sweep (spec F-RET-13): many recordings due at once wait
 *  for a yes, and a clock set backwards pauses deletion. */
export function RetentionPrompts() {
  useEffect(() => {
    const stops: Array<() => void> = [];
    listen<number | { count: number; ids: string[] }>('retention-needs-confirmation', (event) => {
      // The core sends the exact meetings; a yes covers those and nothing that became due since
      const count = typeof event.payload === 'number' ? event.payload : event.payload.count;
      const ids = typeof event.payload === 'number' ? undefined : event.payload.ids;
      toast.warning(`${count} recordings are due for audio deletion at once`, {
        id: 'retention-confirm',
        description: 'This can happen after a long time away or a clock change. Their transcripts stay either way.',
        duration: Infinity,
        action: {
          label: 'Delete their audio',
          onClick: async () => {
            try {
              const report = await invoke<{ deleted: number; pending: number; needs_confirmation: number } | null>('api_retention_confirm_due', ids ? { count, ids } : { count });
              if (!report) {
                toast.warning('Not deleted yet', { description: 'Speaker detection or another task is using the audio. Try again in a moment.' });
              } else if (report.needs_confirmation > 0) {
                toast.warning('The recordings due changed', { description: 'Nothing was deleted. You will be asked again.' });
              } else {
                const pending = report.pending ? ` ${report.pending} will follow when their files are free.` : '';
                toast.success(`Audio deleted for ${report.deleted} recording${report.deleted === 1 ? '' : 's'}`, {
                  description: `The transcripts, notes and summaries stay.${pending}`,
                });
              }
            } catch (error) {
              toast.error('Could not delete the audio', { description: String(error) });
            }
          },
        },
        cancel: { label: 'Not now', onClick: () => {} },
      });
    }).then((u) => stops.push(u)).catch(() => {});
    listen('retention-clock-paused', () => {
      toast.warning('Audio deletion is paused', {
        id: 'retention-clock',
        description: "This computer's clock is earlier than before. If today's date and time are right, resume deletion.",
        duration: Infinity,
        action: {
          label: 'Resume deletion',
          onClick: async () => {
            try {
              await invoke('api_retention_resume_after_clock');
              toast.success('Audio deletion resumed');
            } catch (error) {
              toast.error('Could not resume', { description: String(error) });
            }
          },
        },
        cancel: { label: 'Not now', onClick: () => {} },
      });
    }).then((u) => stops.push(u)).catch(() => {});
    return () => stops.forEach((stop) => stop());
  }, []);
  return null;
}
