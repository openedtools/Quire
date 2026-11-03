import { invoke } from '@tauri-apps/api/core';
import type { MeetingSpeaker } from '@/types';

/** "S3" -> 3, anything else -> null */
export function speakerNumber(key: string): number | null {
  const match = /^S([1-9]\d*)$/.exec(key);
  return match ? Number(match[1]) : null;
}

/** The name shown for a speaker: their person's name, or "Speaker N" until named. */
export function speakerDisplayName(key: string, speaker?: Pick<MeetingSpeaker, 'person_name'>): string {
  if (speaker?.person_name) return speaker.person_name;
  const n = speakerNumber(key);
  return n ? `Speaker ${n}` : key;
}

/** key -> display name for every speaker in the list. */
export function speakerNameMap(speakers: MeetingSpeaker[]): Record<string, string> {
  const names: Record<string, string> = {};
  for (const s of speakers) names[s.speaker_key] = speakerDisplayName(s.speaker_key, s);
  return names;
}

/**
 * "Prof Smith: " for a labelled line, "" for an unlabelled one, so meetings without
 * speakers produce exactly the same text as before.
 */
export function speakerPrefix(key: string | undefined | null, names: Record<string, string>): string {
  if (!key) return '';
  return `${names[key] ?? speakerDisplayName(key)}: `;
}

/** Fresh names straight from the database (used when copying or summarising). */
export async function fetchSpeakerNames(meetingId: string): Promise<Record<string, string>> {
  try {
    const speakers = await invoke<MeetingSpeaker[]>('api_get_meeting_speakers', { meetingId });
    return speakerNameMap(speakers);
  } catch (error) {
    // Names are a nicety; never block a copy or a summary on them.
    console.error('Failed to load speaker names:', error);
    return {};
  }
}

// A fixed palette so a speaker keeps the same colour everywhere in a meeting.
const SPEAKER_COLORS = [
  'bg-speaker-1 text-speaker-1-ink border-speaker-1-ink/30',
  'bg-speaker-2 text-speaker-2-ink border-speaker-2-ink/30',
  'bg-speaker-3 text-speaker-3-ink border-speaker-3-ink/30',
  'bg-speaker-4 text-speaker-4-ink border-speaker-4-ink/30',
  'bg-speaker-5 text-speaker-5-ink border-speaker-5-ink/30',
  'bg-speaker-6 text-speaker-6-ink border-speaker-6-ink/30',
  'bg-speaker-7 text-speaker-7-ink border-speaker-7-ink/30',
  'bg-speaker-8 text-speaker-8-ink border-speaker-8-ink/30',
];

export function speakerColorClasses(key: string): string {
  const n = speakerNumber(key) ?? 1;
  return SPEAKER_COLORS[(n - 1) % SPEAKER_COLORS.length];
}
