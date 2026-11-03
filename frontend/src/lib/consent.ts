// The consent reminder and the first-run notice (spec F-CON-1 to F-CON-6). Choices are stored on
// this computer in preferences.json; the reminder keeps the key the earlier reminder used, so a
// "never show again" given before carries over.

const REMINDER_KEY = 'show_recording_notification';
const FIRST_RUN_KEY = 'first_run_notice_acknowledged';
export const REMINDER_EVENT = 'quire:consent-reminder';

export const GUIDANCE_URL = 'https://www.rcfp.org/reporters-recording-guide/';
export const FIRST_RUN_NOTICE = 'For personal use. Do not record anything you are not allowed to record or keep on a personal computer.';

async function store() {
  const { Store } = await import('@tauri-apps/plugin-store');
  return Store.load('preferences.json');
}

export async function reminderEnabled(): Promise<boolean> {
  try {
    return (await (await store()).get<boolean>(REMINDER_KEY)) ?? true;
  } catch {
    return true;
  }
}

export async function setReminderEnabled(enabled: boolean): Promise<void> {
  const s = await store();
  await s.set(REMINDER_KEY, enabled);
  await s.save();
}

export async function firstRunAcknowledged(): Promise<boolean> {
  try {
    return (await (await store()).get<boolean>(FIRST_RUN_KEY)) ?? false;
  } catch {
    return false;
  }
}

export async function acknowledgeFirstRun(): Promise<void> {
  const s = await store();
  await s.set(FIRST_RUN_KEY, true);
  await s.save();
}
