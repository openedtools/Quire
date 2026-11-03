"use client";

import { useState, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { ButtonGroup } from '@/components/ui/button-group';
import { Copy, FolderOpen, RefreshCw, SpellCheck } from 'lucide-react';
import { CheckTranscriptDialog } from './CheckTranscriptDialog';
import { RetranscribeDialog } from './RetranscribeDialog';
import { useConfig } from '@/contexts/ConfigContext';
import { SpeakersMenu } from '@/components/Speakers/SpeakersMenu';
import { useNoAudioReason } from '@/hooks/useNoAudioReason';


interface TranscriptButtonGroupProps {
  transcriptCount: number;
  onCopyTranscript: () => void;
  onOpenMeetingFolder: () => Promise<void>;
  meetingId?: string;
  meetingFolderPath?: string | null;
  onRefetchTranscripts?: () => Promise<void>;
}


export function TranscriptButtonGroup({
  transcriptCount,
  onCopyTranscript,
  onOpenMeetingFolder,
  meetingId,
  meetingFolderPath,
  onRefetchTranscripts,
}: TranscriptButtonGroupProps) {
  const { betaFeatures } = useConfig();
  const [showRetranscribeDialog, setShowRetranscribeDialog] = useState(false);
  const [showCheck, setShowCheck] = useState(false);
  // Enhance needs the audio (spec F-TO-3)
  const noAudioReason = useNoAudioReason(meetingFolderPath ? meetingId : undefined);

  const handleRetranscribeComplete = useCallback(async () => {
    // Refetch transcripts to show the updated data
    if (onRefetchTranscripts) {
      await onRefetchTranscripts();
    }
  }, [onRefetchTranscripts]);

  return (
    <div className="flex items-center justify-center w-full gap-2">
      <ButtonGroup>
        <Button
          variant="outline"
          size="sm"
          className="px-2 @[30rem]:px-3"
          onClick={() => {
            onCopyTranscript();
          }}
          disabled={transcriptCount === 0}
          title={transcriptCount === 0 ? 'No transcript available' : 'Copy Transcript'}
        >
          <Copy />
          <span className="hidden @[30rem]:inline">Copy</span>
        </Button>

        <Button
          size="sm"
          variant="outline"
          className="px-2 @[30rem]:px-4"
          onClick={() => {
            onOpenMeetingFolder();
          }}
          title="Open Recording Folder"
        >
          <FolderOpen className="@[30rem]:mr-2" size={18} />
          <span className="hidden @[30rem]:inline">Recording</span>
        </Button>

        {meetingId && (
          <Button
            size="sm"
            variant="outline"
            className="px-2 @[30rem]:px-3"
            onClick={() => setShowCheck(true)}
            disabled={transcriptCount === 0}
            title="Check the transcript for likely mistakes"
          >
            <SpellCheck />
            <span className="hidden @[30rem]:inline">Check</span>
          </Button>
        )}

        {meetingId && (
          <SpeakersMenu
            meetingId={meetingId}
            meetingFolderPath={meetingFolderPath}
            onRefetchTranscripts={onRefetchTranscripts}
          />
        )}

        {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
          <Button
            size="sm"
            variant="outline"
            className="bg-accent-tint text-ink hover:bg-surface-hover border-accent/40 px-2 @[30rem]:px-4"
            onClick={() => {
              setShowRetranscribeDialog(true);
            }}
            disabled={!!noAudioReason}
            title={noAudioReason ?? "Retranscribe to enhance your recorded audio"}
          >
            <RefreshCw className="@[30rem]:mr-2" size={18} />
            <span className="hidden @[30rem]:inline">Enhance</span>
          </Button>
        )}
      </ButtonGroup>

      {meetingId && showCheck && (
        <CheckTranscriptDialog
          open={showCheck}
          onOpenChange={setShowCheck}
          meetingId={meetingId}
          onChanged={async () => { await onRefetchTranscripts?.(); }}
        />
      )}

      {betaFeatures.importAndRetranscribe && meetingId && meetingFolderPath && (
        <RetranscribeDialog
          open={showRetranscribeDialog}
          onOpenChange={setShowRetranscribeDialog}
          meetingId={meetingId}
          meetingFolderPath={meetingFolderPath}
          onComplete={handleRetranscribeComplete}
        />
      )}
    </div>
  );
}
