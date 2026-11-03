'use client';

import React, { createContext, useContext, useState, useEffect } from 'react';
import { usePathname, useRouter } from 'next/navigation';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { toast } from 'sonner';
import { canStartRecording, clearRecordTarget, createNotesMeeting } from '@/lib/notesFirst';
import type { Folder, FolderSummary, SummaryProcessResponse } from '@/types';



export interface SidebarItem {
  id: string;
  title: string;
  type: 'folder' | 'file';
  children?: SidebarItem[];
  /** A folder the user created, as opposed to the built-in "Meeting Notes" root */
  userFolder?: boolean;
}

export interface CurrentMeeting {
  id: string;
  title: string;
  /** Folder the meeting is filed in; absent or null when unfiled */
  folder_id?: string | null;
}

// Search result type for transcript search
interface TranscriptSearchResult {
  id: string;
  title: string;
  matchContext: string;
  timestamp: string;
}

interface SummaryPoll {
  processId: string;
  timer: NodeJS.Timeout;
  inFlight: boolean;
}

interface SidebarContextType {
  currentMeeting: CurrentMeeting | null;
  setCurrentMeeting: (meeting: CurrentMeeting | null) => void;
  sidebarItems: SidebarItem[];
  isCollapsed: boolean;
  toggleCollapse: () => void;
  meetings: CurrentMeeting[];
  setMeetings: (meetings: CurrentMeeting[]) => void;
  isMeetingActive: boolean;
  setIsMeetingActive: (active: boolean) => void;
  handleRecordingToggle: () => void;
  /** Creates a meeting with no recording and opens it (spec F-NM-1). */
  startNotesMeeting: () => Promise<void>;
  searchTranscripts: (query: string) => Promise<void>;
  searchResults: TranscriptSearchResult[];
  isSearching: boolean;
  // Summary polling management
  startSummaryPolling: (
    meetingId: string,
    processId: string,
    onUpdate: (result: SummaryProcessResponse) => void | Promise<void>
  ) => void;
  stopSummaryPolling: (meetingId: string, processId?: string) => void;
  // Refetch meetings from backend
  refetchMeetings: () => Promise<void>;
  // Folders for organizing meetings
  folders: FolderSummary[];
  createFolder: (name: string) => Promise<Folder | null>;
  renameFolder: (folderId: string, name: string) => Promise<boolean>;
  deleteFolder: (folderId: string) => Promise<boolean>;
  /** Pass null to take a meeting out of its folder. */
  moveMeetingToFolder: (meetingId: string, folderId: string | null) => Promise<boolean>;

}

const SidebarContext = createContext<SidebarContextType | null>(null);

export const useSidebar = () => {
  const context = useContext(SidebarContext);
  if (!context) {
    throw new Error('useSidebar must be used within a SidebarProvider');
  }
  return context;
};

