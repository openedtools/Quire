'use client';

import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';

/** Says when a model already on this computer did not match its pinned version (plan PC4). */
export function ModelNotice() {
  useEffect(() => {
    let stop: (() => void) | undefined;
    listen<string[]>('models-set-aside', (event) => {
      const names = event.payload.join(', ');
      toast.warning('A model needs downloading again', {
        id: 'models-set-aside',
        description: `${names} did not match the version Quire checks against, so it will not be used. Download it again in Settings, Models. The old file was kept next to it with ".unverified" at the end of its name.`,
        duration: Infinity,
      });
    })
      .then((u) => (stop = u))
      .catch(() => {});
    return () => stop?.();
  }, []);
  return null;
}
