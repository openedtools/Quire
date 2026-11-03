'use client';

import React, { createContext, useCallback, useContext, useEffect, useState } from 'react';

/** What the person chose in Settings. 'system' follows the operating system. */
export type ThemePreference = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

const STORAGE_KEY = 'theme-preference';

/**
 * Runs before React hydrates (inline in <head>) so the first paint already has the
 * right theme. Keep it in sync with readPreference and resolve below.
 */
export const THEME_BOOT_SCRIPT = `(function(){try{var p=localStorage.getItem('${STORAGE_KEY}');var d=p==='dark'||((p!=='light')&&window.matchMedia('(prefers-color-scheme: dark)').matches);document.documentElement.classList.toggle('dark',d);document.documentElement.style.colorScheme=d?'dark':'light';}catch(e){}})();`;

export function readPreference(): ThemePreference {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'light' || stored === 'dark' ? stored : 'system';
  } catch {
    return 'system';
  }
}

export function resolve(preference: ThemePreference, systemDark: boolean): ResolvedTheme {
  if (preference === 'system') return systemDark ? 'dark' : 'light';
  return preference;
}

interface ThemeContextValue {
  preference: ThemePreference;
  theme: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

async function setWindowTheme(preference: ThemePreference) {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    // null lets the window frame follow the operating system again
    await getCurrentWindow().setTheme(preference === 'system' ? null : preference);
  } catch {
    // Not running inside Tauri (browser preview or tests)
  }
}

const systemPrefersDark = () =>
  typeof window !== 'undefined' && window.matchMedia('(prefers-color-scheme: dark)').matches;

export function ThemeProvider({ children }: { children: React.ReactNode }) {
  // Read the real values on the first client render, so the effects below never undo the
  // boot script's choice (that caused a light flash and a title bar flicker at startup)
  const [preference, setPreferenceState] = useState<ThemePreference>(() =>
    typeof window === 'undefined' ? 'system' : readPreference());
  const [systemDark, setSystemDark] = useState(systemPrefersDark);

  useEffect(() => {
    const query = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener('change', onChange);
    return () => query.removeEventListener('change', onChange);
  }, []);

  const theme = resolve(preference, systemDark);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.toggle('dark', theme === 'dark');
    root.style.colorScheme = theme;
  }, [theme]);

  useEffect(() => {
    setWindowTheme(preference);
  }, [preference]);

  const setPreference = useCallback((next: ThemePreference) => {
    setPreferenceState(next);
    try {
      if (next === 'system') localStorage.removeItem(STORAGE_KEY);
      else localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // Storage unavailable: the choice lasts for this session only
    }
  }, []);

  return (
    <ThemeContext.Provider value={{ preference, theme, setPreference }}>
      {children}
    </ThemeContext.Provider>
  );
}

const OUTSIDE_PROVIDER: ThemeContextValue = { preference: 'system', theme: 'light', setPreference: () => {} };

/** Outside the provider (isolated component tests) this reports the light theme. */
export function useTheme(): ThemeContextValue {
  return useContext(ThemeContext) ?? OUTSIDE_PROVIDER;
}
