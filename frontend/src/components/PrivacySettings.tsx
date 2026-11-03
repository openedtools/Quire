'use client';

import { useEffect, useState } from 'react';
import Link from 'next/link';
import { toast } from 'sonner';
import { FIRST_RUN_NOTICE, reminderEnabled, setReminderEnabled } from '@/lib/consent';
import { Switch } from './ui/switch';
import { CLOUD_CONFIRMATION, cloudStatus, setCloudAllowed } from '@/lib/cloud';

/** Settings, Privacy (spec F-CON-3, F-CON-6). */
export function PrivacySettings() {
  const [reminder, setReminder] = useState<boolean | null>(null);
  const [cloud, setCloud] = useState<boolean | null>(null);

  useEffect(() => {
    reminderEnabled().then(setReminder);
    cloudStatus().then((s) => setCloud(s.allowed)).catch(() => setCloud(false));
  }, []);

  const toggleCloud = async (on: boolean) => {
    try {
      setCloud(await setCloudAllowed(on));
    } catch (error) {
      toast.error('Could not change the setting', { description: String(error) });
    }
  };

  const toggle = async (on: boolean) => {
    try {
      await setReminderEnabled(on);
      setReminder(on);
    } catch (error) {
      toast.error('Could not save the setting', { description: String(error) });
    }
  };

  return (
    <div className="space-y-6">
      <div className="bg-paper rounded-card border border-line p-6 flex items-start justify-between gap-4">
        <div>
          <h3 className="text-lg font-semibold text-ink mb-1">Allow sending data to cloud services</h3>
          <p className="text-sm text-ink-muted">
            Off: everything stays on this computer, and cloud providers and servers on other computers cannot be used. On:{' '}
            {CLOUD_CONFIRMATION.charAt(0).toLowerCase() + CLOUD_CONFIRMATION.slice(1)}
          </p>
        </div>
        {cloud !== null && <Switch aria-label="Allow sending data to cloud services" checked={cloud} onCheckedChange={toggleCloud} />}
      </div>

      <div className="bg-paper rounded-card border border-line p-6 flex items-start justify-between gap-4">
        <div>
          <h3 className="text-lg font-semibold text-ink mb-1">Recording reminder</h3>
          <p className="text-sm text-ink-muted">
            When a recording starts, show a reminder to tell people you are recording.{' '}
            <Link href="/consent" className="text-accent-strong underline underline-offset-2">Read the guidance</Link>
          </p>
        </div>
        {reminder !== null && <Switch aria-label="Recording reminder" checked={reminder} onCheckedChange={toggle} />}
      </div>

      <div className="bg-paper rounded-card border border-line p-6">
        <h3 className="text-lg font-semibold text-ink mb-1">Use of this app</h3>
        <p className="text-sm text-ink">{FIRST_RUN_NOTICE}</p>
      </div>
    </div>
  );
}
