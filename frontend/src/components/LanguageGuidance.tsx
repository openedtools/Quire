'use client';

import { useEffect, useState } from 'react';
import { useConfig } from '@/contexts/ConfigContext';
import { loadManifest, whisperRecommendation } from '@/lib/models';
import { LANGUAGES } from './LanguageSelection';

/** Recommends Whisper when the meeting language is outside Parakeet's list (spec F-MOD-2). */
export function LanguageGuidance() {
  const { selectedLanguage, transcriptModelConfig } = useConfig();
  const [codes, setCodes] = useState<string[] | null>(null);
  useEffect(() => {
    loadManifest().then((m) => setCodes(m.parakeet_languages.codes)).catch(() => setCodes(null));
  }, []);
  if (!codes || transcriptModelConfig.provider !== 'parakeet') return null;
  const advice = whisperRecommendation(selectedLanguage, codes);
  if (!advice) return null;
  const name = LANGUAGES.find((l) => l.code === selectedLanguage)?.name ?? selectedLanguage;
  return (
    <div role="status" className="mt-4 rounded-control border border-warning bg-warning-tint p-3 text-sm text-ink">
      <strong>{name}:</strong> {advice}
    </div>
  );
}
