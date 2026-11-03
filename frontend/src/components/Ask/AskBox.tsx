'use client';

import { CopyButton } from '@/components/CopyButton';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useRouter } from 'next/navigation';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { ChevronDown, ChevronUp, MessageSquareText, NotebookPen, Square } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { isKeptCitation, linkCitations, noteBlockMarkdown, parseJumpHref, type AskCitation } from '@/lib/ask';
import { appendToNotes, jumpToTime } from '@/lib/meetingEvents';

interface AskReadiness {
  providerLabel: string;
  model: string;
  local: boolean;
  ready: boolean;
  problem: string | null;
}

interface AskAnswer {
  answer: string;
  citations: AskCitation[];
  answeredFrom: string;
}

interface Turn {
  id: string;
  question: string;
  answer: string;
  citations: AskCitation[];
  answeredFrom: string;
  status: 'streaming' | 'done' | 'failed';
  error?: string;
}

interface AskBoxProps {
  /** A saved meeting; leave out during a recording. */
  meetingId?: string;
  /** During a recording, the notes live in the window and are passed with each question. */
  getLiveNotes?: () => Promise<string | undefined>;
}

/**
 * Ask about the meeting (spec F-QA). Collapsed until opened; history lasts until the window
 * closes. Answers are Markdown without raw HTML or images, so a hostile transcript cannot make
 * the answer run script or load anything from the network (F-QA-7).
 */
