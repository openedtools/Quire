'use client';

import dynamic from 'next/dynamic';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Block } from '@blocknote/core';
import { NotebookPen } from 'lucide-react';
import { loadMeetingNotes, saveMeetingNotes } from '@/lib/userNotes';
import { getRecordTarget } from '@/lib/notesFirst';
import { indexedDBService } from '@/services/indexedDBService';
import { useTranscripts } from '@/contexts/TranscriptContext';
import { parseBlocks, type NotesContent } from './NotesEditor';
import { AskBox } from '@/components/Ask/AskBox';
import { CopyButton } from '@/components/CopyButton';
import { saveStatusLabel, useDebouncedSave, type SaveStatus } from './useDebouncedSave';

const NotesEditor = dynamic(() => import('./NotesEditor'), { ssr: false });

function NotesHeader({ status, hint, copy }: { status: SaveStatus; hint: string; copy?: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-3 px-6 pt-4 pb-2">
      <div className="flex items-center gap-2 text-sm text-ink-muted">
        <NotebookPen className="w-4 h-4" aria-hidden="true" />
        <span>{hint}</span>
      </div>
      <div className="flex items-center gap-3">
        <span className="text-xs text-ink-muted" aria-live="polite">{saveStatusLabel(status)}</span>
        {copy}
      </div>
    </div>
  );
}

/** My notes for a saved meeting, stored in the database (spec F-NOTE-1, F-NOTE-2). */
export function MeetingNotesPanel({ meetingId, autoFocus = false }: { meetingId: string; autoFocus?: boolean }) {
  const [initial, setInitial] = useState<{ blocks?: Block[] } | null>(null);
  const save = useCallback(
    (content: NotesContent) => saveMeetingNotes(meetingId, content.blocks, content.markdown),
    [meetingId],
  );
  const { schedule, status } = useDebouncedSave(save, 1000);
  // The latest text typed, so Copy never waits for the save (PL6)
  const latest = useRef<string | null>(null);
  const onChange = useCallback((content: NotesContent) => {
    latest.current = content.markdown;
    schedule(content);
  }, [schedule]);

  useEffect(() => {
    let cancelled = false;
    setInitial(null);
    loadMeetingNotes(meetingId)
      .then((notes) => { if (!cancelled) setInitial({ blocks: parseBlocks(notes?.body_blocks) }); })
      .catch((error) => {
        console.error('Failed to load notes:', error);
        if (!cancelled) setInitial({});
      });
    return () => { cancelled = true; };
  }, [meetingId]);

  return (
    <div className="flex h-full flex-col">
      <NotesHeader
        status={status}
        hint="Your notes. The summary builds on them."
        copy={<CopyButton what="notes" text={async () => latest.current ?? (await loadMeetingNotes(meetingId))?.body_markdown ?? ''} />}
      />
      <div className="flex-1 overflow-y-auto px-2 pb-6">
        {initial && <NotesEditor key={meetingId} initialBlocks={initial.blocks} onChange={onChange} autoFocus={autoFocus} />}
      </div>
    </div>
  );
}

/**
 * My notes during a recording. A recording started from a notes-first meeting writes straight to
 * that meeting, which already exists (spec F-NM-3). Otherwise the meeting is not saved yet, so
 * notes go to the crash-recovery store under the recording session and move to the meeting when
 * it is saved (spec F-NOTE-2).
 */
export function LiveNotesPanel() {
  const { currentMeetingId: sessionId } = useTranscripts();
  const target = useMemo(() => (sessionId ? getRecordTarget() : null), [sessionId]);
  return target ? <TargetMeetingNotes meetingId={target.id} /> : <SessionNotes sessionId={sessionId} />;
}

function TargetMeetingNotes({ meetingId }: { meetingId: string }) {
  const getLiveNotes = useCallback(async () => (await loadMeetingNotes(meetingId))?.body_markdown, [meetingId]);
  return (
    <aside aria-label="My notes" className="flex h-full w-[380px] flex-shrink-0 flex-col border-l border-line bg-paper">
      <div className="px-6 pt-4">
        <h2 className="text-lg text-ink">My notes</h2>
      </div>
      <div className="min-h-0 flex-1">
        <MeetingNotesPanel meetingId={meetingId} />
      </div>
      <AskBox getLiveNotes={getLiveNotes} />
    </aside>
  );
}

function SessionNotes({ sessionId }: { sessionId: string | null }) {
  const [initial, setInitial] = useState<{ blocks?: Block[] } | null>(null);
  const save = useCallback(
    async (content: NotesContent) => {
      if (sessionId) await indexedDBService.saveLiveNotes(sessionId, content.blocks, content.markdown);
    },
    [sessionId],
  );
  const { schedule, status } = useDebouncedSave(save, 500);
  const getLiveNotes = useCallback(
    async () => (sessionId ? (await indexedDBService.getLiveNotes(sessionId))?.markdown : undefined),
    [sessionId],
  );

  useEffect(() => {
    let cancelled = false;
    setInitial(null);
    if (!sessionId) return;
    indexedDBService.getLiveNotes(sessionId)
      .then((notes) => { if (!cancelled) setInitial({ blocks: parseBlocks(notes?.blocks) }); })
      .catch(() => { if (!cancelled) setInitial({}); });
    return () => { cancelled = true; };
  }, [sessionId]);

  return (
    <aside aria-label="My notes" className="flex h-full w-[380px] flex-shrink-0 flex-col border-l border-line bg-paper">
      <div className="px-6 pt-4">
        <h2 className="text-lg text-ink">My notes</h2>
      </div>
      <NotesHeader status={status} hint="Saved as you type. The summary builds on them." />
      <div className="flex-1 overflow-y-auto px-2 pb-6">
        {sessionId && initial && <NotesEditor key={sessionId} initialBlocks={initial.blocks} onChange={schedule} />}
        {!sessionId && <p className="px-4 text-sm text-ink-muted">Notes open when the recording starts.</p>}
      </div>
      <AskBox getLiveNotes={getLiveNotes} />
    </aside>
  );
}
