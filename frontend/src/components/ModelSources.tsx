'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ExternalLink } from 'lucide-react';
import { formatSize, loadManifest, modelSources, type ModelSource } from '@/lib/models';

const KIND_LABEL: Record<ModelSource['kind'], string> = {
  parakeet: 'Parakeet',
  whisper: 'Whisper',
  summary: 'Built-in summary model',
  speaker: 'Speaker detection',
};

/** Where each model comes from: size, languages and a source link, pinned by SHA-256 (F-MOD-1, F-MOD-8). */
export function ModelSources({ kinds, title }: { kinds: ModelSource['kind'][]; title: string }) {
  const [rows, setRows] = useState<ModelSource[] | null>(null);
  useEffect(() => {
    loadManifest()
      .then((m) => setRows(modelSources(m).filter((r) => kinds.includes(r.kind))))
      .catch(() => setRows([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  if (!rows || rows.length === 0) return null;
  return (
    <details className="mt-4 rounded-card border border-line bg-paper">
      <summary className="cursor-pointer px-4 py-3 text-sm font-medium text-ink">{title}</summary>
      <div className="px-4 pb-4">
        <p className="text-xs text-ink-muted mb-2">Every file is downloaded from a fixed version of its source and checked against a SHA-256 fingerprint before it is used.</p>
        <table className="w-full text-sm">
          <thead>
            <tr className="text-left text-ink-muted">
              <th className="py-1 pr-2 font-medium">Model</th>
              <th className="py-1 pr-2 font-medium">Size</th>
              <th className="py-1 pr-2 font-medium">Languages</th>
              <th className="py-1 font-medium">Source</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={`${r.kind}/${r.model}`} className="border-t border-line">
                <td className="py-1 pr-2 text-ink">
                  <span className="text-ink-muted">{KIND_LABEL[r.kind]}</span> {r.model}
                </td>
                <td className="py-1 pr-2 text-ink">{formatSize(r.bytes)}</td>
                <td className="py-1 pr-2 text-ink">{r.languages}</td>
                <td className="py-1">
                  <button
                    type="button"
                    onClick={() => invoke('open_external_url', { url: r.page }).catch(() => {})}
                    className="inline-flex items-center gap-1 text-accent-strong underline underline-offset-2"
                    aria-label={`Open the source of ${r.model}`}
                  >
                    {r.revision || 'Source'}
                    <ExternalLink className="w-3 h-3" aria-hidden="true" />
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}
