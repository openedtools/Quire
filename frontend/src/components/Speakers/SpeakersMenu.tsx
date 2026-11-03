'use client';

import React, { useState } from 'react';
import { Merge, ScanSearch, Sparkles, Trash2, Users } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { cn } from '@/lib/utils';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { useMeetingSpeakersContext } from '@/contexts/MeetingSpeakersContext';
import { speakerColorClasses, speakerDisplayName } from '@/lib/speakers';
import { DetectSpeakersDialog } from './DetectSpeakersDialog';
import { NamePanel } from './SpeakerChip';
import { useNoAudioReason } from '@/hooks/useNoAudioReason';

interface SpeakersMenuProps {
  meetingId: string;
  /** Folder holding the meeting's recording; none means there is nothing to analyse. */
  meetingFolderPath?: string | null;
  /** Refresh the transcript lines after detection relabelled them. */
  onRefetchTranscripts?: () => Promise<void>;
}

type Pending = { kind: 'merge'; from: string; into: string } | { kind: 'remove'; key: string };

/** The "Speakers" button in the transcript header: detect, name, merge and remove speakers. */
export function SpeakersMenu({ meetingId, meetingFolderPath, onRefetchTranscripts }: SpeakersMenuProps) {
  const ctx = useMeetingSpeakersContext();
  const [open, setOpen] = useState(false);
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [showDetect, setShowDetect] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const noAudioReason = useNoAudioReason(meetingFolderPath ? meetingId : undefined);

  if (!ctx) return null;
  const { speakers } = ctx;
  const bestSuggestions = ctx.suggestions.filter((s) => s.is_best);

  const nameOf = (key: string) => speakerDisplayName(key, speakers.find((s) => s.speaker_key === key));

  const confirmPending = async () => {
    const action = pending;
    setPending(null);
    if (!action) return;
    if (action.kind === 'merge') await ctx.mergeSpeakers(action.from, action.into);
    else await ctx.removeSpeaker(action.key);
    setExpandedKey(null);
  };

  const openDetect = () => {
    setOpen(false);
    setShowDetect(true);
  };

  return (
    <>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button
            size="sm"
            variant="outline"
            // Wider breakpoint than the other buttons: with four of them the group would overflow
            className="px-2 @[34rem]:px-4"
            title={speakers.length > 0 ? `Speakers (${speakers.length}): detect, name and manage` : 'Detect, name and manage speakers'}
            aria-label="Speakers"
          >
            <Users className="@[34rem]:mr-2" size={18} />
            <span className="hidden @[34rem]:inline">Speakers{speakers.length > 0 ? ` (${speakers.length})` : ''}</span>
          </Button>
        </PopoverTrigger>
        <PopoverContent align="center" className="w-80 p-0">
          <div className="px-4 py-3 border-b">
            <h4 className="text-sm font-semibold text-ink">Speakers in this meeting</h4>
          </div>

          {speakers.length === 0 ? (
            <p className="px-4 py-3 text-sm text-ink-muted">
              No speakers yet. Detect them automatically from the recording, or add them by hand with the person icon
              next to a line.
            </p>
          ) : (
            <ul className="max-h-80 overflow-y-auto divide-y divide-line">
              {speakers.map((s) => {
                const isExpanded = expandedKey === s.speaker_key;
                const others = speakers.filter((o) => o.speaker_key !== s.speaker_key);
                return (
                  <li key={s.speaker_key}>
                    <button
                      onClick={() => setExpandedKey(isExpanded ? null : s.speaker_key)}
                      className="w-full flex items-center gap-2 px-4 py-2.5 text-left hover:bg-surface-hover"
                      aria-expanded={isExpanded}
                    >
                      <span className={cn('inline-block h-2.5 w-2.5 rounded-full border flex-shrink-0', speakerColorClasses(s.speaker_key))} />
                      <span className="flex-1 min-w-0 truncate text-sm font-medium text-ink">{nameOf(s.speaker_key)}</span>
                      <span className="text-xs text-ink-muted flex-shrink-0">
                        {s.segment_count} {s.segment_count === 1 ? 'line' : 'lines'}
                      </span>
                    </button>
                    {!isExpanded && !s.person_id && (() => {
                      const best = bestSuggestions.find((b) => b.speaker_key === s.speaker_key);
                      return best ? (
                        <div className="px-4 pb-2 -mt-1 flex items-center gap-2 text-xs text-ink-muted">
                          <Sparkles className="w-3 h-3 flex-shrink-0 text-accent-strong" />
                          <span className="flex-1 min-w-0 truncate">
                            Sounds like <strong className="text-ink">{best.person_name}</strong>
                            {' '}
                            <span className={best.strength === 'likely' ? 'text-success' : 'text-warning'}>
                              ({best.strength})
                            </span>
                          </span>
                          <button
                            onClick={() => void ctx.acceptSuggestion(best.speaker_key, best.person_id)}
                            className="flex-shrink-0 px-2 py-0.5 text-xs font-medium text-accent-strong bg-accent-tint hover:bg-accent-tint border border-accent/40 rounded"
                            aria-label={`Name ${nameOf(s.speaker_key)} as ${best.person_name}`}
                          >
                            Accept
                          </button>
                        </div>
                      ) : null;
                    })()}
                    {isExpanded && (
                      <div className="border-t border-line bg-surface/60">
                        <NamePanel speakerKey={s.speaker_key} lineCount={s.segment_count} onDone={() => setExpandedKey(null)} />
                        <div className="px-4 py-2 space-y-2 border-t border-line">
                          {others.length > 0 && (
                            <div>
                              <p className="text-xs text-ink-muted mb-1 flex items-center">
                                <Merge className="w-3 h-3 mr-1" />
                                Same person as…
                              </p>
                              <div className="flex flex-wrap gap-1">
                                {others.map((o) => (
                                  <button
                                    key={o.speaker_key}
                                    onClick={() => setPending({ kind: 'merge', from: s.speaker_key, into: o.speaker_key })}
                                    className={cn('text-xs px-2 py-0.5 rounded-control border hover:brightness-95', speakerColorClasses(o.speaker_key))}
                                    aria-label={`Merge into ${nameOf(o.speaker_key)}`}
                                  >
                                    {nameOf(o.speaker_key)}
                                  </button>
                                ))}
                              </div>
                            </div>
                          )}
                          <button
                            onClick={() => setPending({ kind: 'remove', key: s.speaker_key })}
                            className="text-xs text-danger hover:text-danger inline-flex items-center"
                          >
                            <Trash2 className="w-3 h-3 mr-1" />
                            Remove speaker
                          </button>
                        </div>
                      </div>
                    )}
                  </li>
                );
              })}
            </ul>
          )}

          <div className="p-3 border-t space-y-2">
            {bestSuggestions.length > 0 && (
              <button
                onClick={() => void ctx.acceptAllSuggestions()}
                className="w-full inline-flex items-center justify-center px-3 py-2 text-sm font-medium text-on-accent bg-accent-strong hover:bg-accent-strong-hover rounded-md"
              >
                <Sparkles className="w-4 h-4 mr-2" />
                Accept {bestSuggestions.length === 1 ? 'suggested name' : `${bestSuggestions.length} suggested names`}
              </button>
            )}
            <button
              onClick={openDetect}
              className="w-full inline-flex items-center justify-center px-3 py-2 text-sm font-medium text-accent-strong bg-accent-tint hover:bg-accent-tint border border-accent/40 rounded-md"
            >
              <ScanSearch className="w-4 h-4 mr-2" />
              {speakers.length > 0 ? 'Detect speakers again…' : 'Detect speakers…'}
            </button>
          </div>
        </PopoverContent>
      </Popover>

      <DetectSpeakersDialog
        open={showDetect}
        onOpenChange={setShowDetect}
        meetingId={meetingId}
        hasRecording={!!meetingFolderPath}
        noAudioReason={noAudioReason}
        hasSpeakers={speakers.length > 0}
        onDetected={async () => {
          await ctx.reload();
          await onRefetchTranscripts?.();
        }}
      />

      {pending?.kind === 'merge' && (
        <ConfirmDialog
          title="Merge speakers"
          body={`Treat ${nameOf(pending.from)} and ${nameOf(pending.into)} as the same person? All of ${nameOf(pending.from)}'s lines move to ${nameOf(pending.into)}.`}
          confirmLabel="Merge"
          onConfirm={() => void confirmPending()}
          onClose={() => setPending(null)}
        />
      )}
      {pending?.kind === 'remove' && (
        <ConfirmDialog
          title="Remove speaker"
          body={`Remove ${nameOf(pending.key)}? Their lines stay in the transcript but become unlabelled.`}
          confirmLabel="Remove"
          destructive
          onConfirm={() => void confirmPending()}
          onClose={() => setPending(null)}
        />
      )}
    </>
  );
}
