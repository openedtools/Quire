'use client';

import { NamingSettings } from '@/components/NamingSettings';
import { KeepModelReadySwitch } from '@/components/KeepModelReadySwitch';
import { ModelGuide } from '@/components/ModelGuide';
import React, { useState, useEffect, useLayoutEffect, useRef } from 'react';
import { ArrowLeft, Settings2, Mic, Database as DatabaseIcon, FlaskConical, ShieldCheck, Cloud } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { motion } from 'framer-motion';
import { TranscriptSettings } from '@/components/TranscriptSettings';
import { RecordingSettings } from '@/components/RecordingSettings';
import { PreferenceSettings } from '@/components/PreferenceSettings';
import { SummaryModelSettings } from '@/components/SummaryModelSettings';
import { BetaSettings } from '@/components/BetaSettings';
import { PrivacySettings } from '@/components/PrivacySettings';
import { ModelSources } from '@/components/ModelSources';
import { VaultSettings } from '@/components/VaultSettings';
import { LanguageGuidance } from '@/components/LanguageGuidance';
import { useConfig } from '@/contexts/ConfigContext';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';

// Tabs configuration (constant)
const TABS = [
  { value: 'general', label: 'General', icon: Settings2 },
  { value: 'recording', label: 'Recordings', icon: Mic },
  { value: 'models', label: 'Models', icon: DatabaseIcon },
  { value: 'sync', label: 'Sync', icon: Cloud },
  { value: 'privacy', label: 'Privacy', icon: ShieldCheck },
  { value: 'beta', label: 'Beta', icon: FlaskConical }
] as const;

export default function SettingsPage() {
  const router = useRouter();
  const { transcriptModelConfig, setTranscriptModelConfig } = useConfig();

  // Animation state for tabs
  const [activeTab, setActiveTab] = useState('general');
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [underlineStyle, setUnderlineStyle] = useState({ left: 0, width: 0 });

  // Load saved transcript configuration on mount
  useEffect(() => {
    const loadTranscriptConfig = async () => {
      try {
        const config = await invoke('api_get_transcript_config') as any;
        if (config) {
          setTranscriptModelConfig({
            provider: config.provider || 'localWhisper',
            model: config.model || 'large-v3',
            apiKey: null
          });
        }
      } catch (error) {
        console.error('Failed to load transcript config:', error);
      }
    };
    loadTranscriptConfig();
  }, [setTranscriptModelConfig]);

  // Update underline position when active tab changes
  useLayoutEffect(() => {
    const activeIndex = TABS.findIndex(tab => tab.value === activeTab);
    const activeTabElement = tabRefs.current[activeIndex];

    if (activeTabElement) {
      const { offsetLeft, offsetWidth } = activeTabElement;
      setUnderlineStyle({ left: offsetLeft, width: offsetWidth });
    }
  }, [activeTab]);

  return (
    <div className="h-screen bg-surface flex flex-col">
      {/* Fixed Header */}
      <div className="sticky top-0 z-10 bg-surface border-b border-line">
        <div className="max-w-6xl mx-auto px-8 py-6">
          <div className="flex items-center gap-4">
            <button
              onClick={() => router.back()}
              className="flex items-center gap-2 text-ink-muted hover:text-ink transition-colors"
            >
              <ArrowLeft className="w-5 h-5" />
              <span>Back</span>
            </button>
            <h1 className="text-3xl font-bold">Settings</h1>
          </div>
        </div>
      </div>

      {/* Scrollable Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-6xl mx-auto p-8 pt-6">
          {/* Tabs */}
          <Tabs value={activeTab} onValueChange={setActiveTab}>
            <TabsList className="bg-transparent relative rounded-none border-b border-line p-0 h-auto">
              {TABS.map((tab, index) => {
                const Icon = tab.icon;
                return (
                  <TabsTrigger
                    key={tab.value}
                    value={tab.value}
                    ref={el => { tabRefs.current[index] = el }}
                    className="flex items-center gap-2 px-6 py-4 bg-transparent rounded-none border-0 data-[state=active]:bg-transparent data-[state=active]:text-accent-strong data-[state=active]:shadow-none text-ink-muted hover:text-ink relative z-10"
                  >
                    <Icon className="w-4 h-4" />
                    {tab.label}
                  </TabsTrigger>
                );
              })}

              <motion.div
                className="absolute bottom-0 z-20 h-0.5 bg-accent-strong"
                layoutId="underline"
                style={{ left: underlineStyle.left, width: underlineStyle.width }}
                transition={{ type: 'spring', stiffness: 400, damping: 40 }}
              />
            </TabsList>

            <TabsContent value="general">
              <PreferenceSettings />
            </TabsContent>
            <TabsContent value="recording">
              <RecordingSettings />
            </TabsContent>
            {/* One Models page: transcription, then summaries and Ask (spec F-MOD-1, plan PC7) */}
            <TabsContent value="models" className="space-y-8">
              <section aria-labelledby="models-transcription">
                <h2 id="models-transcription" className="text-lg font-semibold text-ink mb-3">Transcription</h2>
                <TranscriptSettings
                  transcriptModelConfig={transcriptModelConfig}
                  setTranscriptModelConfig={setTranscriptModelConfig}
                />
                <KeepModelReadySwitch />
                <LanguageGuidance />
                <ModelSources kinds={['parakeet', 'whisper']} title="Where the transcription models come from" />
              </section>
              <section aria-labelledby="models-summaries">
                <h2 id="models-summaries" className="text-lg font-semibold text-ink mb-3">Summaries and Ask</h2>
                <SummaryModelSettings />
                <ModelGuide />
                <ModelSources kinds={['summary', 'speaker']} title="Where the built-in models come from" />
              </section>
              <section aria-labelledby="models-names">
                <h2 id="models-names" className="text-lg font-semibold text-ink mb-3">Meeting names</h2>
                <NamingSettings />
              </section>
            </TabsContent>
            <TabsContent value="sync">
              <VaultSettings />
            </TabsContent>
            <TabsContent value="privacy">
              <PrivacySettings />
            </TabsContent>
            <TabsContent value="beta" className="mt-6">
              <BetaSettings />
            </TabsContent>
          </Tabs>
        </div>
      </div>
    </div>
  );
};
