'use client';

import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { Loader2 } from 'lucide-react';
import { toast } from 'sonner';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '../ui/dialog';
import { announceEdits, applySuggestion, checkTranscript, clock, kindOf, repeatedKinds, replaceEverywhere, type Suggestion } from '@/lib/transcriptEdits';

const buttonClass =
  'min-h-8 px-3 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';
const primaryClass =
  'min-h-8 px-3 rounded-control border text-sm transition-colors bg-accent-strong border-accent-strong text-on-accent hover:bg-accent-strong-hover disabled:opacity-60';

/** "Check transcript" (plan PL5): the model suggests likely mistakes; each one waits for a yes. */
export function CheckTranscriptDialog({ open, onOpenChange, meetingId, onChanged }: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  meetingId: string;
  onChanged: () => Promise<void>;
}) {
  const [suggestions, setSuggestions] = useState<Suggestion[] | null>(null);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** The card whose correction is being edited, by line and words (not by position) */
  const [editing, setEditing] = useState<string | null>(null);
  const keyOf = (s: Suggestion) => `${s.lineId}\u0000${s.find}`;

  useEffect(() => {
    if (!open) return;
    let stop: (() => void) | undefined;
    let gone = false;
    setSuggestions(null);
    setProblem(null);
    setProgress(null);
    listen<{ meetingId: string; done: number; total: number }>('transcript-check-progress', ({ payload }) => {
      if (payload.meetingId === meetingId) setProgress({ done: payload.done, total: payload.total });
    }).then((u) => (gone ? u() : (stop = u))).catch(() => {});
    checkTranscript(meetingId)
      .then((found) => !gone && setSuggestions(found))
      .catch((error) => !gone && setProblem(typeof error === 'string' ? error : 'The check did not finish'));
    return () => {
      gone = true;
      stop?.();
    };
  }, [open, meetingId]);

  const repeated = useMemo(() => repeatedKinds(suggestions ?? []), [suggestions]);

  const drop = (match: (s: Suggestion, i: number) => boolean) => setSuggestions((all) => (all ?? []).filter((s, i) => !match(s, i)));

  const accept = async (s: Suggestion, i: number) => {
    setBusy(true);
    try {
      // Applied to the line as it is now, so an earlier fix to it stays
      const now = await applySuggestion(meetingId, s);
      drop((_, j) => j === i);
      // Other cards for this line show its new text
      setSuggestions((all) => (all ?? []).map((o) => (o.lineId === s.lineId ? { ...o, lineText: now } : o)));
      setEditing(null);
      announceEdits();
      await onChanged();
    } catch (error) {
      toast.error(typeof error === 'string' ? error : 'The change could not be saved');
    } finally {
      setBusy(false);
    }
  };

  const acceptAll = async (s: Suggestion) => {
    setBusy(true);
    try {
      const count = await replaceEverywhere(meetingId, s.find, s.replace);
      drop((o) => kindOf(o) === kindOf(s));
      // Lines changed everywhere: other cards show the new text, or go if their words went
      setSuggestions((all) => (all ?? []).map((o) => ({ ...o, lineText: applyFixAll(o.lineText, s.find, s.replace) })).filter((o) => o.lineText.includes(o.find)));
      toast.success(`Changed ${count} line${count === 1 ? '' : 's'}`);
      announceEdits();
      await onChanged();
    } catch (error) {
      toast.error(typeof error === 'string' ? error : 'The changes could not be saved');
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[640px]">
        <DialogHeader>
          <DialogTitle>Check transcript</DialogTitle>
          <DialogDescription>
            The summary model suggests words that were probably misheard. Nothing changes until you accept it, and every
            change can be undone from the line.
          </DialogDescription>
        </DialogHeader>
        <div aria-live="polite" className="max-h-[60vh] space-y-3 overflow-y-auto py-2">
          {problem && <p className="text-sm text-danger">{problem}</p>}
          {!problem && suggestions === null && (
            <p className="flex items-center gap-2 text-sm text-ink-muted">
              <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
              {progress ? `Checking part ${Math.min(progress.done + 1, progress.total)} of ${progress.total}...` : 'Checking...'}
            </p>
          )}
          {suggestions?.length === 0 && <p className="text-sm text-ink">No likely mistakes found.</p>}
          {suggestions?.map((s, i) => (
            <div key={`${s.lineId}-${s.find}`} className="space-y-2 rounded-card border border-line bg-surface p-3">
              <p className="text-xs text-ink-muted">{clock(s.start)} {s.reason}</p>
              <p className="text-sm text-ink">
                <span className="rounded-sm bg-danger-tint px-0.5 line-through">{s.find}</span>
                {' '}
                <span className="rounded-sm bg-success-tint px-0.5">{s.replace}</span>
              </p>
              <p className="text-xs text-ink-muted">{s.lineText}</p>
              {editing === keyOf(s) && (
                <input
                  aria-label="Correction"
                  value={s.replace}
                  autoFocus
                  onChange={(e) => setSuggestions((all) => (all ?? []).map((o, j) => (j === i ? { ...o, replace: e.target.value } : o)))}
                  onKeyDown={(e) => e.key === 'Enter' && s.replace.trim() && accept(s, i)}
                  className="w-full rounded-control border border-line-strong bg-paper px-2 py-1 text-sm text-ink focus:outline-none focus:ring-2 focus:ring-accent"
                />
              )}
              <div className="flex flex-wrap gap-2">
                <button className={primaryClass} disabled={busy || !s.replace.trim()} onClick={() => accept(s, i)} aria-label={`Accept ${s.replace} for ${s.find}`}>Accept</button>
                {editing !== keyOf(s) && <button className={buttonClass} disabled={busy} onClick={() => setEditing(keyOf(s))} aria-label={`Edit the correction for ${s.find}`}>Edit</button>}
                <button className={buttonClass} disabled={busy} onClick={() => drop((_, j) => j === i)} aria-label={`Skip the suggestion for ${s.find}`}>Skip</button>
                {repeated.has(kindOf(s)) && (
                  <button className={buttonClass} disabled={busy} onClick={() => acceptAll(s)}>
                    Accept all {repeated.get(kindOf(s))}, and every other &quot;{s.find}&quot;
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
        <p className="text-xs text-ink-muted">AI generated suggestions. They use the model chosen in Settings, Models.</p>
      </DialogContent>
    </Dialog>
  );
}

/** Every whole-word `find` in `text` replaced, as the backend's Accept all does. */
function applyFixAll(text: string, find: string, replace: string): string {
  const escaped = find.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return text.replace(new RegExp(`(^|\\b|\\s)${escaped}(\\b|\\s|$)`, 'g'), (_, a, b) => `${a}${replace}${b}`);
}
