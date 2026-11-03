'use client';

import { createContext, useCallback, useContext, useEffect, useState } from 'react';
import { Pencil, Undo2 } from 'lucide-react';
import { toast } from 'sonner';
import { EDITED_EVENT, editLine, lineEdits, undoLine } from '@/lib/transcriptEdits';

interface EditState {
  /** Changed lines and their first text */
  edited: Record<string, string>;
  save: (lineId: string, text: string) => Promise<boolean>;
  undo: (lineId: string) => Promise<void>;
  /** Reload after changes made elsewhere (the transcript check) */
  refresh: () => Promise<void>;
}

const EditContext = createContext<EditState | null>(null);

/** Editing for a saved meeting's lines (plan PL5); the live transcript has none. */
export function TranscriptEditProvider({ meetingId, onChanged, children }: { meetingId: string; onChanged?: () => Promise<void>; children: React.ReactNode }) {
  const [edited, setEdited] = useState<Record<string, string>>({});

  const refresh = useCallback(async () => {
    const list = await lineEdits(meetingId).catch(() => []);
    setEdited(Object.fromEntries(list.map((e) => [e.lineId, e.original])));
    await onChanged?.();
  }, [meetingId, onChanged]);

  useEffect(() => {
    const load = () =>
      lineEdits(meetingId)
        .then((list) => setEdited(Object.fromEntries(list.map((e) => [e.lineId, e.original]))))
        .catch(() => setEdited({}));
    load();
    window.addEventListener(EDITED_EVENT, load);
    return () => window.removeEventListener(EDITED_EVENT, load);
  }, [meetingId]);

  const save = useCallback(async (lineId: string, text: string) => {
    try {
      await editLine(meetingId, lineId, text);
      await refresh();
      return true;
    } catch (error) {
      toast.error(typeof error === 'string' ? error : 'The line could not be saved');
      return false;
    }
  }, [meetingId, refresh]);

  const undo = useCallback(async (lineId: string) => {
    try {
      await undoLine(meetingId, lineId);
      await refresh();
    } catch (error) {
      toast.error(typeof error === 'string' ? error : 'The change could not be undone');
    }
  }, [meetingId, refresh]);

  return <EditContext.Provider value={{ edited, save, undo, refresh }}>{children}</EditContext.Provider>;
}

export const useTranscriptEditing = () => useContext(EditContext);

/** A line's text, with Edit and, once changed, Undo (plan PL5). Plain text where nothing can be edited. */
export function LineText({ lineId, text, shown, time }: { lineId: string; text: string; shown: string; time: string }) {
  const editing = useTranscriptEditing();
  const [draft, setDraft] = useState<string | null>(null);

  if (!editing) return <p className="text-base text-ink leading-relaxed">{shown}</p>;
  const original = editing.edited[lineId];

  if (draft !== null) {
    const done = async () => {
      if (draft.trim() === text.trim() || (await editing.save(lineId, draft))) setDraft(null);
    };
    return (
      <div className="space-y-2">
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              done();
            } else if (e.key === 'Escape') {
              setDraft(null);
            }
          }}
          aria-label={`Text of the line at ${time}`}
          autoFocus
          rows={Math.min(6, Math.max(2, Math.ceil(draft.length / 60)))}
          className="w-full rounded-control border border-line-strong bg-paper px-2 py-1 text-base leading-relaxed text-ink focus:outline-none focus:ring-2 focus:ring-accent"
        />
        <div className="flex gap-2 text-sm">
          <button type="button" onClick={done} aria-label={`Save the line at ${time}`} className="min-h-8 rounded-control bg-accent-strong px-3 text-on-accent hover:bg-accent-strong-hover">Save</button>
          <button type="button" onClick={() => setDraft(null)} className="min-h-8 rounded-control border border-line-strong bg-surface px-3 text-ink hover:bg-surface-hover">Cancel</button>
        </div>
      </div>
    );
  }

  return (
    <div>
      <p className="text-base text-ink leading-relaxed">
        {shown}
        <button
          type="button"
          onClick={() => setDraft(text)}
          aria-label={`Edit the line at ${time}`}
          className="ml-2 inline-flex h-6 w-6 items-center justify-center rounded-control align-middle text-ink-muted opacity-0 hover:bg-surface-hover focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring group-hover:opacity-100"
        >
          <Pencil className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </p>
      {original !== undefined && (
        <p className="mt-0.5 flex items-center gap-2 text-xs text-ink-muted">
          <span title={`Before: ${original}`}>Edited<span className="sr-only">. Before: {original}</span></span>
          <button type="button" onClick={() => editing.undo(lineId)} className="inline-flex min-h-6 items-center gap-1 text-ink hover:underline" aria-label={`Undo the change to the line at ${time}`}>
            <Undo2 className="h-3 w-3" aria-hidden="true" />
            Undo
          </button>
        </p>
      )}
    </div>
  );
}
