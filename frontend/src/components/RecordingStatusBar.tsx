'use client';

import { motion } from 'framer-motion';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useEffect, useState } from 'react';
import { LiveWaveform } from './LiveWaveform';

interface RecordingStatusBarProps {
  isPaused?: boolean;
}

export const RecordingStatusBar: React.FC<RecordingStatusBarProps> = ({ isPaused = false }) => {
  // Get recording duration from backend-synced context (in seconds)
  // Backend polls every 500ms, providing smooth updates
  const { activeDuration, isRecording } = useRecordingState();

  // Display state synced from backend
  const [displaySeconds, setDisplaySeconds] = useState(0);

  // Sync with backend duration when it changes (handles refresh/navigation)
  useEffect(() => {
    if (activeDuration !== null) {
      // Round to nearest second to avoid decimal issues
      setDisplaySeconds(Math.floor(activeDuration));
    }
  }, [activeDuration]);

  const formatDuration = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: -10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -10 }}
      transition={{ duration: 0.2 }}
      className="flex items-center gap-3 px-4 py-2 bg-surface rounded-card mb-2 shadow-sm"
    >
      <div className={`w-2.5 h-2.5 rounded-full ${isPaused ? 'bg-warning' : 'bg-danger animate-pulse'}`} />
      <span className={`text-sm font-medium ${isPaused ? 'text-warning' : 'text-ink'}`}>{isPaused ? 'Paused' : 'Recording'}</span>
      <span className="font-serif text-xl tabular-nums text-ink">{formatDuration(displaySeconds)}</span>
      <div className="ml-auto">
        <LiveWaveform paused={isPaused} bars={28} height={28} />
      </div>
    </motion.div>
  );
};
