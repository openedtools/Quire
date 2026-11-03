"use client";
import { meetingKind, THOUGHTS } from '@/lib/thoughts';
import { useState, useEffect, useRef } from 'react';
import { motion } from 'framer-motion';
import { MeetingSummary, SummaryProcessResponse, Transcript } from '@/types';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { TranscriptPanel } from '@/components/MeetingDetails/TranscriptPanel';
import { NotesAndSummaryPane } from '@/components/MeetingDetails/NotesAndSummaryPane';
import { SummaryPanel } from '@/components/MeetingDetails/SummaryPanel';
import { MeetingDetailsSplitView, type MeetingDetailsTab } from '@/components/MeetingDetails/MeetingDetailsSplitView';
import { ModelConfig } from '@/components/ModelSettingsModal';

// Custom hooks
import { useMeetingData } from '@/hooks/meeting-details/useMeetingData';
import { useSummaryGeneration } from '@/hooks/meeting-details/useSummaryGeneration';
import { useTemplates } from '@/hooks/meeting-details/useTemplates';
import { useCopyOperations } from '@/hooks/meeting-details/useCopyOperations';
import { useMeetingOperations } from '@/hooks/meeting-details/useMeetingOperations';
import { useConfig } from '@/contexts/ConfigContext';
import { useMeetingSpeakers } from '@/hooks/useMeetingSpeakers';
import { MeetingSpeakersProvider } from '@/contexts/MeetingSpeakersContext';
import { useRouter, useSearchParams } from 'next/navigation';
import { NoRecordingPanel } from '@/components/MeetingDetails/NoRecordingPanel';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { canStartRecording, claimForRecording } from '@/lib/notesFirst';
import { saveWithRemoteConfirmation } from '@/lib/cloud';

