import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { formatSize, modelSources, sourcePage, whisperRecommendation, type Manifest } from '../../src/lib/models';

// Spec F-MOD-1, F-MOD-2, AC-MOD-1, with the real models.json
const manifest: Manifest = JSON.parse(readFileSync(path.join(import.meta.dir, '..', '..', 'src-tauri', 'models.json'), 'utf8'));
const codes = manifest.parakeet_languages.codes;

describe('modelSources', () => {
  test('lists Parakeet first, then Whisper, then the summary models', () => {
    const kinds = modelSources(manifest).map((r) => r.kind);
    expect(kinds[0]).toBe('parakeet');
    expect(kinds.indexOf('whisper')).toBeGreaterThan(kinds.lastIndexOf('parakeet'));
    expect(kinds.indexOf('summary')).toBeGreaterThan(kinds.lastIndexOf('whisper'));
  });

  test('sums the files of a model and names its languages and pinned source', () => {
    const v3 = modelSources(manifest).find((r) => r.model === 'parakeet-tdt-0.6b-v3-int8')!;
    expect(v3.bytes).toBe(652_183_999 + 18_202_004 + 139_764 + 93_939);
    expect(v3.languages).toBe('25 European languages');
    expect(v3.page).toBe('https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/tree/8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce');
    expect(v3.revision).toBe('8f23f0c');
    expect(formatSize(v3.bytes)).toBe('671 MB');
    expect(formatSize(3_095_033_483)).toBe('3.1 GB');
  });
});

describe('whisperRecommendation', () => {
  test('a language outside Parakeet list recommends Whisper', () => {
    expect(whisperRecommendation('ja', codes)).toMatch(/Whisper/);
    expect(whisperRecommendation('zh', codes)).toMatch(/Whisper/);
  });
  test('Parakeet languages and automatic detection need nothing', () => {
    for (const lang of ['en', 'de', 'fr', 'uk', 'auto', 'auto-translate', '', null]) expect(whisperRecommendation(lang, codes)).toBeNull();
  });
});

describe('sourcePage', () => {
  test('a GitHub release download links to its release', () => {
    expect(sourcePage('https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/x.onnx').page).toBe(
      'https://github.com/k2-fsa/sherpa-onnx/releases/tag/speaker-recongition-models',
    );
  });
});
