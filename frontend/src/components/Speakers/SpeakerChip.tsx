'use client';

import { memo, useState } from 'react';
import { Check, Plus, Sparkles, User, UserPlus, UserX } from 'lucide-react';
import { cn } from '@/lib/utils';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
} from '@/components/ui/command';
import { useMeetingSpeakersContext } from '@/contexts/MeetingSpeakersContext';
import { speakerColorClasses, speakerDisplayName } from '@/lib/speakers';
import { similarPeople } from '@/lib/names';

type Mode = 'name' | 'line';

interface SpeakerChipProps {
  transcriptId: string;
  /** Speaker slot key of this line ("S1"), if it has one. */
  speakerKey?: string;
  /**
   * Show the speaker's name on the line. False for the 2nd, 3rd... line in a run by the
   * same speaker, which gets a small hover-only control instead.
   */
  showName: boolean;
}

/**
 * The speaker label on a transcript line. Click it to name the speaker (renames every
 * line they spoke) or to move just this line to someone else.
 * Renders nothing outside a meeting page, where there is no speaker context.
 */
export const SpeakerChip = memo(function SpeakerChip({ transcriptId, speakerKey, showName }: SpeakerChipProps) {
  const ctx = useMeetingSpeakersContext();
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<Mode>(speakerKey ? 'name' : 'line');

  if (!ctx) return null;

  const speaker = speakerKey ? ctx.speakers.find((s) => s.speaker_key === speakerKey) : undefined;
  const label = speakerKey ? speakerDisplayName(speakerKey, speaker) : 'Assign speaker';
  // An unnamed speaker whose voice matches someone named in an earlier meeting
  const familiar = speakerKey && !speaker?.person_id
    ? ctx.suggestions.find((s) => s.speaker_key === speakerKey && s.is_best)
    : undefined;

  const handleOpenChange = (next: boolean) => {
    setOpen(next);
    // Unlabelled lines can only be assigned; labelled ones open on "name speaker".
    if (next) setMode(speakerKey ? 'name' : 'line');
  };

  const trigger =
    speakerKey && showName ? (
      <button
        type="button"
        className={cn(
          'mb-1 inline-flex min-h-6 max-w-full items-center rounded-control border px-2.5 py-0.5 text-xs font-semibold',
          'transition-colors hover:brightness-95 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent',
          speakerColorClasses(speakerKey),
        )}
        title={familiar ? `Sounds like ${familiar.person_name}. Click to confirm or name this speaker` : "Click to name this speaker or change this line's speaker"}
      >
        <span className="truncate">{label}</span>
        {familiar && <Sparkles className="ml-1 h-3 w-3 flex-shrink-0" aria-label={`Sounds like ${familiar.person_name}`} />}
      </button>
    ) : (
      <button
        type="button"
        className={cn(
          'mt-0.5 flex-shrink-0 rounded p-1 text-ink-muted transition-opacity',
          'hover:bg-surface-hover hover:text-ink-muted focus:outline-none focus-visible:ring-2 focus-visible:ring-accent',
          open ? 'opacity-100' : 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100',
        )}
        title={speakerKey ? `${label} (click to change)` : 'Assign a speaker to this line'}
        aria-label={speakerKey ? `Speaker: ${label}. Change` : 'Assign speaker'}
      >
        <User className="h-3.5 w-3.5" />
      </button>
    );

  return (
    <Popover open={open} onOpenChange={handleOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-0">
        {speakerKey && (
          <div className="flex border-b text-xs font-medium">
            <ModeTab active={mode === 'name'} onClick={() => setMode('name')}>
              Name speaker
            </ModeTab>
            <ModeTab active={mode === 'line'} onClick={() => setMode('line')}>
              This line only
            </ModeTab>
          </div>
        )}
        {mode === 'name' && speakerKey ? (
          <NamePanel speakerKey={speakerKey} lineCount={speaker?.segment_count} onDone={() => setOpen(false)} />
        ) : (
          <LinePanel transcriptId={transcriptId} speakerKey={speakerKey} onDone={() => setOpen(false)} />
        )}
      </PopoverContent>
    </Popover>
  );
});

function ModeTab({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'flex-1 px-3 py-2 transition-colors',
        active ? 'border-b-2 border-accent text-accent-strong' : 'text-ink-muted hover:text-ink',
      )}
    >
      {children}
    </button>
  );
}

