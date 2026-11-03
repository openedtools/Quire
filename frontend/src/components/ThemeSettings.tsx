'use client';

import { Monitor, Moon, Sun } from 'lucide-react';
import { useTheme, type ThemePreference } from '@/contexts/ThemeContext';

const OPTIONS: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
  { value: 'system', label: 'System', icon: Monitor },
  { value: 'light', label: 'Light', icon: Sun },
  { value: 'dark', label: 'Dark', icon: Moon },
];

/** Light, Dark or System appearance (spec F-UI-2). */
export function ThemeSettings() {
  const { preference, setPreference } = useTheme();

  return (
    <div className="bg-paper rounded-card border border-line p-6">
      <h3 className="text-lg font-semibold text-ink mb-1">Appearance</h3>
      <p className="text-sm text-ink-muted mb-4">System follows your computer&apos;s light or dark setting.</p>
      <div role="radiogroup" aria-label="Appearance" className="inline-flex gap-2">
        {OPTIONS.map(({ value, label, icon: Icon }) => {
          const selected = preference === value;
          return (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => setPreference(value)}
              className={`flex items-center gap-2 min-h-9 px-4 rounded-control border text-sm transition-colors ${
                selected
                  ? 'bg-accent-strong border-accent-strong text-on-accent'
                  : 'bg-surface border-line-strong text-ink hover:bg-surface-hover'
              }`}
            >
              <Icon className="w-4 h-4" aria-hidden="true" />
              {label}
            </button>
          );
        })}
      </div>
    </div>
  );
}