export function AskBox({ meetingId, getLiveNotes }: AskBoxProps) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [readiness, setReadiness] = useState<AskReadiness | null>(null);
  const [question, setQuestion] = useState('');
  const [turns, setTurns] = useState<Turn[]>([]);
  const runningId = useRef<string | null>(null);
  const historyEnd = useRef<HTMLDivElement>(null);

  // Session-only history: a different meeting starts a fresh conversation. An answer still
  // running is cancelled, so it cannot hold the built-in model or block the next question.
  useEffect(() => {
    setTurns([]);
    return () => {
      if (runningId.current) invoke('api_cancel_ask', { requestId: runningId.current }).catch(() => {});
      runningId.current = null;
    };
  }, [meetingId]);

  useEffect(() => {
    if (!open) return;
    invoke<AskReadiness>('api_ask_readiness').then(setReadiness).catch(() => setReadiness(null));
  }, [open]);

  useEffect(() => {
    const unlisten = listen<{ requestId: string; text: string }>('ask-stream', ({ payload }) => {
      setTurns((all) =>
        all.map((t) => (t.id === payload.requestId && t.status === 'streaming' ? { ...t, answer: t.answer + payload.text } : t)),
      );
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  useEffect(() => historyEnd.current?.scrollIntoView({ block: 'nearest' }), [turns]);

  const ask = useCallback(
    async (e: FormEvent) => {
      e.preventDefault();
      const text = question.trim();
      if (!text || runningId.current) return;
      const id = `ask-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      runningId.current = id;
      setQuestion('');
      setTurns((all) => [...all, { id, question: text, answer: '', citations: [], answeredFrom: '', status: 'streaming' }]);
      try {
        const liveNotes = meetingId ? undefined : await getLiveNotes?.();
        const result = await invoke<AskAnswer>('api_ask_transcript', {
          requestId: id,
          meetingId: meetingId ?? null,
          question: text,
          liveNotes: liveNotes ?? null,
        });
        setTurns((all) => all.map((t) => (t.id === id ? { ...t, ...result, status: 'done' } : t)));
      } catch (error) {
        setTurns((all) => all.map((t) => (t.id === id ? { ...t, status: 'failed', error: String(error) } : t)));
      } finally {
        if (runningId.current === id) runningId.current = null;
      }
    },
    [question, meetingId, getLiveNotes],
  );

  const stop = useCallback(() => {
    if (runningId.current) invoke('api_cancel_ask', { requestId: runningId.current }).catch(() => {});
  }, []);

  const addToNote = useCallback((turn: Turn) => {
    appendToNotes(noteBlockMarkdown(turn.question, turn.answer));
    toast.success('Added to My notes');
  }, []);

  const busy = turns.some((t) => t.status === 'streaming');

  return (
    <section aria-label="Ask about this meeting" className="border-t border-line bg-paper">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        className="flex w-full items-center justify-between px-6 py-3 text-sm text-ink hover:bg-surface-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
      >
        <span className="flex items-center gap-2">
          <MessageSquareText className="h-4 w-4" aria-hidden="true" />
          Ask about this meeting
        </span>
        {open ? <ChevronDown className="h-4 w-4" aria-hidden="true" /> : <ChevronUp className="h-4 w-4" aria-hidden="true" />}
      </button>

      {open && (
        <div className="flex max-h-[45vh] flex-col gap-3 px-6 pb-4">
          {readiness && !readiness.ready && (
            <p className="text-sm text-ink-muted">
              {readiness.problem}.{' '}
              <button type="button" className="min-h-6 text-ink underline underline-offset-2" onClick={() => router.push('/settings')}>
                Open Settings
              </button>
            </p>
          )}
          {readiness?.ready && !readiness.local && (
            <p className="text-xs text-warning" role="note">
              Sends the transcript to {readiness.providerLabel}.
            </p>
          )}

          {turns.length > 0 && (
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto" aria-live="polite">
              {turns.map((turn) => (
                <div key={turn.id} className="space-y-1">
                  <p className="text-sm font-medium text-ink">{turn.question}</p>
                  {turn.status === 'failed' ? (
                    <p className="text-sm text-danger">{turn.error}</p>
                  ) : (
                    <>
                      {turn.answer ? (
                        <div className="prose prose-sm max-w-none text-ink">
                          <ReactMarkdown
                            remarkPlugins={[remarkGfm]}
                            disallowedElements={['img']}
                            components={{
                              a: ({ href, children }) => {
                                // Only a citation the app checked can jump (F-QA-4)
                                const parsed = parseJumpHref(href);
                                const seconds =
                                  parsed !== null && turn.status === 'done' && isKeptCitation(parsed, turn.citations) ? parsed : null;
                                return seconds === null ? (
                                  <span>{children}</span>
                                ) : (
                                  <button
                                    type="button"
                                    className="rounded-sm bg-accent-tint px-1 text-xs text-ink hover:underline"
                                    onClick={() => jumpToTime(seconds)}
                                    aria-label={`Go to ${String(children)}`}
                                  >
                                    {children}
                                  </button>
                                );
                              },
                            }}
                          >
                            {turn.status === 'done' ? linkCitations(turn.answer, turn.citations) : turn.answer}
                          </ReactMarkdown>
                        </div>
                      ) : (
                        <p className="text-sm text-ink-muted">Thinking...</p>
                      )}
                      {turn.status === 'done' && (
                        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-ink-muted">
                          <span>AI generated{turn.answeredFrom ? `. ${turn.answeredFrom}` : ''}</span>
                          <CopyButton what="answer" text={turn.answer} />
                          <button
                            type="button"
                            onClick={() => addToNote(turn)}
                            className="inline-flex min-h-6 items-center gap-1 text-ink hover:underline"
                          >
                            <NotebookPen className="h-3 w-3" aria-hidden="true" />
                            Add to note
                          </button>
                        </div>
                      )}
                    </>
                  )}
                </div>
              ))}
              <div ref={historyEnd} />
            </div>
          )}

          <form onSubmit={ask} className="flex items-center gap-2">
            <label htmlFor="ask-question" className="sr-only">
              Question about this meeting
            </label>
            <input
              id="ask-question"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              placeholder={meetingId ? 'Ask about this meeting' : 'What did I miss?'}
              maxLength={2000}
              disabled={readiness !== null && !readiness.ready}
              className="h-9 min-w-0 flex-1 rounded-md border border-line bg-paper px-3 text-sm text-ink placeholder:text-ink-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            />
            {busy ? (
              <Button type="button" variant="secondary" size="sm" onClick={stop}>
                <Square aria-hidden="true" />
                Stop
              </Button>
            ) : (
              <Button type="submit" size="sm" disabled={!question.trim() || (readiness !== null && !readiness.ready)}>
                Ask
              </Button>
            )}
          </form>
        </div>
      )}
    </section>
  );
}