/** Pick or type a name for the whole speaker. Typed names are saved and reused next time. */
export function NamePanel({
  speakerKey,
  lineCount,
  onDone,
}: {
  speakerKey: string;
  lineCount?: number;
  onDone: () => void;
}) {
  const ctx = useMeetingSpeakersContext()!;
  const [query, setQuery] = useState('');
  const speaker = ctx.speakers.find((s) => s.speaker_key === speakerKey);
  const typed = query.trim().replace(/\s+/g, ' ');
  const exactMatch = ctx.people.some((p) => p.name.toLowerCase() === typed.toLowerCase());
  // Voices that match someone named in another meeting (only for speakers still unnamed)
  const suggested = speaker?.person_id ? [] : ctx.suggestions.filter((s) => s.speaker_key === speakerKey);
  // Names close to what was typed, so "Professor Smith" finds "Prof Smith" instead of adding a duplicate
  const similar = typed && !exactMatch ? similarPeople(typed, ctx.people) : [];

  const choose = async (target: Parameters<typeof ctx.nameSpeaker>[1]) => {
    if (await ctx.nameSpeaker(speakerKey, target)) onDone();
  };

  return (
    <Command>
      <CommandInput placeholder="Type a name…" value={query} onValueChange={setQuery} />
      <CommandList>
        {/* Once something is typed there is always an "Add" or a match, so only hint when empty */}
        {!typed && <CommandEmpty>No one saved yet. Type a name</CommandEmpty>}
        {suggested.length > 0 && (
          <CommandGroup heading="Sounds like">
            {suggested.map((s) => (
              <CommandItem
                key={s.person_id}
                value={`__suggested__${s.person_name}`}
                onSelect={() => choose({ personId: s.person_id })}
                title={`Voice match from ${s.meetings} earlier ${s.meetings === 1 ? 'meeting' : 'meetings'} (similarity ${Math.round(s.similarity * 100)}%)`}
              >
                <Sparkles className="mr-2 h-4 w-4 text-accent-strong" />
                <span className="flex-1 truncate">{s.person_name}</span>
                <span className={cn('ml-2 text-xs', s.strength === 'likely' ? 'text-success' : 'text-warning')}>
                  {s.strength === 'likely' ? 'Likely' : 'Possible'}
                </span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        {similar.length > 0 && (
          // Force-mounted: these deliberately do not match the typed text, so cmdk would hide them
          <CommandGroup forceMount heading="Did you mean">
            {similar.map((p) => (
              <CommandItem
                forceMount
                key={p.id}
                value={`__similar__${p.name}`}
                onSelect={() => choose({ personId: p.id })}
              >
                <User className="mr-2 h-4 w-4 text-ink-muted" />
                <span className="truncate">{p.name}</span>
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        {typed && !exactMatch && (
          // The group must be force-mounted too, or cmdk hides it when no person matches the text
          <CommandGroup forceMount>
            <CommandItem forceMount value={`__create__${typed}`} onSelect={() => choose({ newName: typed })}>
              <UserPlus className="mr-2 h-4 w-4 text-accent-strong" />
              <span className="truncate">
                Add &ldquo;<strong>{typed}</strong>&rdquo;
              </span>
            </CommandItem>
          </CommandGroup>
        )}
        {ctx.people.length > 0 && (
          <CommandGroup heading={lineCount ? `People (applies to all ${lineCount} lines)` : 'People'}>
            {ctx.people.map((p) => (
              <CommandItem key={p.id} value={p.name} onSelect={() => choose({ personId: p.id })}>
                <Check className={cn('mr-2 h-4 w-4', speaker?.person_id === p.id ? 'opacity-100' : 'opacity-0')} />
                <span className="flex-1 truncate">{p.name}</span>
                {p.meeting_count > 0 && (
                  <span className="ml-2 text-xs text-ink-muted">
                    {p.meeting_count} {p.meeting_count === 1 ? 'meeting' : 'meetings'}
                  </span>
                )}
              </CommandItem>
            ))}
          </CommandGroup>
        )}
        {speaker?.person_id && (
          <>
            <CommandSeparator />
            <CommandGroup>
              <CommandItem value="__clear__" onSelect={() => choose(null)}>
                <UserX className="mr-2 h-4 w-4 text-ink-muted" />
                Clear name ({speakerDisplayName(speakerKey)})
              </CommandItem>
            </CommandGroup>
          </>
        )}
      </CommandList>
    </Command>
  );
}

/** Move just this line to another speaker, a brand-new one, or no one. */
function LinePanel({
  transcriptId,
  speakerKey,
  onDone,
}: {
  transcriptId: string;
  speakerKey?: string;
  onDone: () => void;
}) {
  const ctx = useMeetingSpeakersContext()!;

  const moveTo = async (key: string | null) => {
    if (await ctx.moveLine(transcriptId, key)) onDone();
  };

  const moveToNew = async () => {
    const key = await ctx.addSpeaker();
    if (key) await moveTo(key);
  };

  return (
    <Command>
      <CommandList>
        <CommandGroup heading={speakerKey ? 'Move this line to' : 'Who said this?'}>
          {ctx.speakers.map((s) => (
            <CommandItem key={s.speaker_key} value={s.speaker_key} onSelect={() => moveTo(s.speaker_key)}>
              <Check className={cn('mr-2 h-4 w-4', s.speaker_key === speakerKey ? 'opacity-100' : 'opacity-0')} />
              <span
                className={cn(
                  'mr-2 inline-block h-2.5 w-2.5 rounded-full border',
                  speakerColorClasses(s.speaker_key),
                )}
              />
              <span className="flex-1 truncate">{speakerDisplayName(s.speaker_key, s)}</span>
              <span className="ml-2 text-xs text-ink-muted">{s.segment_count}</span>
            </CommandItem>
          ))}
          <CommandItem value="__new__" onSelect={moveToNew}>
            <Plus className="mr-2 h-4 w-4 text-accent-strong" />
            New speaker
          </CommandItem>
        </CommandGroup>
        {speakerKey && (
          <>
            <CommandSeparator />
            <CommandGroup>
              <CommandItem value="__none__" onSelect={() => moveTo(null)}>
                <UserX className="mr-2 h-4 w-4 text-ink-muted" />
                No speaker
              </CommandItem>
            </CommandGroup>
          </>
        )}
      </CommandList>
    </Command>
  );
}
