'use client';

import { MoreAbout } from './MoreAbout';
import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from './ui/switch';
import { formatTitle, DEFAULT_TITLE_FORMAT } from '@/lib/naming';

interface NamingSettingsData {
  autoName: boolean;
  titleFormat: string;
}

const TOKENS: { token: string; meaning: string }[] = [
  { token: '{date}', meaning: 'the day, as 2026-11-03' },
  { token: '{time}', meaning: 'the start time, as 14.05' },
  { token: '{topic}', meaning: 'what the meeting was about, from the model' },
  { token: '{folder}', meaning: 'the folder, if the meeting is in one' },
];

const buttonClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';

/** Automatic meeting names and their format (plan PL1, PL2). */
export function NamingSettings() {
  const [saved, setSaved] = useState<NamingSettingsData | null>(null);
  const [format, setFormat] = useState('');

  useEffect(() => {
    invoke<NamingSettingsData>('api_naming_get_settings')
      .then((s) => {
        setSaved(s);
        setFormat(s.titleFormat);
      })
      .catch(() => toast.error('The name settings could not be read'));
  }, []);

  const preview = useMemo(
    () => formatTitle(format, { start: new Date(), topic: 'Budget review', folder: 'Garden Club' }),
    [format],
  );

  const save = async (next: NamingSettingsData) => {
    try {
      const s = await invoke<NamingSettingsData>('api_naming_set_settings', { settings: next });
      setSaved(s);
      setFormat(s.titleFormat);
    } catch {
      toast.error('The name settings could not be saved');
    }
  };

  if (!saved) return null;
  return (
    <div className="space-y-4 rounded-card border border-line bg-surface p-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <label htmlFor="auto-name" className="font-medium text-ink">Name meetings automatically</label>
          <p className="text-sm text-ink-muted">When a recording is saved, the summary model names it from the transcript.</p>
          <MoreAbout>
            <p>Only meetings that still have their default name are renamed; a name you typed is never replaced.</p>
          </MoreAbout>
        </div>
        <Switch id="auto-name" checked={saved.autoName} onCheckedChange={(on) => save({ ...saved, autoName: on })} />
      </div>
      <div className="space-y-2">
        <label htmlFor="title-format" className="block text-sm font-medium text-ink">Name format</label>
        <div className="flex gap-2">
          <input
            id="title-format"
            type="text"
            value={format}
            onChange={(e) => setFormat(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && save({ ...saved, titleFormat: format })}
            className="flex-1 px-3 py-2 border border-line-strong rounded-control bg-paper text-ink focus:outline-none focus:ring-2 focus:ring-accent"
            aria-describedby="title-format-help title-format-preview"
          />
          <button
            className={buttonClass}
            disabled={format.trim() === saved.titleFormat}
            onClick={() => save({ ...saved, titleFormat: format })}
          >
            Save
          </button>
          <button
            className={buttonClass}
            disabled={saved.titleFormat === DEFAULT_TITLE_FORMAT && format === DEFAULT_TITLE_FORMAT}
            onClick={() => save({ ...saved, titleFormat: DEFAULT_TITLE_FORMAT })}
          >
            Reset
          </button>
        </div>
        <p id="title-format-preview" className="text-sm text-ink">
          Example: <span className="font-medium">{preview}</span>
        </p>
        <ul id="title-format-help" className="text-sm text-ink-muted space-y-0.5">
          {TOKENS.map((t) => (
            <li key={t.token}>
              <code className="text-ink">{t.token}</code> {t.meaning}
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
