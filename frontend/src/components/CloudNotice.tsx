'use client';

import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { cloudStatus } from '@/lib/cloud';

/** One notice after an upgrade that kept cloud services on, with one click to turn them off (F-MOD-5). */
export function CloudNotice() {
  useEffect(() => {
    let tries = 0;
    const check = () =>
      cloudStatus()
        .then((status) => {
          if (!status.notice) return;
          const dismiss = () => invoke('api_cloud_dismiss_notice').catch(() => {});
          toast.info('Cloud services are on', {
            id: 'cloud-notice',
            description: `This computer already sent summaries to ${status.notice}, so sending data to cloud services stays allowed.`,
            duration: Infinity,
            action: {
              label: 'Turn off',
              onClick: async () => {
                try {
                  await invoke('api_set_cloud_allowed', { allowed: false });
                  toast.success('Cloud services are off', { description: 'Summaries now need a model on this computer.' });
                } catch (error) {
                  toast.error('Could not change the setting', { description: String(error) });
                }
              },
            },
            cancel: { label: 'Keep on', onClick: dismiss },
            onDismiss: dismiss,
          });
        })
        // The database may not be ready on the first launch; try a few times
        .catch(() => {
          tries += 1;
          if (tries < 5) setTimeout(check, 3000);
        });
    check();
  }, []);
  return null;
}
