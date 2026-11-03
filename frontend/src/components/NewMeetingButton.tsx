'use client';

import { NotebookPen } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useSidebar } from '@/components/Sidebar/SidebarProvider';

/** Starts a meeting with no recording, ready for notes (spec F-NM-1). */
export function NewMeetingButton({ className }: { className?: string }) {
  const { startNotesMeeting } = useSidebar();
  return (
    <Button type="button" onClick={() => void startNotesMeeting()} className={className}>
      <NotebookPen aria-hidden="true" />
      New meeting
    </Button>
  );
}
