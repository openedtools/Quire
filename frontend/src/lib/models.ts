import { invoke } from '@tauri-apps/api/core';

/** One pinned file from models.json (spec F-MOD-8). */
export interface ManifestFile {
  kind: 'parakeet' | 'whisper' | 'summary' | 'speaker';
  model: string;
  file: string;
  url: string;
  bytes: number;
  sha256: string;
}

export interface Manifest {
  parakeet_languages: { model: string; codes: string[] };
  files: ManifestFile[];
}

export interface ModelSource {
  kind: ManifestFile['kind'];
  model: string;
  bytes: number;
  /** The model's page at its source, for the "Source" link */
  page: string;
  revision: string;
  languages: string;
}

export const loadManifest = () => invoke<Manifest>('api_model_manifest');

/** The source's page for a pinned download address (`.../resolve/<revision>/<file>`). */
export function sourcePage(url: string): { page: string; revision: string } {
  const m = url.match(/^(https:\/\/huggingface\.co\/[^/]+\/[^/]+)\/resolve\/([^/]+)\//);
  if (m) return { page: `${m[1]}/tree/${m[2]}`, revision: m[2].slice(0, 7) };
  const release = url.match(/^(https:\/\/github\.com\/[^/]+\/[^/]+\/releases)\/download\/([^/]+)\//);
  if (release) return { page: `${release[1]}/tag/${release[2]}`, revision: release[2] };
  return { page: url, revision: '' };
}

function languagesOf(kind: ManifestFile['kind'], model: string, manifest: Manifest): string {
  if (kind === 'parakeet') {
    return model === manifest.parakeet_languages.model
      ? `${manifest.parakeet_languages.codes.length} European languages`
      : 'English';
  }
  if (kind === 'whisper') return model.endsWith('.en') ? 'English' : 'About 99 languages';
  if (kind === 'summary') return 'Many languages';
  return 'Any language';
}

/** One row per model, Parakeet first, then Whisper, then summary models (spec F-MOD-1, F-MOD-4). */
export function modelSources(manifest: Manifest): ModelSource[] {
  const order: ManifestFile['kind'][] = ['parakeet', 'whisper', 'summary', 'speaker'];
  const rows = new Map<string, ModelSource>();
  for (const f of manifest.files) {
    const key = `${f.kind}/${f.model}`;
    const row = rows.get(key);
    if (row) {
      row.bytes += f.bytes;
      continue;
    }
    rows.set(key, { kind: f.kind, model: f.model, bytes: f.bytes, ...sourcePage(f.url), languages: languagesOf(f.kind, f.model, manifest) });
  }
  return [...rows.values()].sort((a, b) => order.indexOf(a.kind) - order.indexOf(b.kind));
}

/**
 * The Whisper recommendation (spec F-MOD-2): null when Parakeet covers the language, or when it is
 * left to automatic detection.
 */
export function whisperRecommendation(language: string | null | undefined, parakeetCodes: string[]): string | null {
  if (!language || language === 'auto' || language === 'auto-translate') return null;
  if (parakeetCodes.includes(language)) return null;
  return 'Parakeet covers 25 European languages and not this one. Choose a Whisper model for meetings in this language.';
}

export function formatSize(bytes: number): string {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.max(1, Math.round(bytes / 1e6))} MB`;
}
