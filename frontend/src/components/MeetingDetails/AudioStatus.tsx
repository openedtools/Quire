'use client';

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { AudioLines, MoreHorizontal } from 'lucide-react';
import { toast } from 'sonner';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogTitle } from '@/components/ui/dialog';

export interface MeetingAudio {
  meeting_id: string;
  retention: string | null;
  expires_at: string | null;
  deleted_at: string | null;
  finalized_at: string | null;
  state: string;
  mode: string;
}

function day(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
}

/** The meeting's audio in words (spec F-RET-9). */
export function audioStatusText(audio: MeetingAudio | null, now = new Date()): string {
  if (!audio) return '';
  if (audio.mode === 'transcript_only' || audio.state === 'none') return 'No audio saved';
  if (audio.state === 'deleted') return audio.deleted_at ? `Audio deleted on ${day(audio.deleted_at)}` : 'Audio deleted';
  if (audio.expires_at && audio.retention !== 'keep') {
    return new Date(audio.expires_at) <= now ? 'Audio deletion pending' : `Audio kept until ${day(audio.expires_at)}`;
  }
  return 'Audio kept';
}

const buttonClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';
const dangerClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-danger border-danger text-on-accent hover:opacity-90 disabled:opacity-60';

/** One line under the transcript buttons: what happens to this meeting's audio, with Keep this one
 *  and Delete audio now (spec F-RET-8, F-RET-9). */
export function AudioStatus({ meetingId }: { meetingId: string }) {
  const [audio, setAudio] = useState<MeetingAudio | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setAudio(await invoke<MeetingAudio | null>('api_meeting_audio', { meetingId }));
    } catch (error) {
      console.error('Could not read the audio status:', error);
    }
  }, [meetingId]);

  useEffect(() => {
    refresh();
    let unlisten: (() => void) | undefined;
    listen('retention-swept', () => refresh()).then((u) => { unlisten = u; }).catch(() => {});
    return () => unlisten?.();
  }, [refresh]);

  if (!audio || (!audio.finalized_at && audio.state === 'none')) return null;
  const text = audioStatusText(audio);
  const canAct = audio.state === 'present';

  const keep = async () => {
    try {
      await invoke('api_meeting_keep_audio', { meetingId });
      toast.success('This recording is kept');
      await refresh();
    } catch (error) {
      toast.error('Could not change this recording', { description: String(error) });
    }
  };

  const deleteNow = async () => {
    setBusy(true);
    try {
      const result = await invoke<string>('api_meeting_delete_audio_now', { meetingId });
      if (result === 'deleted') toast.success('Audio deleted', { description: 'The transcript, notes and summary stay.' });
      else if (result === 'pending') toast.warning('A file is in use', { description: 'The audio is deleted as soon as it is free.' });
      else if (result === 'shared') toast.warning('Not deleted', { description: 'Another meeting uses the same recording folder.' });
      else if (result === 'outside') toast.warning('Not deleted', { description: 'The recording is outside your recordings folder.' });
      setConfirming(false);
      await refresh();
    } catch (error) {
      toast.error('Could not delete the audio', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="mt-2 flex items-center justify-center gap-2 text-xs text-ink-muted">
      <AudioLines className="w-4 h-4" aria-hidden="true" />
      <span data-testid="audio-status">{text}</span>
      {canAct && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button type="button" aria-label="Audio options" className="min-h-6 min-w-6 rounded-control hover:bg-surface-hover inline-flex items-center justify-center">
              <MoreHorizontal className="w-4 h-4" aria-hidden="true" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="center">
            {audio.retention !== 'keep' && <DropdownMenuItem onSelect={keep}>Keep this one</DropdownMenuItem>}
            <DropdownMenuItem onSelect={() => setConfirming(true)}>Delete audio now</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
      <Dialog open={confirming} onOpenChange={setConfirming}>
        <DialogContent>
          <DialogTitle>Delete this recording&apos;s audio?</DialogTitle>
          <DialogDescription>
            The audio file is removed from this computer. The transcript, notes and summary stay. Speakers can no longer be
            detected for this meeting. This cannot be undone.
          </DialogDescription>
          <DialogFooter>
            <button type="button" className={buttonClass} disabled={busy} onClick={() => setConfirming(false)}>
              Cancel
            </button>
            <button type="button" className={dangerClass} disabled={busy} onClick={deleteNow}>
              Delete audio
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
