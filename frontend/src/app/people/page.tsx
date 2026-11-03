'use client';

import { Suspense } from 'react';
import { PeopleView } from '@/components/People/PeopleView';

// The app is a static export, so the selected person travels as ?id= (like /meeting-details).
export default function PeoplePage() {
  return (
    <Suspense fallback={<div className="flex items-center justify-center h-screen text-ink-muted">Loading…</div>}>
      <PeopleView />
    </Suspense>
  );
}
