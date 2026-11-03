'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from './ui/switch';

type Preferences = Record<string, unknown> & { transcript_only?: boolean };

export const TRANSCRIPT_ONLY_NOTE =
  "Transcript only saves no audio file. Whether you need people's consent depends on where you and they are, and it does not change with this setting.";

/** "Transcript only: do not save audio" for the next recording, remembered (spec F-TO-1, F-TO-4). */
export function TranscriptOnlySwitch({ compact = false }: { compact?: boolean }) {
  const [prefs, setPrefs] = useState<Preferences | null>(null);

  useEffect(() => {
    invoke<Preferences>('get_recording_preferences')
      .then((p) => setPrefs(p ?? {}))
      .catch((error) => console.error('Could not read the recording preferences:', error));
  }, []);

  if (!prefs) return null;
  const on = prefs.transcript_only === true;

  const toggle = async (next: boolean) => {
    // Read the preferences again, so a change made in Settings meanwhile is not overwritten
    const fresh = await invoke<Preferences>('get_recording_preferences').catch(() => prefs);
    const updated = { ...(fresh ?? prefs), transcript_only: next };
    try {
      await invoke('set_recording_preferences', { preferences: updated });
      setPrefs(updated);
    } catch (error) {
      toast.error('Could not change the setting', { description: String(error) });
    }
  };

  return (
    <div className={`flex flex-col items-center gap-1 ${compact ? '' : 'max-w-md text-center'}`}>
      <label className="inline-flex items-center gap-2 text-sm text-ink">
        <Switch aria-label="Transcript only: do not save audio" checked={on} onCheckedChange={toggle} />
        Transcript only: do not save audio
      </label>
      {on && <p className="text-xs text-ink-muted">{TRANSCRIPT_ONLY_NOTE}</p>}
    </div>
  );
}
