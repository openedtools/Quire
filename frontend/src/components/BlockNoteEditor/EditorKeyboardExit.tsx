'use client';

import React, { useId, useRef } from 'react';

/**
 * The editor keeps Tab for indenting, so keyboard users need another way out (WCAG 2.1.2):
 * Escape leaves the editor, and the next Tab moves on to the following control.
 */
export function EditorKeyboardExit({ children }: { children: React.ReactNode }) {
  const hintId = useId();
  const wrapper = useRef<HTMLDivElement>(null);
  // Focus parks here, after the editor, so the next Tab moves on instead of back into the editor
  const exit = useRef<HTMLSpanElement>(null);

  const onKeyDownCapture = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== 'Escape') return;
    const active = document.activeElement as HTMLElement | null;
    if (active && active !== exit.current && wrapper.current?.contains(active)) {
      active.blur();
      exit.current?.focus();
    }
  };

  return (
    <div ref={wrapper} className="outline-none" aria-describedby={hintId} onKeyDownCapture={onKeyDownCapture}>
      <p id={hintId} className="sr-only">Press Escape, then Tab, to leave the editor.</p>
      {children}
      <span ref={exit} tabIndex={-1} className="sr-only" />
    </div>
  );
}
