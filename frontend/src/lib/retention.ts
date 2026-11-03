import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';

interface FinalizeOutcome {
  finalized: boolean;
  retention: string | null;
  audio_state: string;
  deleted_now: boolean;
  kept_because: string | null;
}

/**
 * Starts a saved recording's retention (spec F-RET-2, F-RET-3). Call after the meeting and its
 * notes are saved, and after a recovery. With Off the audio is deleted here, once the save is
 * confirmed; if it had to be kept the person is told why. Never throws: a failure leaves the audio
 * in place and the sweep finalizes the meeting later.
 */
export async function finalizeRecording(meetingId: string): Promise<FinalizeOutcome | null> {
  try {
    const outcome = await invoke<FinalizeOutcome>('api_finalize_recording', { meetingId });
    if (outcome?.kept_because) {
      toast.warning('The audio was kept', { description: outcome.kept_because });
    }
    return outcome;
  } catch (error) {
    console.error('Could not finalize the recording:', error);
    return null;
  }
}
