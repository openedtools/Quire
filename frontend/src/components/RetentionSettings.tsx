'use client';

import { MoreAbout } from './MoreAbout';
import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { Switch } from './ui/switch';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle } from './ui/dialog';

export type Retention = 'off' | 'h24' | 'd7' | 'd14' | 'd30' | 'keep';

interface RetentionSettingsData {
  default_retention: Retention;
  detect_on_save: boolean;
}

interface ApplyPreview {
  recordings: number;
  bytes: number;
  due_now: number;
}

interface KeptRecording {
  meeting_id: string;
  title: string;
  reason: 'shared' | 'outside';
}

interface RetentionStatus {
  last_sweep_at: string | null;
  unmanaged: string[];
  kept?: KeptRecording[];
  /** The daily audio check; null where there is none (the Mac for now) */
  scheduled?: { wanted: boolean; exists: boolean } | null;
}

export const RETENTION_CHOICES: { value: Retention; label: string; description: string }[] = [
  { value: 'off', label: 'Off', description: 'Delete the audio as soon as the transcript is safely saved.' },
  { value: 'h24', label: '24 hours', description: 'Delete the audio a day after the recording ends.' },
  { value: 'd7', label: '7 days', description: 'Delete the audio a week after the recording ends.' },
  { value: 'd14', label: '14 days', description: 'Delete the audio two weeks after the recording ends.' },
  { value: 'd30', label: '30 days', description: 'Delete the audio a month after the recording ends.' },
  { value: 'keep', label: 'Keep', description: 'Keep the audio until you delete the meeting.' },
];

const label = (r: Retention) => RETENTION_CHOICES.find((c) => c.value === r)?.label ?? r;
const megabytes = (bytes: number) => `${Math.max(0.1, bytes / 1_000_000).toFixed(1)} MB`;

const buttonClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';
const primaryClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-accent-strong border-accent-strong text-on-accent hover:opacity-90 disabled:opacity-60';