export function SidebarProvider({ children }: { children: React.ReactNode }) {
  const [currentMeeting, setCurrentMeeting] = useState<CurrentMeeting | null>({ id: 'intro-call', title: '+ New Call' });
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [meetings, setMeetings] = useState<CurrentMeeting[]>([]);
  const [folders, setFolders] = useState<FolderSummary[]>([]);
  const [sidebarItems, setSidebarItems] = useState<SidebarItem[]>([]);
  const [isMeetingActive, setIsMeetingActive] = useState(false);
  const [searchResults, setSearchResults] = useState<any[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const summaryPollsRef = React.useRef(new Map<string, SummaryPoll>());

  // Use recording state from RecordingStateContext (single source of truth)
  const { isRecording, status: recordingStatus } = useRecordingState();

  const pathname = usePathname();
  const router = useRouter();

  // Extract fetchMeetings as a reusable function
  const fetchMeetings = React.useCallback(async () => {
    try {
      const meetings = await invoke('api_get_meetings') as Array<{ id: string, title: string, folder_id?: string | null, deleting?: boolean }>;
      const transformedMeetings = meetings.map((meeting) => ({
        id: meeting.id,
        // A meeting waiting for a recording file that is in use (spec F-RET-12)
        title: meeting.deleting ? `${meeting.title} (deleting)` : meeting.title,
        folder_id: meeting.folder_id ?? null
      }));
      setMeetings(transformedMeetings);
      // Folders are optional: a failure here must not hide the meetings themselves
      try {
        const loadedFolders = await invoke('api_list_folders') as FolderSummary[] | null;
        setFolders(Array.isArray(loadedFolders) ? loadedFolders : []);
      } catch (folderError) {
        console.error('Error fetching folders:', folderError);
      }
    } catch (error) {
      console.error('Error fetching meetings:', error);
      setMeetings([]);
    }
  }, []);

  useEffect(() => {
    fetchMeetings();
  }, [fetchMeetings]);

  // A meeting named automatically when its recording was finalized (plan PL1)
  useEffect(() => {
    let stop: (() => void) | undefined;
    let gone = false;
    listen<{ meetingId: string; title: string }>('meeting-renamed', ({ payload }) => {
      setMeetings(prev => prev.map(m => m.id === payload.meetingId ? { ...m, title: payload.title } : m));
      setCurrentMeeting(prev => prev?.id === payload.meetingId ? { ...prev, title: payload.title } : prev);
    }).then(unlisten => {
      if (gone) unlisten();
      else stop = unlisten;
    }).catch(() => {
      // No event bridge (tests, or the window closing): nothing to follow
    });
    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  const sortFolders = (list: FolderSummary[]) =>
    [...list].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));

  const folderErrorToast = (title: string, error: unknown) =>
    toast.error(title, { description: error instanceof Error ? error.message : String(error) });

  const createFolder = async (name: string): Promise<Folder | null> => {
    try {
      const folder = await invoke('api_create_folder', { name }) as Folder;
      setFolders(prev => sortFolders([...prev, { id: folder.id, name: folder.name, meeting_count: 0 }]));
      return folder;
    } catch (error) {
      folderErrorToast('Could not create folder', error);
      return null;
    }
  };

  const renameFolder = async (folderId: string, name: string): Promise<boolean> => {
    try {
      const folder = await invoke('api_rename_folder', { folderId, name }) as Folder;
      setFolders(prev => sortFolders(prev.map(f => f.id === folderId ? { ...f, name: folder.name } : f)));
      return true;
    } catch (error) {
      folderErrorToast('Could not rename folder', error);
      return false;
    }
  };

  // Meetings in a deleted folder are kept; they simply become unfiled.
  const deleteFolder = async (folderId: string): Promise<boolean> => {
    try {
      await invoke('api_delete_folder', { folderId });
      setFolders(prev => prev.filter(f => f.id !== folderId));
      setMeetings(prev => prev.map(m => m.folder_id === folderId ? { ...m, folder_id: null } : m));
      return true;
    } catch (error) {
      folderErrorToast('Could not delete folder', error);
      return false;
    }
  };

  const moveMeetingToFolder = async (meetingId: string, folderId: string | null): Promise<boolean> => {
    try {
      await invoke('api_move_meeting_to_folder', { meetingId, folderId });
      setMeetings(prev => prev.map(m => m.id === meetingId ? { ...m, folder_id: folderId } : m));
      return true;
    } catch (error) {
      folderErrorToast('Could not move meeting', error);
      return false;
    }
  };

  // Meeting Notes root: user folders first, then meetings that are not in any folder.
  // With no folders this is exactly the flat list the sidebar always showed.
  const buildBaseItems = (): SidebarItem[] => {
    const toFile = (m: CurrentMeeting) => ({ id: m.id, title: m.title, type: 'file' as const });
    const folderIds = new Set(folders.map(f => f.id));
    const folderItems: SidebarItem[] = folders.map(f => ({
      id: f.id,
      title: f.name,
      type: 'folder' as const,
      userFolder: true,
      children: meetings.filter(m => m.folder_id === f.id).map(toFile),
    }));
    const unfiled = meetings.filter(m => !m.folder_id || !folderIds.has(m.folder_id)).map(toFile);
    return [
      {
        id: 'meetings',
        title: 'Meeting Notes',
        type: 'folder' as const,
        children: [...folderItems, ...unfiled],
      },
    ];
  };
  const baseItems = buildBaseItems();


  const toggleCollapse = () => {
    setIsCollapsed(!isCollapsed);
  };

  // Update current meeting when on home page
  useEffect(() => {
    if (pathname === '/') {
      setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
    }
    setSidebarItems(baseItems);
  }, [pathname]);

  // Update sidebar items when meetings or folders change
  useEffect(() => {
    setSidebarItems(baseItems);
  }, [meetings, folders]);

  // Function to handle recording toggle from sidebar
  const handleRecordingToggle = () => {
    if (!isRecording && canStartRecording(recordingStatus)) {
      // The Record shortcut always makes a new meeting (spec F-NM-6)
      clearRecordTarget();
      // Check if already on home page
      if (pathname === '/') {
        // Already on home - trigger recording directly via custom event
        console.log('Triggering recording from sidebar (already on home page)');
        window.dispatchEvent(new CustomEvent('start-recording-from-sidebar'));
      } else {
        // Not on home - navigate and use auto-start mechanism
        console.log('Navigating to home page with auto-start flag');
        sessionStorage.setItem('autoStartRecording', 'true');
        router.push('/');
      }
    }
    // The actual recording start/stop is handled in the Home component
  };

  const startNotesMeeting = async () => {
    try {
      const meeting = await createNotesMeeting();
      await fetchMeetings();
      router.push(`/meeting-details?id=${meeting.id}&new=1`);
    } catch (error) {
      console.error('Failed to create a meeting:', error);
      toast.error('Could not start a new meeting', { description: String(error) });
    }
  };

  // Function to search through meeting transcripts
  const searchTranscripts = async (query: string) => {
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }

    try {
      setIsSearching(true);


      const results = await invoke('api_search_transcripts', { query }) as TranscriptSearchResult[];

      // Also surface meetings where someone whose name matches spoke. Optional: a failure
      // here must not hide the normal transcript results.
      let withPerson: TranscriptSearchResult[] = [];
      try {
        const hits = await invoke('api_search_meetings_by_person', { query }) as Array<{
          meeting_id: string; title: string; person_name: string;
        }>;
        const alreadyListed = new Set(results.map(r => r.id));
        withPerson = (hits ?? [])
          .filter(h => !alreadyListed.has(h.meeting_id))
          .map(h => ({ id: h.meeting_id, title: h.title, matchContext: `With ${h.person_name}`, timestamp: '' }));
      } catch (personError) {
        console.error('Error searching by person:', personError);
      }

      setSearchResults([...results, ...withPerson]);
    } catch (error) {
      console.error('Error searching transcripts:', error);
      setSearchResults([]);
    } finally {
      setIsSearching(false);
    }
  };

  // Summary polling management
  const stopSummaryPolling = React.useCallback((meetingId: string, processId?: string) => {
    const poll = summaryPollsRef.current.get(meetingId);
    if (!poll || (processId && poll.processId !== processId)) {
      return;
    }
    clearInterval(poll.timer);
    summaryPollsRef.current.delete(meetingId);
  }, []);

  const startSummaryPolling = React.useCallback((
    meetingId: string,
    processId: string,
    onUpdate: (result: SummaryProcessResponse) => void | Promise<void>
  ) => {
    stopSummaryPolling(meetingId);
    let pollCount = 0;
    const maxPolls = 200;
    const poll = async () => {
      const entry = summaryPollsRef.current.get(meetingId);
      if (!entry || entry.processId !== processId || entry.inFlight) {
        return;
      }
      entry.inFlight = true;
      try {
        pollCount += 1;
        if (pollCount >= maxPolls) {
          await onUpdate({
            status: 'error',
            meetingName: null,
            meeting_id: meetingId,
            start: processId,
            end: null,
            data: null,
            error: 'Summary generation timed out after 15 minutes. Please try again or check your model configuration.',
          });
          if (summaryPollsRef.current.get(meetingId) === entry) {
            stopSummaryPolling(meetingId, processId);
          }
          return;
        }

        const result = await invoke<SummaryProcessResponse>('api_get_summary', { meetingId });
        const current = summaryPollsRef.current.get(meetingId);
        if (current !== entry || result.start !== processId) {
          return;
        }
        await onUpdate(result);
        if (summaryPollsRef.current.get(meetingId) !== entry) return;
        if (
          result.status === 'completed'
          || result.status === 'error'
          || result.status === 'failed'
          || result.status === 'cancelled'
          || (result.status === 'idle' && pollCount > 1)
        ) {
          stopSummaryPolling(meetingId, processId);
        }
      } catch (error) {
        const current = summaryPollsRef.current.get(meetingId);
        if (current !== entry) {
          return;
        }
        try {
          await onUpdate({
            status: 'error',
            meetingName: null,
            meeting_id: meetingId,
            start: processId,
            end: null,
            data: null,
            error: error instanceof Error ? error.message : 'Unknown error',
          });
        } catch (callbackError) {
          console.error('Failed to handle summary polling error:', callbackError);
        } finally {
          if (summaryPollsRef.current.get(meetingId) === entry) {
            stopSummaryPolling(meetingId, processId);
          }
        }
      } finally {
        const current = summaryPollsRef.current.get(meetingId);
        if (current === entry) {
          current.inFlight = false;
        }
      }
    };

    const timer = setInterval(() => void poll(), 5000);
    summaryPollsRef.current.set(meetingId, { processId, timer, inFlight: false });
  }, [stopSummaryPolling]);

  useEffect(() => () => {
    summaryPollsRef.current.forEach(({ timer }) => clearInterval(timer));
    summaryPollsRef.current.clear();
  }, []);



  return (
    <SidebarContext.Provider value={{
      currentMeeting,
      setCurrentMeeting,
      sidebarItems,
      isCollapsed,
      toggleCollapse,
      meetings,
      setMeetings,
      isMeetingActive,
      setIsMeetingActive,
      handleRecordingToggle,
      startNotesMeeting,
      searchTranscripts,
      searchResults,
      isSearching,
      startSummaryPolling,
      stopSummaryPolling,
      refetchMeetings: fetchMeetings,
      folders,
      createFolder,
      renameFolder,
      deleteFolder,
      moveMeetingToFolder,
    }}>
      {children}
    </SidebarContext.Provider>
  );
}
