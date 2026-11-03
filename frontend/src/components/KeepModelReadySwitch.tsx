'use client';

import { MoreAbout } from './MoreAbout';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from './ui/switch';

type Preferences = Record<string, unknown> & { keep_speech_model_ready?: boolean };

/** "Keep the speech model ready" (plan PL3): Parakeet stays loaded while the app is open. */
export function KeepModelReadySwitch() {
  const [prefs, setPrefs] = useState<Preferences | null>(null);

  useEffect(() => {
    invoke<Preferences>('get_recording_preferences')
      .then((p) => setPrefs(p ?? {}))
      .catch((error) => console.error('Could not read the recording preferences:', error));
  }, []);

  if (!prefs) return null;
  const on = prefs.keep_speech_model_ready !== false;

  const toggle = async (next: boolean) => {
    // Read the preferences again, so a change made elsewhere meanwhile is not overwritten
    const fresh = await invoke<Preferences>('get_recording_preferences').catch(() => prefs);
    const updated = { ...(fresh ?? prefs), keep_speech_model_ready: next };
    try {
      await invoke('set_recording_preferences', { preferences: updated });
      setPrefs(updated);
    } catch (error) {
      toast.error('Could not change the setting', { description: String(error) });
    }
  };

  return (
    <div className="mt-4 flex items-start justify-between gap-4 rounded-card border border-line bg-surface p-4">
      <div>
        <label htmlFor="keep-model-ready" className="font-medium text-ink">Keep the speech model ready</label>
        <p className="text-sm text-ink-muted">A recording starts transcribing at once.</p>
        <MoreAbout>
          <p>Parakeet stays loaded while Quire is open. It uses about 1 GB of memory; turn this off on a computer with 8 GB or less.</p>
        </MoreAbout>
      </div>
      <Switch id="keep-model-ready" checked={on} onCheckedChange={toggle} />
    </div>
  );
}
