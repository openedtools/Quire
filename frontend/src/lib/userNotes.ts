/**
 * My notes (spec F-NOTE): the person's own notes for a meeting.
 * Saved meetings keep notes in SQLite. During a recording the meeting does not exist yet, so
 * notes live in the crash-recovery store under the recording's session id and are moved into
 * SQLite when the meeting is saved or recovered.
 */
import { invoke } from '@tauri-apps/api/core';
import { indexedDBService, type LiveNotes } from '@/services/indexedDBService';

export interface UserNotes {
  meeting_id: string;
  body_blocks: string;
  body_markdown: string;
  updated_at: string;
}

export async function loadMeetingNotes(meetingId: string): Promise<UserNotes | null> {
  return invoke<UserNotes | null>('api_get_user_notes', { meetingId });
}

/** Save notes for a saved meeting. Empty notes clear them. */
export async function saveMeetingNotes(meetingId: string, bodyBlocks: string, bodyMarkdown: string): Promise<UserNotes | null> {
  return invoke<UserNotes | null>('api_save_user_notes', { meetingId, bodyBlocks, bodyMarkdown });
}

export interface AttachDeps {
  getLiveNotes: (sessionId: string) => Promise<LiveNotes | null>;
  deleteLiveNotes: (sessionId: string) => Promise<void>;
  save: (meetingId: string, bodyBlocks: string, bodyMarkdown: string) => Promise<unknown>;
}

const defaultDeps: AttachDeps = {
  getLiveNotes: (id) => indexedDBService.getLiveNotes(id),
  deleteLiveNotes: (id) => indexedDBService.deleteLiveNotes(id),
  save: saveMeetingNotes,
};

/**
 * Move notes typed during a recording onto the saved meeting. The live copy is removed only
 * after the save succeeded, so a failure leaves the notes recoverable. Returns true when
 * notes were attached.
 */
export async function attachLiveNotes(sessionId: string, meetingId: string, deps: AttachDeps = defaultDeps): Promise<boolean> {
  const live = await deps.getLiveNotes(sessionId);
  if (!live) return false;
  if (live.markdown.trim()) {
    await deps.save(meetingId, live.blocks, live.markdown);
  }
  await deps.deleteLiveNotes(sessionId);
  return live.markdown.trim().length > 0;
}
