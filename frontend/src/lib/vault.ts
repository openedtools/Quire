/** The vault disclosure (spec F-SYNC-2): shown before anything is written. */
export const VAULT_DISCLOSURE =
  'Quire will write your meetings, notes, people and folders into this folder as plain files, so the Quire iPhone app can read them. ' +
  'If the folder is in iCloud Drive, Apple\'s iCloud software uploads the files, not this app. ' +
  'Without Advanced Data Protection turned on in your Apple account, Apple holds the keys to those files. ' +
  'Audio, voiceprints, API keys and settings are never written there.';

export interface VaultReport {
  written: number;
  unchanged: number;
  changed_in_vault: number;
  moved: number;
  deleted: number;
  failed: number;
  notes: number;
  people: number;
  folders: number;
  verified: boolean;
}

export interface VaultStatus {
  enabled: boolean;
  folder: string | null;
  last_export_at: string | null;
  last_report: VaultReport | null;
  suggested_folder: string | null;
  long_path: boolean;
}

/** One line about the last run, counts only. */
export function describeReport(report: VaultReport | null): string {
  if (!report) return 'Not exported yet.';
  const parts = [`${report.notes} meeting${report.notes === 1 ? '' : 's'}`, `${report.people} ${report.people === 1 ? 'person' : 'people'}`, `${report.folders} folder${report.folders === 1 ? '' : 's'}`];
  let line = `In the vault: ${parts.join(', ')}.`;
  if (!report.verified) line += ' The last export could not be checked and will be tried again.';
  return line;
}
