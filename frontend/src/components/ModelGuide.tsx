'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ModelGuideData {
  machine: { appleSilicon: boolean; memoryGb: number; cores: number; nvidia: boolean };
  recommendation: { model: string; label: string; reason: string; otherOption: string | null };
}

interface SpeedTest {
  seconds: number;
  hourEstimateSeconds: number;
}

const buttonClass =
  'min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';

/** Seconds as plain words: "40 seconds", "about 6 minutes". */
export function duration(seconds: number): string {
  if (seconds < 90) {
    const whole = Math.max(1, Math.round(seconds));
    return `${whole} second${whole === 1 ? '' : 's'}`;
  }
  const minutes = Math.round(seconds / 60);
  return `about ${minutes} minute${minutes === 1 ? '' : 's'}`;
}

/** "Which model suits this computer" with a speed test (plan PL4). */
export function ModelGuide() {
  const [guide, setGuide] = useState<ModelGuideData | null>(null);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<SpeedTest | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    invoke<ModelGuideData>('api_model_guide').then(setGuide).catch(() => setGuide(null));
  }, []);

  const test = async () => {
    setTesting(true);
    setProblem(null);
    setResult(null);
    try {
      setResult(await invoke<SpeedTest>('api_model_speed_test'));
    } catch (error) {
      setProblem(typeof error === 'string' ? error : 'The speed test did not finish');
    } finally {
      setTesting(false);
    }
  };

  if (!guide) return null;
  const { machine, recommendation } = guide;
  const graphics = machine.appleSilicon ? 'Apple Silicon' : machine.nvidia ? 'an NVIDIA graphics card' : 'no graphics card Quire can use';
  return (
    <section aria-labelledby="model-guide" className="mt-4 space-y-3 rounded-card border border-line bg-surface p-4">
      <h3 id="model-guide" className="font-medium text-ink">Which model suits this computer</h3>
      <p className="text-sm text-ink-muted">
        This computer: {machine.memoryGb} GB of memory, {machine.cores} cores, {graphics}.
      </p>
      <p className="text-sm text-ink">
        Recommended built-in model: <span className="font-medium">{recommendation.label}</span>. {recommendation.reason}
      </p>
      {recommendation.otherOption && <p className="text-sm text-ink">{recommendation.otherOption}</p>}
      <div className="flex flex-wrap items-center gap-3">
        <button className={buttonClass} onClick={test} disabled={testing}>
          {testing ? 'Testing...' : 'Test speed'}
        </button>
        <span className="text-sm text-ink-muted">
          Summarizes a made-up three-minute meeting with the model chosen above. Nothing else is sent anywhere.
        </span>
      </div>
      <div aria-live="polite" className="text-sm">
        {result && (
          <p className="text-ink">
            The chosen model took {duration(result.seconds)} for three minutes of talk. An hour-long meeting would take{' '}
            {duration(result.hourEstimateSeconds)}.
          </p>
        )}
        {problem && <p className="text-danger">{problem}</p>}
      </div>
    </section>
  );
}