/** Audio retention (spec F-RET-1, F-RET-8, F-RET-10, F-RET-14). */
export function RetentionSettings() {
  const [settings, setSettings] = useState<RetentionSettingsData | null>(null);
  const [status, setStatus] = useState<RetentionStatus | null>(null);
  const [preview, setPreview] = useState<ApplyPreview | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      setSettings(await invoke<RetentionSettingsData>('api_retention_get_settings'));
      setStatus(await invoke<RetentionStatus>('api_retention_status'));
    } catch (error) {
      console.error('Could not read the retention settings:', error);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const keepAudio = async (meetingId: string) => {
    try {
      await invoke('api_meeting_keep_audio', { meetingId });
      toast.success('The audio is kept');
      await load();
    } catch (error) {
      toast.error('Could not change it', { description: String(error) });
    }
  };

  const save = async (next: RetentionSettingsData) => {
    try {
      setSettings(await invoke<RetentionSettingsData>('api_retention_set_settings', { settings: next }));
    } catch (error) {
      toast.error('Could not save the retention setting', { description: String(error) });
    }
  };

  const askToApply = async () => {
    if (!settings) return;
    try {
      setPreview(await invoke<ApplyPreview>('api_retention_apply_to_existing', { retention: settings.default_retention, apply: false }));
    } catch (error) {
      toast.error('Could not count the recordings', { description: String(error) });
    }
  };

  const apply = async () => {
    if (!settings) return;
    setBusy(true);
    try {
      const done = await invoke<ApplyPreview>('api_retention_apply_to_existing', { retention: settings.default_retention, apply: true });
      toast.success(`${label(settings.default_retention)} applied to ${done.recordings} recording${done.recordings === 1 ? '' : 's'}`);
      setPreview(null);
    } catch (error) {
      toast.error('Could not apply the setting', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  if (!settings) return null;

  return (
    <section aria-labelledby="retention-title" className="space-y-4 p-4 border rounded-lg">
      <div>
        <h4 id="retention-title" className="font-medium text-ink">Audio retention</h4>
        <p className="text-sm text-ink-muted">How long the audio of a recording is kept. Transcripts, notes and summaries stay until you delete the meeting.</p>
      </div>

      <div role="radiogroup" aria-labelledby="retention-title" className="grid gap-2 sm:grid-cols-2">
        {RETENTION_CHOICES.map((choice) => {
          const selected = settings.default_retention === choice.value;
          return (
            <button
              key={choice.value}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => save({ ...settings, default_retention: choice.value })}
              className={`text-left p-3 rounded-control border transition-colors ${
                selected ? 'border-accent-strong bg-accent-tint' : 'border-line-strong bg-surface hover:bg-surface-hover'
              }`}
            >
              <div className="text-sm font-medium text-ink">{choice.label}</div>
              <div className="text-xs text-ink-muted">{choice.description}</div>
            </button>
          );
        })}
      </div>

      <p className="text-sm text-ink-muted">
        Off saves the audio briefly so speakers can be detected, then deletes it. Transcript only, a switch next to
        Record, never saves audio at all.
      </p>

      <p className="text-sm text-ink-muted">
        Copies outside the app&apos;s reach, such as a cloud service&apos;s recently deleted files, backups and disk recovery
        tools, are not affected.
      </p>
      <MoreAbout>
        <p>
          A change applies to new recordings. Deleting removes the audio from this computer. Deletion happens while the app
          is running; a deadline that passes while it is closed is handled the next time it starts.
        </p>
      </MoreAbout>

      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={buttonClass} onClick={askToApply}>
          Apply to existing recordings
        </button>
      </div>

      <div className="flex items-center justify-between gap-4 pt-2 border-t border-line">
        <div>
          <div className="text-sm font-medium text-ink">Detect speakers when a recording ends</div>
          <div className="text-sm text-ink-muted">
            {settings.detect_on_save
              ? 'Speakers are detected before any audio is deleted. Needs the speaker model.'
              : settings.default_retention === 'off'
                ? 'With Off, speakers cannot be detected afterwards.'
                : 'Detect speakers yourself from a meeting while its audio is kept.'}
          </div>
        </div>
        <Switch
          aria-label="Detect speakers when a recording ends"
          checked={settings.detect_on_save}
          onCheckedChange={(on) => save({ ...settings, detect_on_save: on })}
        />
      </div>

      {status && status.scheduled != null && (
        <p className="text-sm text-ink-muted" role="status">
          {status.scheduled.wanted && status.scheduled.exists
            ? 'Daily audio check: on. Windows runs it once a day, so audio is deleted on time even while Quire is closed.'
            : status.scheduled.wanted
              ? 'Daily audio check: Windows did not accept it, so audio is deleted only while Quire is running.'
              : status.scheduled.exists
                ? 'Daily audio check: being removed, because no audio is waiting to be deleted.'
                : 'Daily audio check: off, because no audio is waiting to be deleted. Audio is checked each time Quire runs.'}
        </p>
      )}

      {status && (status.kept?.length ?? 0) > 0 && (
        <div className="pt-2 border-t border-line">
          <div className="text-sm font-medium text-ink">Past their deadline but kept</div>
          <p className="text-sm text-ink-muted mb-2">
            Automatic deletion never touches a folder another meeting uses, or one outside your recordings folders. Delete these files yourself, or keep the audio.
          </p>
          <ul className="space-y-2" aria-label="Recordings kept by the safety rules">
            {status.kept!.map((k) => (
              <li key={k.meeting_id} className="flex items-center justify-between gap-3 text-sm">
                <span className="text-ink">
                  {k.title}{' '}
                  <span className="text-ink-muted">({k.reason === 'shared' ? 'its folder is shared with another meeting' : 'outside the recordings folders'})</span>
                </span>
                <button type="button" className={buttonClass} onClick={() => keepAudio(k.meeting_id)}>
                  Keep the audio
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      {status && status.unmanaged.length > 0 && (
        <div className="pt-2 border-t border-line">
          <div className="text-sm font-medium text-ink">Recording folders no meeting uses</div>
          <p className="text-sm text-ink-muted mb-1">Kept as they are and never deleted automatically.</p>
          <ul className="text-xs font-mono text-ink-muted space-y-0.5" aria-label="Unmanaged recording folders">
            {status.unmanaged.map((name) => (
              <li key={name} className="break-all">{name}</li>
            ))}
          </ul>
        </div>
      )}

      <Dialog open={preview !== null} onOpenChange={(open) => !open && setPreview(null)}>
        <DialogContent>
          <DialogTitle>Apply {label(settings.default_retention)} to existing recordings?</DialogTitle>
          <DialogDescription>
            {preview && preview.recordings === 0
              ? 'No existing recording has audio to change.'
              : preview &&
                `${preview.recordings} recording${preview.recordings === 1 ? '' : 's'} with ${megabytes(preview.bytes)} of audio will use this setting.` +
                  (preview.due_now > 0 ? ` ${preview.due_now} of them are past the new deadline, and their audio will be deleted now.` : '')}
          </DialogDescription>
          <DialogFooter>
            <button type="button" className={buttonClass} disabled={busy} onClick={() => setPreview(null)}>
              Cancel
            </button>
            {preview && preview.recordings > 0 && (
              <button type="button" className={primaryClass} disabled={busy} onClick={apply}>
                Apply
              </button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </section>
  );
}
