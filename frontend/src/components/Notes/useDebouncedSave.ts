'use client';

import { useCallback, useEffect, useRef, useState } from 'react';

export type SaveStatus = 'idle' | 'pending' | 'saving' | 'saved' | 'error';

/**
 * Saves the latest value `delayMs` after the last change, and saves anything still pending
 * when the component goes away (navigation, closing the pane), so no typing is lost.
 */
export function useDebouncedSave<T>(save: (value: T) => Promise<unknown>, delayMs: number) {
  const [status, setStatus] = useState<SaveStatus>('idle');
  const pending = useRef<{ value: T } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saveRef = useRef(save);
  saveRef.current = save;

  const flush = useCallback(async () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const next = pending.current;
    if (!next) return;
    pending.current = null;
    setStatus('saving');
    try {
      await saveRef.current(next.value);
      setStatus(pending.current ? 'pending' : 'saved');
    } catch (error) {
      console.error('Failed to save notes:', error);
      setStatus('error');
    }
  }, []);

  const schedule = useCallback((value: T) => {
    pending.current = { value };
    setStatus('pending');
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void flush(), delayMs);
  }, [delayMs, flush]);

  useEffect(() => () => void flush(), [flush]);

  return { schedule, flush, status };
}

export function saveStatusLabel(status: SaveStatus): string {
  switch (status) {
    case 'pending':
    case 'saving':
      return 'Saving';
    case 'saved':
      return 'Saved';
    case 'error':
      return 'Not saved. Check the log';
    default:
      return '';
  }
}
