'use client';

import { MoreAbout } from './MoreAbout';
import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileText, FolderOpen, RefreshCw } from 'lucide-react';
import { toast } from 'sonner';
import { BRAND } from '@/lib/brand.generated';
import { Switch } from './ui/switch';

interface ChangedFile {
  meeting_id: string;
  title: string;
  path: string;
  state: 'edited' | 'missing';
}

interface ExportStatus {
  enabled: boolean;
  folder: string | null;
  cloud_provider: string | null;
  last_export_at: string | null;
  exported: number;
  changed: ChangedFile[];
}

interface FolderChoice {
  path: string;
  cloud_provider: string | null;
}

interface ExportReport {
  written: number;
  unchanged: number;
  changed_outside: number;
  removed: number;
  failed: number;
}

const SYNC_NOTE =
  'Files in a synced folder (iCloud Drive, OneDrive, Dropbox, Obsidian Sync) leave this computer through that service, not through this app.';

const buttonClass =
  'flex items-center gap-2 min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';
const primaryClass =
  'flex items-center gap-2 min-h-9 px-4 rounded-control border text-sm transition-colors bg-accent-strong border-accent-strong text-on-accent hover:opacity-90 disabled:opacity-60';

function when(iso: string | null): string {
  if (!iso) return 'not yet';
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? 'not yet' : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

/** Markdown export for Obsidian (spec F-MD-1, F-MD-5, F-MD-6). */
export function MarkdownExportSettings() {
  const [status, setStatus] = useState<ExportStatus | null>(null);
  const [choice, setChoice] = useState<FolderChoice | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmReplace, setConfirmReplace] = useState<string | null>(null);
  const [loadError, setLoadError] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await invoke<ExportStatus>('api_markdown_export_status'));
      setLoadError(false);
    } catch (error) {
      console.error('Could not read the Markdown export status:', error);
      setLoadError(true);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const pickFolder = async () => {
    try {
      const picked = await invoke<FolderChoice | null>('api_markdown_export_pick_folder');
      if (picked) setChoice(picked);
    } catch (error) {
      toast.error('Could not open the folder picker', { description: String(error) });
    }
  };

  const confirmFolder = async () => {
    if (!choice) return;
    setBusy(true);
    try {
      setStatus(await invoke<ExportStatus>('api_markdown_export_enable', { folder: choice.path }));
      setChoice(null);
      toast.success('Markdown export is on', { description: 'Meetings are being written to the folder now.' });
      setTimeout(refresh, 2000);
    } catch (error) {
      toast.error('Could not use that folder', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const toggle = async (on: boolean) => {
    if (on) {
      if (status?.folder) {
        setChoice({ path: status.folder, cloud_provider: status.cloud_provider });
      } else {
        await pickFolder();
      }
      return;
    }
    try {
      setStatus(await invoke<ExportStatus>('api_markdown_export_disable'));
    } catch (error) {
      toast.error('Could not turn the export off', { description: String(error) });
    }
  };

  const exportAll = async () => {
    setBusy(true);
    try {
      const report = await invoke<ExportReport>('api_markdown_export_all');
      const parts = [`${report.written} written`, `${report.unchanged} unchanged`];
      if (report.changed_outside) parts.push(`${report.changed_outside} changed in another app`);
      if (report.failed) parts.push(`${report.failed} failed`);
      (report.failed ? toast.warning : toast.success)('Export finished', { description: parts.join(', ') });
      await refresh();
    } catch (error) {
      toast.error('Export did not run', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const replace = async (meetingId: string) => {
    setBusy(true);
    try {
      setStatus(await invoke<ExportStatus>('api_markdown_export_replace', { meetingId }));
      setConfirmReplace(null);
      toast.success(`${BRAND.name}'s version is back in place`);
    } catch (error) {
      toast.error('Could not replace that file', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const enabled = status?.enabled ?? false;

  return (
    <section aria-labelledby="markdown-export-title" className="bg-paper rounded-card border border-line p-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 id="markdown-export-title" className="text-lg font-semibold text-ink mb-1">Markdown export</h3>
          <p className="text-sm text-ink-muted">Save each meeting as a Markdown file, for Obsidian or another notes app.</p>
          <MoreAbout>
            <p>Files update as meetings change. {BRAND.name} never reads them back, and never overwrites a file you edited elsewhere.</p>
          </MoreAbout>
        </div>
        <Switch aria-label="Markdown export" checked={enabled} disabled={busy || !status} onCheckedChange={toggle} />
      </div>

      {loadError && (
        <p role="alert" className="mt-4 text-sm text-danger">
          The export settings could not be read. Restart {BRAND.name} and try again.
        </p>
      )}

      {choice && (
        <div role="group" aria-label="Confirm export folder" className="mt-4 p-4 rounded-card border border-line-strong bg-surface">
          <div className="text-sm font-medium text-ink mb-1">Export meetings to this folder?</div>
          <div className="text-xs font-mono text-ink-muted break-all mb-3">{choice.path}</div>
          {choice.cloud_provider && (
            <p className="text-sm text-warning mb-2">This folder is in {choice.cloud_provider}.</p>
          )}
          <p className="text-sm text-ink mb-4">{SYNC_NOTE} Audio is never exported.</p>
          <div className="flex flex-wrap gap-2">
            <button type="button" className={primaryClass} disabled={busy} onClick={confirmFolder}>
              Export here
            </button>
            <button type="button" className={buttonClass} disabled={busy} onClick={() => setChoice(null)}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {!choice && !enabled && (
        <div className="mt-4">
          <button type="button" className={buttonClass} disabled={busy || !status} onClick={pickFolder}>
            <FolderOpen className="w-4 h-4" aria-hidden="true" />
            Choose folder
          </button>
        </div>
      )}

      {!choice && enabled && status && (
        <div className="mt-4 space-y-4">
          <div className="p-4 rounded-card border border-line bg-surface">
            <div className="text-sm font-medium text-ink mb-1">Folder</div>
            <div className="text-xs font-mono text-ink-muted break-all mb-2">{status.folder}</div>
            <div className="text-sm text-ink-muted">
              {status.exported} {status.exported === 1 ? 'meeting' : 'meetings'} exported. Last change written {when(status.last_export_at)}.
            </div>
            <div className="flex flex-wrap gap-2 mt-3">
              <button type="button" className={buttonClass} disabled={busy} onClick={exportAll}>
                <RefreshCw className="w-4 h-4" aria-hidden="true" />
                Export all now
              </button>
              <button type="button" className={buttonClass} disabled={busy} onClick={pickFolder}>
                <FolderOpen className="w-4 h-4" aria-hidden="true" />
                Change folder
              </button>
            </div>
          </div>

          {status.changed.length > 0 && (
            <div>
              <h4 className="text-sm font-medium text-ink mb-1">Changed in another app</h4>
              <p className="text-sm text-ink-muted mb-2">
                {BRAND.name} leaves these files alone. Replacing one puts {BRAND.name}&apos;s version back and discards the other changes.
              </p>
              <ul className="space-y-2">
                {status.changed.map((file) => (
                  <li key={file.meeting_id} className="flex flex-wrap items-center justify-between gap-2 p-3 rounded-control border border-line">
                    <div className="min-w-0">
                      <div className="flex items-center gap-2 text-sm text-ink">
                        <FileText className="w-4 h-4 shrink-0" aria-hidden="true" />
                        <span className="truncate">{file.title}</span>
                      </div>
                      <div className="text-xs text-ink-muted">
                        {file.state === 'edited' ? 'Edited in another app' : 'Moved or removed in another app'}
                      </div>
                    </div>
                    {confirmReplace === file.meeting_id ? (
                      <div className="flex gap-2" role="group" aria-label={`Replace ${file.title}?`}>
                        <span className="sr-only" aria-live="polite">Replacing discards the changes made in the other app.</span>
                        <button type="button" autoFocus className={primaryClass} disabled={busy} onClick={() => replace(file.meeting_id)}>
                          Replace
                        </button>
                        <button type="button" className={buttonClass} disabled={busy} onClick={() => setConfirmReplace(null)}>
                          Keep theirs
                        </button>
                      </div>
                    ) : (
                      <button type="button" className={buttonClass} disabled={busy} onClick={() => setConfirmReplace(file.meeting_id)}>
                        Replace with {BRAND.name}&apos;s version
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
