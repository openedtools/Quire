'use client';

import { MoreAbout } from './MoreAbout';
import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Cloud, FolderOpen, RefreshCw, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { askConfirm } from '@/lib/confirm';
import { describeReport, VAULT_DISCLOSURE, type VaultReport, type VaultStatus } from '@/lib/vault';
import { Switch } from './ui/switch';

const buttonClass =
  'flex items-center gap-2 min-h-9 px-4 rounded-control border text-sm transition-colors bg-surface border-line-strong text-ink hover:bg-surface-hover disabled:opacity-60';
const primaryClass =
  'flex items-center gap-2 min-h-9 px-4 rounded-control border text-sm transition-colors bg-accent-strong border-accent-strong text-on-accent hover:opacity-90 disabled:opacity-60';

function when(iso: string | null): string {
  if (!iso) return 'not yet';
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? 'not yet' : date.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

/** The vault for the iPhone app (spec F-SYNC-1, F-SYNC-2, F-SYNC-3, F-SYNC-9; plan Phase E). */
export function VaultSettings() {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await invoke<VaultStatus>('api_vault_status'));
    } catch (error) {
      console.error('Could not read the vault status:', error);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  /** Nothing is written until the person has read the disclosure and said yes (F-SYNC-1, F-SYNC-2). */
  const turnOn = async (folder: string) => {
    const yes = await askConfirm({ title: 'Write a vault to this folder?', body: `${folder}\n\n${VAULT_DISCLOSURE}`, confirmLabel: 'Write the vault' });
    if (!yes) return;
    setBusy(true);
    try {
      setStatus(await invoke<VaultStatus>('api_vault_enable', { folder }));
      toast.success('The vault is on', { description: 'Your meetings are being written to it in the background.' });
      setTimeout(refresh, 3000);
    } catch (error) {
      toast.error('Could not use that folder', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const choose = async () => {
    try {
      const picked = await invoke<string | null>('api_vault_pick_folder');
      if (picked) await turnOn(picked);
    } catch (error) {
      toast.error('Could not open the folder picker', { description: String(error) });
    }
  };

  const turnOff = async () => {
    try {
      setStatus(await invoke<VaultStatus>('api_vault_disable'));
      toast.success('The vault is off', { description: 'Files already written stay in the folder.' });
    } catch (error) {
      toast.error('Could not turn the vault off', { description: String(error) });
    }
  };

  const exportNow = async () => {
    setBusy(true);
    try {
      const report = await invoke<VaultReport>('api_vault_export_now');
      toast.success(report.written ? `Wrote ${report.written} file${report.written === 1 ? '' : 's'}` : 'The vault is up to date');
      await refresh();
    } catch (error) {
      toast.error('The export did not finish', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  const removeFiles = async () => {
    const yes = await askConfirm({
      title: "Remove Quire's files from the vault?",
      body: 'This removes the files Quire wrote to the vault folder and turns the vault off. Files changed in the folder since, and anything else in it, stay. Your meetings in Quire are not touched.',
      confirmLabel: 'Remove the files',
    });
    if (!yes) return;
    setBusy(true);
    try {
      const report = await invoke<{ removed: number; kept: number }>('api_vault_remove_files');
      toast.success(`Removed ${report.removed} file${report.removed === 1 ? '' : 's'}`, {
        description: report.kept ? `${report.kept} changed in the folder since were left in place.` : undefined,
      });
      await refresh();
    } catch (error) {
      toast.error('Could not remove the files', { description: String(error) });
    } finally {
      setBusy(false);
    }
  };

  if (!status) return null;
  const report = status.last_report;

  return (
    <section aria-labelledby="vault-title" className="space-y-4 p-6 bg-paper rounded-card border border-line">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h3 id="vault-title" className="text-lg font-semibold text-ink mb-1">Vault for the iPhone app</h3>
          <p className="text-sm text-ink-muted">Writes your meetings to a folder the Quire iPhone app can read.</p>
          <MoreAbout>
            <p>One way for now: changes made in the folder are not brought back into Quire.</p>
          </MoreAbout>
        </div>
        {status.enabled && <Switch aria-label="Write the vault" checked onCheckedChange={(on) => !on && turnOff()} />}
      </div>

      {!status.enabled && (
        <div className="space-y-3">
          {status.folder && (
            <div className="flex flex-wrap items-center gap-2">
              <p className="text-sm text-ink-muted">Off. Files already written stay in {status.folder}.</p>
              <button type="button" className={buttonClass} disabled={busy} onClick={removeFiles}>
                <Trash2 className="w-4 h-4" aria-hidden="true" />
                Remove Quire's files from the vault
              </button>
            </div>
          )}
          <div className="flex flex-wrap gap-2">
            {status.suggested_folder && (
              <button type="button" className={primaryClass} disabled={busy} onClick={() => turnOn(status.suggested_folder!)}>
                <Cloud className="w-4 h-4" aria-hidden="true" />
                Use iCloud Drive
              </button>
            )}
            <button type="button" className={buttonClass} disabled={busy} onClick={choose}>
              <FolderOpen className="w-4 h-4" aria-hidden="true" />
              Choose a folder
            </button>
          </div>
          {status.suggested_folder && <p className="text-xs text-ink-muted">iCloud Drive: {status.suggested_folder}</p>}
        </div>
      )}

      {status.enabled && (
        <div className="space-y-3">
          <p className="text-sm text-ink">
            Folder: <span className="break-all">{status.folder}</span>
          </p>
          <p className="text-sm text-ink-muted">
            Last written {when(status.last_export_at)}. {describeReport(report)}
          </p>
          {report && report.changed_in_vault > 0 && (
            <p role="status" className="text-sm text-ink rounded-control border border-warning bg-warning-tint p-3">
              {report.changed_in_vault} file{report.changed_in_vault === 1 ? ' was' : 's were'} changed in the vault folder since Quire wrote {report.changed_in_vault === 1 ? 'it' : 'them'}, so Quire left {report.changed_in_vault === 1 ? 'it' : 'them'} alone.
            </p>
          )}
          {status.long_path && (
            <p role="status" className="text-sm text-ink rounded-control border border-warning bg-warning-tint p-3">
              This folder's path is long. Windows limits paths to 260 characters, so a shorter folder is safer.
            </p>
          )}
          <p className="text-xs text-ink-muted">Audio is never written to the vault in this version.</p>
          <MoreAbout className="text-xs">
            <p>For iCloud Drive on Windows, right-click the folder and choose Always keep on this device.</p>
          </MoreAbout>
          <div className="flex flex-wrap gap-2">
            <button type="button" className={buttonClass} disabled={busy} onClick={exportNow}>
              <RefreshCw className="w-4 h-4" aria-hidden="true" />
              Write now
            </button>
            <button type="button" className={buttonClass} disabled={busy} onClick={removeFiles}>
              <Trash2 className="w-4 h-4" aria-hidden="true" />
              Remove Quire's files from the vault
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
