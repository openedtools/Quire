import { invoke } from '@tauri-apps/api/core';
import { askConfirm } from './confirm';

export interface CloudStatus {
  allowed: boolean;
  notice: string | null;
}

/** Providers that always send the transcript to another company (spec F-MOD-4, F-MOD-5). */
export const CLOUD_PROVIDERS = ['claude', 'groq', 'openai', 'openrouter'] as const;

export const isCloudProvider = (provider: string) => (CLOUD_PROVIDERS as readonly string[]).includes(provider);

export const CLOUD_CONFIRMATION =
  'Summaries and Ask questions will send the meeting transcript and your notes to the provider you choose, ' +
  'where their privacy policy applies. Recording and transcription stay on this computer.';

export const cloudStatus = () => invoke<CloudStatus>('api_cloud_status');

/** Turns the switch on after one confirmation, or off at once. Returns the new state. */
export async function setCloudAllowed(allowed: boolean): Promise<boolean> {
  if (allowed) {
    const yes = await askConfirm({
      title: 'Allow sending data to cloud services?',
      body: CLOUD_CONFIRMATION,
      confirmLabel: 'Allow',
    });
    if (!yes) return false;
  }
  const status = await invoke<CloudStatus>('api_set_cloud_allowed', { allowed });
  return status.allowed;
}

const REMOTE = 'needs_remote_confirmation';

/** Runs a save; when the core says the address is on another computer, asks once and retries. */
export async function saveWithRemoteConfirmation<T>(save: (confirmRemote: boolean) => Promise<T>): Promise<T | null> {
  try {
    return await save(false);
  } catch (error) {
    if (!String(error).includes(REMOTE)) throw error;
    const yes = await askConfirm({
      title: 'Use a server on another computer?',
      body: 'This address is not on this computer, so meeting transcripts will cross the network to it. It also needs "Allow sending data to cloud services" in Settings, Privacy.',
      confirmLabel: 'Use this address',
    });
    if (!yes) return null;
    return save(true);
  }
}