export default function PageContent({
  meeting,
  summaryData,
  initialSummary,
  shouldAutoGenerate = false,
  onAutoGenerateComplete,
  onMeetingUpdated,
  onRefetchTranscripts,
  onPatchTranscripts,
  // Pagination props for efficient transcript loading
  segments,
  hasMore,
  isLoadingMore,
  totalCount,
  loadedCount,
  onLoadMore,
}: {
  meeting: any;
  summaryData: MeetingSummary | null;
  initialSummary: SummaryProcessResponse | null;
  shouldAutoGenerate?: boolean;
  onAutoGenerateComplete?: () => void;
  onMeetingUpdated?: () => Promise<void>;
  onRefetchTranscripts?: () => Promise<void>;
  onPatchTranscripts?: (update: (transcript: Transcript) => Transcript) => void;
  // Pagination props
  segments?: any[];
  hasMore?: boolean;
  isLoadingMore?: boolean;
  totalCount?: number;
  loadedCount?: number;
  onLoadMore?: () => void;
}) {
  console.log('📄 PAGE CONTENT: Initializing with data:', {
    meetingId: meeting.id,
    summaryDataKeys: summaryData ? Object.keys(summaryData) : null,
    transcriptsCount: meeting.transcripts?.length
  });

  // State
  const [customPrompt, setCustomPrompt] = useState<string>('');
  const isRecording = false;
  // A notes-first meeting before its recording (spec F-NM): notes are the main pane
  const hasRecording = (meeting.transcripts?.length ?? 0) > 0 || !!meeting.folder_path;
  const [activeTab, setActiveTab] = useState<MeetingDetailsTab>(hasRecording ? 'transcript' : 'summary');
  const router = useRouter();
  const isNewMeeting = useSearchParams().get('new') === '1';
  // Busy until any earlier recording is fully saved, so its transcript cannot be redirected
  const recordingElsewhere = !canStartRecording(useRecordingState().status);

  // Ref to store the modal open function from SummaryGeneratorButtonGroup
  const openModelSettingsRef = useRef<(() => void) | null>(null);
  const autoSwitchedSummaryMeetingIdsRef = useRef(new Set<string>());
  const manuallySelectedTabMeetingIdsRef = useRef(new Set<string>());
  const autoGenerationStartedMeetingIdRef = useRef<string | null>(null);

  // Get model config from ConfigContext
  const { modelConfig, setModelConfig, isModelConfigLoading } = useConfig();

  // Custom hooks
  const meetingData = useMeetingData({ meeting, summaryData, onMeetingUpdated });
  // A recording of thoughts gets a prompt, not meeting notes (PN4)
  const [isThoughts, setIsThoughts] = useState(false);
  const [kindLoaded, setKindLoaded] = useState(false);
  useEffect(() => {
    let gone = false;
    setKindLoaded(false);
    meetingKind(meeting.id)
      .then((k) => !gone && setIsThoughts(k === THOUGHTS))
      .catch(() => {})
      .finally(() => !gone && setKindLoaded(true));
    return () => { gone = true; };
  }, [meeting.id]);
  const templates = useTemplates(isThoughts);
  const speakers = useMeetingSpeakers(meeting.id, onPatchTranscripts);

  // Callback to register the modal open function
  const handleRegisterModalOpen = (openFn: () => void) => {
    console.log('📝 Registering modal open function in PageContent');
    openModelSettingsRef.current = openFn;
  };

  // Callback to trigger modal open (called from error handler)
  const handleOpenModelSettings = () => {
    console.log('🔔 Opening model settings from PageContent');
    if (openModelSettingsRef.current) {
      openModelSettingsRef.current();
    } else {
      console.warn('⚠️ Modal open function not yet registered');
    }
  };

  // Save model config to backend database and sync via event
  const handleSaveModelConfig = async (config?: ModelConfig) => {
    if (!config) return;
    try {
      // A new address on another computer is confirmed first (spec F-MOD-7)
      const saved = await saveWithRemoteConfirmation((confirmRemote) =>
        invoke('api_save_model_config', {
          provider: config.provider,
          model: config.model,
          whisperModel: config.whisperModel,
          apiKey: config.apiKey ?? null,
          ollamaEndpoint: config.ollamaEndpoint ?? null,
          confirmRemote,
        }),
      );
      if (saved === null) return false;

      // Emit event so ConfigContext and other listeners stay in sync, without the typed key (F-MOD-6)
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', { ...config, apiKey: null, customOpenAIApiKey: null });

      toast.success('Model settings saved successfully');
      return true;
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error('Failed to save model settings', { description: String(error) });
      return false;
    }
  };

  const summaryGeneration = useSummaryGeneration({
    initialSummary,
    meeting,
    transcripts: meetingData.transcripts,
    modelConfig: modelConfig,
    isModelConfigLoading,
    selectedTemplate: templates.selectedTemplate,
    onMeetingUpdated,
    updateMeetingTitle: meetingData.updateMeetingTitle,
    setAiSummary: meetingData.setAiSummary,
    onOpenModelSettings: handleOpenModelSettings,
  });

  const copyOperations = useCopyOperations({
    meeting,
    transcripts: meetingData.transcripts,
    meetingTitle: meetingData.meetingTitle,
    aiSummary: meetingData.aiSummary,
    blockNoteSummaryRef: meetingData.blockNoteSummaryRef,
  });

  const meetingOperations = useMeetingOperations({
    meeting,
  });

  // Record into this meeting: the home screen records, and the save attaches to this id (F-NM-3)
  const handleRecordIntoMeeting = () => {
    claimForRecording({ id: meeting.id, title: meetingData.meetingTitle || meeting.title });
    sessionStorage.setItem('autoStartRecording', 'true');
    router.push('/');
  };

  useEffect(() => {
    if (
      (meetingData.aiSummary || summaryGeneration.summaryStatus === 'completed')
      && !autoSwitchedSummaryMeetingIdsRef.current.has(meeting.id)
      && !manuallySelectedTabMeetingIdsRef.current.has(meeting.id)
    ) {
      autoSwitchedSummaryMeetingIdsRef.current.add(meeting.id);
      setActiveTab('summary');
    }
  }, [meeting.id, meetingData.aiSummary, summaryGeneration.summaryStatus]);

  // Auto-generate only after the model configuration has settled.
  useEffect(() => {
    if (
      !shouldAutoGenerate
      || summaryGeneration.summaryStatus !== 'idle'
      || isModelConfigLoading
      || meetingData.transcripts.length === 0
      || autoGenerationStartedMeetingIdRef.current === meeting.id
      // Thoughts get the prompt template, so wait until the kind is known (PN4)
      || !kindLoaded
      || (isThoughts && templates.selectedTemplate !== 'thought_prompt')
    ) {
      return;
    }

    autoGenerationStartedMeetingIdRef.current = meeting.id;
    console.log(`🤖 Auto-generating summary with ${modelConfig.provider}/${modelConfig.model}...`);
    onAutoGenerateComplete?.();
    void summaryGeneration.handleGenerateSummary('');
  }, [
    shouldAutoGenerate,
    kindLoaded,
    isThoughts,
    templates.selectedTemplate,
    meeting.id,
    meetingData.transcripts.length,
    isModelConfigLoading,
    modelConfig.provider,
    modelConfig.model,
    summaryGeneration.handleGenerateSummary,
    summaryGeneration.summaryStatus,
    onAutoGenerateComplete,
  ]);

  return (
    <MeetingSpeakersProvider value={speakers}>
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: 'easeOut' }}
      className="flex flex-col h-screen min-w-0 bg-surface"
    >
      <div className="flex flex-1 min-w-0 overflow-hidden">
        <MeetingDetailsSplitView
          activeTab={activeTab}
          onTabChange={(tab) => {
            manuallySelectedTabMeetingIdsRef.current.add(meeting.id);
            setActiveTab(tab);
          }}
          transcript={!hasRecording ? (
            <NoRecordingPanel onRecord={handleRecordIntoMeeting} recordingElsewhere={recordingElsewhere} />
          ) : (
            <TranscriptPanel
              transcripts={meetingData.transcripts}
              customPrompt={customPrompt}
              onPromptChange={setCustomPrompt}
              onCopyTranscript={copyOperations.handleCopyTranscript}
              onOpenMeetingFolder={meetingOperations.handleOpenMeetingFolder}
              isRecording={isRecording}
              disableAutoScroll={true}
              usePagination={true}
              segments={segments}
              hasMore={hasMore}
              isLoadingMore={isLoadingMore}
              totalCount={totalCount}
              loadedCount={loadedCount}
              onLoadMore={onLoadMore}
              meetingId={meeting.id}
              meetingFolderPath={meeting.folder_path}
              onRefetchTranscripts={onRefetchTranscripts}
            />
          )}
          summary={
            <NotesAndSummaryPane
              meetingId={meeting.id}
              focusNotes={isNewMeeting && !hasRecording}
              hasSummary={!!meetingData.aiSummary}
              isGenerating={['processing', 'summarizing', 'regenerating'].includes(summaryGeneration.summaryStatus)}
              summary={
                <SummaryPanel
                  meeting={meeting}
                  meetingTitle={meetingData.meetingTitle}
                  summaryRef={meetingData.blockNoteSummaryRef}
                  isSaving={meetingData.isSaving}
                  isSummaryDirty={meetingData.isSummaryDirty}
                  onSaveAll={meetingData.saveAllChanges}
                  onCopySummary={copyOperations.handleCopySummary}
                  aiSummary={meetingData.aiSummary}
                  summaryStatus={summaryGeneration.summaryStatus}
                  transcripts={meetingData.transcripts}
                  modelConfig={modelConfig}
                  setModelConfig={setModelConfig}
                  onSaveModelConfig={handleSaveModelConfig}
                  onGenerateSummary={summaryGeneration.handleGenerateSummary}
                  onStopGeneration={summaryGeneration.handleStopGeneration}
                  customPrompt={customPrompt}
                  onSaveSummary={meetingData.handleSaveSummary}
                  onSummaryChange={meetingData.handleSummaryChange}
                  onDirtyChange={meetingData.setIsSummaryDirty}
                  summaryError={summaryGeneration.summaryError}
                  onRegenerateSummary={summaryGeneration.handleRegenerateSummary}
                  getSummaryStatusMessage={summaryGeneration.getSummaryStatusMessage}
                  availableTemplates={templates.availableTemplates}
                  selectedTemplate={templates.selectedTemplate}
                  onTemplateSelect={templates.handleTemplateSelection}
                  isModelConfigLoading={isModelConfigLoading}
                  onOpenModelSettings={handleRegisterModalOpen}
                />
              }
            />
          }
        />
      </div>
    </motion.div>
    </MeetingSpeakersProvider>
  );
}
