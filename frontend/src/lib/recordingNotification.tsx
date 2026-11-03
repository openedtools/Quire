import { REMINDER_EVENT, reminderEnabled } from './consent';
import { currentRecordingKind, THOUGHTS } from './thoughts';

/**
 * Asks the consent reminder card to appear (spec F-CON-1). Called after a recording has started,
 * so it never blocks or delays the start; the card itself is `ConsentReminder`.
 */
export async function showRecordingNotification(): Promise<void> {
  try {
    // Thoughts are just you: no reminder (PN4)
    if (currentRecordingKind() !== THOUGHTS && (await reminderEnabled())) window.dispatchEvent(new Event(REMINDER_EVENT));
  } catch (error) {
    console.error('Failed to show the consent reminder:', error);
  }
}
