'use client';

import { useEffect, useState, type ReactNode } from 'react';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { MeetingNotesPanel } from '@/components/Notes/NotesPanels';
import { AskBox } from '@/components/Ask/AskBox';

type PaneTab = 'notes' | 'summary';

interface NotesAndSummaryPaneProps {
  meetingId: string;
  /** Put the cursor in My notes, for a meeting just created (spec F-NM-1). */
  focusNotes?: boolean;
  hasSummary: boolean;
  isGenerating: boolean;
  summary: ReactNode;
}

/**
 * The meeting's right-hand pane: My notes and Summary (spec 3.10). It opens on the summary when
 * there is one, otherwise on the notes, and moves to the summary when generation starts.
 */
export function NotesAndSummaryPane({ meetingId, focusNotes = false, hasSummary, isGenerating, summary }: NotesAndSummaryPaneProps) {
  const [tab, setTab] = useState<PaneTab>(hasSummary ? 'summary' : 'notes');

  useEffect(() => {
    if (isGenerating) setTab('summary');
  }, [isGenerating]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <Tabs value={tab} onValueChange={(v) => setTab(v as PaneTab)} className="flex min-h-0 flex-1 flex-col">
        <TabsList className="mx-4 mt-3 w-fit rounded-full border border-line bg-surface p-1">
          <TabsTrigger value="notes" className="rounded-full px-4 text-ink-muted data-[state=active]:bg-accent-tint data-[state=active]:text-accent-strong data-[state=active]:shadow-none">My notes</TabsTrigger>
          <TabsTrigger value="summary" className="rounded-full px-4 text-ink-muted data-[state=active]:bg-accent-tint data-[state=active]:text-accent-strong data-[state=active]:shadow-none">Summary</TabsTrigger>
        </TabsList>
        {/* Both stay mounted: the summary keeps its unsaved edits, and notes save on their own */}
        <TabsContent value="notes" forceMount className="mt-0 min-h-0 flex-1 data-[state=inactive]:hidden">
          <MeetingNotesPanel meetingId={meetingId} autoFocus={focusNotes} />
        </TabsContent>
        <TabsContent value="summary" forceMount className="mt-0 flex min-h-0 flex-1 flex-col data-[state=inactive]:hidden">
          {summary}
        </TabsContent>
      </Tabs>
      <AskBox meetingId={meetingId} />
    </div>
  );
}
