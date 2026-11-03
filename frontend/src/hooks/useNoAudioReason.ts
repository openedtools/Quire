import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface MeetingAudioState {
  state: string;
  mode: string;
}

/** Why a meeting with a recording has no audio to work with, or null when it has audio
 *  (spec F-TO-3). Features that need audio are disabled with this reason. */
export function noAudioReasonFor(audio: MeetingAudioState | null): string | null {
  if (!audio) return null;
  if (audio.mode === 'transcript_only') return 'This meeting was recorded as transcript only, so no audio was saved.';
  if (audio.state === 'deleted') return "This meeting's audio was deleted, so there is no audio to work with.";
  if (audio.state === 'none') return 'This meeting has no saved audio.';
  return null;
}

export function useNoAudioReason(meetingId: string | undefined): string | null {
  const [reason, setReason] = useState<string | null>(null);
  useEffect(() => {
    if (!meetingId) return;
    let alive = true;
    const load = () =>
      invoke<MeetingAudioState | null>('api_meeting_audio', { meetingId })
        .then((audio) => alive && setReason(noAudioReasonFor(audio)))
        .catch(() => {});
    load();
    let unlisten: (() => void) | undefined;
    listen('retention-swept', load).then((u) => { unlisten = u; }).catch(() => {});
    return () => {
      alive = false;
      unlisten?.();
    };
  }, [meetingId]);
  return reason;
}
