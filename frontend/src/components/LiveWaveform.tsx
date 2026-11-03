'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** A mirrored waveform of the recording's loudness, as on the iPhone (plan PL6). */
export function LiveWaveform({ paused = false, bars = 48, height = 96 }: { paused?: boolean; bars?: number; height?: number }) {
  const [levels, setLevels] = useState<number[]>(() => Array(bars).fill(0));

  useEffect(() => {
    if (paused) return;
    let stopped = false;
    const timer = setInterval(async () => {
      const level = await invoke<number>('api_recording_level').catch(() => 0);
      if (!stopped) setLevels((prev) => [...prev.slice(1), Math.max(0, Math.min(1, level))]);
    }, 100);
    return () => {
      stopped = true;
      clearInterval(timer);
    };
  }, [paused]);

  return (
    <div aria-hidden="true" className="flex items-center justify-center gap-[3px]" style={{ height }}>
      {levels.map((level, i) => (
        <span
          key={i}
          className="w-[5px] rounded-full bg-sage transition-[height] duration-100 motion-reduce:transition-none"
          style={{ height: `${Math.max(3, level * height)}px`, opacity: 0.35 + 0.65 * (i / bars) }}
        />
      ))}
    </div>
  );
}
