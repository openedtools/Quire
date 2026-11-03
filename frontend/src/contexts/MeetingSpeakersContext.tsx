'use client';

import { createContext, useContext, type ReactNode } from 'react';
import type { MeetingSpeakersApi } from '@/hooks/useMeetingSpeakers';

const MeetingSpeakersContext = createContext<MeetingSpeakersApi | null>(null);

export function MeetingSpeakersProvider({
  value,
  children,
}: {
  value: MeetingSpeakersApi;
  children: ReactNode;
}) {
  return <MeetingSpeakersContext.Provider value={value}>{children}</MeetingSpeakersContext.Provider>;
}

/** Null outside a meeting page (e.g. while recording live), where lines have no speakers yet. */
export function useMeetingSpeakersContext(): MeetingSpeakersApi | null {
  return useContext(MeetingSpeakersContext);
}
