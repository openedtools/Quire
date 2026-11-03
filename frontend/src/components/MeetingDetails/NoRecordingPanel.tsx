'use client';

import { Mic } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { TranscriptOnlySwitch } from '@/components/TranscriptOnlySwitch';

interface NoRecordingPanelProps {
  onRecord: () => void;
  /** Another recording is running, so this meeting cannot start one. */
  recordingElsewhere: boolean;
}

/** Shown in place of the transcript on a meeting with no recording yet (spec F-NM-3). */
export function NoRecordingPanel({ onRecord, recordingElsewhere }: NoRecordingPanelProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 bg-paper px-8 text-center">
      <h2 className="text-lg text-ink">No recording yet</h2>
      <p className="max-w-xs text-sm text-ink-muted">
        Take notes now. Record when the meeting starts and the transcript joins this meeting.
      </p>
      <Button type="button" onClick={onRecord} disabled={recordingElsewhere}>
        <Mic aria-hidden="true" />
        Record
      </Button>
      {recordingElsewhere && <p className="text-xs text-ink-muted">Another recording is in progress.</p>}
      {!recordingElsewhere && <TranscriptOnlySwitch />}
    </div>
  );
}
