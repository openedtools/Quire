'use client';

import { useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { ArrowLeft, Copy, ExternalLink } from 'lucide-react';
import { toast } from 'sonner';
import { GUIDANCE_URL } from '@/lib/consent';
import { TRANSCRIPT_ONLY_NOTE } from '@/components/TranscriptOnlySwitch';

const SPOKEN = "I'm using a tool to transcribe this meeting so I can take notes. Is that okay with everyone?";
const INVITATION = "I'll be using a local transcription tool during this meeting. Reply if you'd rather I didn't.";

function CopyLine({ label, text }: { label: string; text: string }) {
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      toast.success(`${label} copied`);
    } catch {
      toast.error('Could not copy');
    }
  };
  return (
    <div className="p-3 rounded-control border border-line bg-surface">
      <div className="text-xs font-medium text-ink-muted mb-1">{label}</div>
      <div className="flex items-start justify-between gap-3">
        <p className="text-sm text-ink">{text}</p>
        <button type="button" onClick={copy} aria-label={`Copy the ${label.toLowerCase()}`} className="min-h-9 min-w-9 rounded-control border border-line-strong bg-paper hover:bg-surface-hover inline-flex items-center justify-center">
          <Copy className="w-4 h-4" aria-hidden="true" />
        </button>
      </div>
    </div>
  );
}

/** Plain-language guidance about telling people you record them (spec F-CON-4, design 3.5). */
export default function ConsentGuidancePage() {
  const router = useRouter();
  const openGuide = async () => {
    try {
      await invoke('open_guidance_url', { url: GUIDANCE_URL });
    } catch (error) {
      toast.error('Could not open the guide', { description: String(error) });
    }
  };

  return (
    <div className="h-full overflow-y-auto bg-paper">
      <div className="max-w-2xl mx-auto px-6 py-8 space-y-6">
        <button type="button" onClick={() => router.back()} className="inline-flex items-center gap-2 text-sm text-ink-muted hover:text-ink">
          <ArrowLeft className="w-4 h-4" aria-hidden="true" />
          Back
        </button>
        <h1 className="text-3xl text-ink">Telling people you&apos;re recording</h1>

        <section className="space-y-2">
          <h2 className="text-lg font-semibold text-ink">Why it matters</h2>
          <ul className="list-disc pl-5 space-y-1 text-sm text-ink">
            <li>Laws about recording conversations differ between states and countries.</li>
            <li>Some laws cover learning what is said in a conversation, not only recording it.</li>
            <li>People on a call may be somewhere with different rules than where you are.</li>
          </ul>
        </section>

        <section className="space-y-2">
          <h2 className="text-lg font-semibold text-ink">What to do</h2>
          <ul className="list-disc pl-5 space-y-1 text-sm text-ink">
            <li>Say it out loud at the start.</li>
            <li>Ask for a yes.</li>
            <li>Put it in the invitation.</li>
            <li>Stop if someone objects.</li>
            <li>Follow your workplace rules.</li>
          </ul>
        </section>

        <section className="space-y-2">
          <h2 className="text-lg font-semibold text-ink">Words you can use</h2>
          <CopyLine label="Spoken line" text={SPOKEN} />
          <CopyLine label="Invitation line" text={INVITATION} />
        </section>

        <section className="space-y-2">
          <h2 className="text-lg font-semibold text-ink">Transcript only</h2>
          <p className="text-sm text-ink">Not saving audio keeps less on your computer. It does not change whether you need people&apos;s consent.</p>
          <p className="text-sm text-ink-muted">{TRANSCRIPT_ONLY_NOTE}</p>
        </section>

        <section className="space-y-2">
          <h2 className="text-lg font-semibold text-ink">State by state</h2>
          <p className="text-sm text-ink">
            The Reporters Committee for Freedom of the Press keeps a guide to recording laws in each US state.
          </p>
          <button type="button" onClick={openGuide} className="inline-flex items-center gap-2 min-h-9 px-4 rounded-control border text-sm bg-surface border-line-strong text-ink hover:bg-surface-hover">
            <ExternalLink className="w-4 h-4" aria-hidden="true" />
            Open the Reporter&apos;s Recording Guide
          </button>
        </section>

        <p className="text-sm text-ink-muted">This page is general guidance, not legal advice.</p>
      </div>
    </div>
  );
}
