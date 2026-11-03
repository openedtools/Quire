'use client';

import { BRAND } from '@/lib/brand.generated';
import React, { useState, useMemo, useEffect, useCallback, useRef } from 'react';
import { ChevronDown, ChevronRight, File, Folder, FolderOpen, FolderPlus, Users, Settings, Calendar, StickyNote, Home, Trash2, Mic, Square, Plus, Search, Pencil, NotebookPen, SearchIcon, X, Upload, PanelLeftClose, PanelLeftOpen, Sparkles } from 'lucide-react';
import { useRouter, usePathname } from 'next/navigation';
import { useSidebar } from './SidebarProvider';
import type { CurrentMeeting, SidebarItem } from '@/components/Sidebar/SidebarProvider';
import { FolderNameDialog } from './FolderNameDialog';
import { MoveToFolderMenu } from './MoveToFolderMenu';
import { ConfirmationModal } from '../ConfirmationModel/confirmation-modal';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { SettingTabs } from '../SettingTabs';
import { TranscriptModelProps } from '@/components/TranscriptSettings';
import { invoke } from '@tauri-apps/api/core';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { toast } from 'sonner';
import { useRecordingState } from '@/contexts/RecordingStateContext';
import { useImportDialog } from '@/contexts/ImportDialogContext';
import { useConfig } from '@/contexts/ConfigContext';

import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog"
import { VisuallyHidden } from "@/components/ui/visually-hidden"

import { MessageToast } from '../MessageToast';
import Logo from '../Logo';
import Info from '../Info';
import { Input } from '../ui/input';
import { InputGroup, InputGroupAddon, InputGroupButton, InputGroupInput } from '../ui/input-group';
import { saveWithRemoteConfirmation } from '@/lib/cloud';

const Sidebar: React.FC = () => {
  const router = useRouter();
  const pathname = usePathname();
  const {
    currentMeeting,
    setCurrentMeeting,
    sidebarItems,
    isCollapsed,
    toggleCollapse,
    handleRecordingToggle,
    startNotesMeeting,
    searchTranscripts,
    searchResults,
    isSearching,
    meetings,
    setMeetings,
    createFolder,
    renameFolder,
    deleteFolder,
    moveMeetingToFolder,
  } = useSidebar();

  // Get recording state from RecordingStateContext (single source of truth)
  const { isRecording } = useRecordingState();
  const { openImportDialog } = useImportDialog();
  const { betaFeatures } = useConfig();
  const [expandedFolders, setExpandedFolders] = useState<Set<string>>(new Set(['meetings']));
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [showModelSettings, setShowModelSettings] = useState(false);
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: '',
    whisperModel: '',
    apiKey: null,
    ollamaEndpoint: null
  });
  const [transcriptModelConfig, setTranscriptModelConfig] = useState<TranscriptModelProps>({
    provider: 'parakeet',
    model: 'parakeet-tdt-0.6b-v3-int8',
  });
  const [settingsSaveSuccess, setSettingsSaveSuccess] = useState<boolean | null>(null);

  // State for edit modal
  const [editModalState, setEditModalState] = useState<{ isOpen: boolean; meetingId: string | null; currentTitle: string }>({
    isOpen: false,
    meetingId: null,
    currentTitle: ''
  });
  const [editingTitle, setEditingTitle] = useState<string>('');
  const [suggesting, setSuggesting] = useState(false);
  const editModalIdRef = useRef<string | null>(null);

  // Ensure 'meetings' folder is always expanded
  useEffect(() => {
    if (!expandedFolders.has('meetings')) {
      const newExpanded = new Set(expandedFolders);
      newExpanded.add('meetings');
      setExpandedFolders(newExpanded);
    }
  }, [expandedFolders]);

  // useEffect(() => {
  //   if (settingsSaveSuccess !== null) {
  //     const timer = setTimeout(() => {
  //       setSettingsSaveSuccess(null);
  //     }, 3000);
  //   }
  // }, [settingsSaveSuccess]);


  const [deleteModalState, setDeleteModalState] = useState<{ isOpen: boolean; itemId: string | null }>({ isOpen: false, itemId: null });

  // Folder dialogs: create (optionally filing a meeting in the new folder), rename, delete
  const [folderDialog, setFolderDialog] = useState<
    { mode: 'create' | 'rename'; folderId?: string; initialName?: string; moveMeetingId?: string } | null
  >(null);
  const [deleteFolderState, setDeleteFolderState] = useState<{ id: string; name: string; count: number } | null>(null);

  const expandFolder = (folderId: string) =>
    setExpandedFolders(prev => (prev.has(folderId) ? prev : new Set(prev).add(folderId)));

  const handleFolderDialogSubmit = async (name: string): Promise<boolean> => {
    if (!folderDialog) return false;
    if (folderDialog.mode === 'rename' && folderDialog.folderId) {
      return renameFolder(folderDialog.folderId, name);
    }
    const folder = await createFolder(name);
    if (!folder) return false;
    if (folderDialog.moveMeetingId) {
      await moveMeetingToFolder(folderDialog.moveMeetingId, folder.id);
    }
    expandFolder(folder.id);
    return true;
  };

  const handleDeleteFolderConfirm = async () => {
    if (!deleteFolderState) return;
    const { id } = deleteFolderState;
    setDeleteFolderState(null);
    await deleteFolder(id);
  };

  useEffect(() => {
    // Note: Don't set hardcoded defaults - let DB be the source of truth
    const fetchModelConfig = async () => {
      try {
        const data = await invoke('api_get_model_config') as any;
        if (data && data.provider !== null) {
          setModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch model config:', error);
      }
    };

    fetchModelConfig();
  }, []);


  useEffect(() => {
    // Note: Don't set hardcoded defaults - let DB be the source of truth
    const fetchTranscriptSettings = async () => {
      try {
        const data = await invoke('api_get_transcript_config') as any;
        if (data && data.provider !== null) {
          setTranscriptModelConfig(data);
        }
      } catch (error) {
        console.error('Failed to fetch transcript settings:', error);
      }
    };
    fetchTranscriptSettings();
  }, []);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        setModelConfig({ ...event.payload, apiKey: null });
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, []);



  // Handle model config save
  const handleSaveModelConfig = async (config: ModelConfig) => {
    try {
      // A new address on another computer is confirmed first (spec F-MOD-7)
      const saved = await saveWithRemoteConfirmation((confirmRemote) =>
        invoke('api_save_model_config', {
          provider: config.provider,
          model: config.model,
          whisperModel: config.whisperModel,
          apiKey: config.apiKey,
          ollamaEndpoint: config.ollamaEndpoint,
          confirmRemote,
        }),
      );
      if (saved === null) return false;

      // The typed key went to the core; nothing else keeps it (spec F-MOD-6)
      const withoutKey = { ...config, apiKey: null, customOpenAIApiKey: null };
      setModelConfig(withoutKey);
      setSettingsSaveSuccess(true);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', withoutKey);
      return true;
    } catch (error) {
      console.error('Error saving model config:', error);
      setSettingsSaveSuccess(false);
      toast.error('Failed to save model settings', { description: String(error) });
      return false;
    }
  };

  const handleSaveTranscriptConfig = async (updatedConfig?: TranscriptModelProps) => {
    try {
      const configToSave = updatedConfig || transcriptModelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        apiKey: null
      };

      await invoke('api_save_transcript_config', {
        provider: payload.provider,
        model: payload.model,
        apiKey: payload.apiKey,
      });


      setSettingsSaveSuccess(true);

    } catch (error) {
      console.error('Failed to save transcript config:', error);
      setSettingsSaveSuccess(false);
    }
  };

  // Handle search input changes
  const handleSearchChange = useCallback(async (value: string) => {
    setSearchQuery(value);

    // If search query is empty, just return to normal view
    if (!value.trim()) return;

    // Search through transcripts
    await searchTranscripts(value);

    // Make sure the meetings folder is expanded when searching
    if (!expandedFolders.has('meetings')) {
      const newExpanded = new Set(expandedFolders);
      newExpanded.add('meetings');
      setExpandedFolders(newExpanded);
    }
  }, [expandedFolders, searchTranscripts]);

  // Combine search results with sidebar items. A meeting matches on its title or on a
  // transcript hit; a user folder stays when its name matches (showing everything in it)
  // or when something inside it matches.
  const filteredSidebarItems = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return sidebarItems;

    const matchedMeetingIds = new Set(searchResults.map(result => result.id));
    const titleMatches = (item: SidebarItem) => item.title.toLowerCase().includes(query);

    const filterItems = (items: SidebarItem[]): SidebarItem[] =>
      items.flatMap((item): SidebarItem[] => {
        if (item.type === 'file') {
          return matchedMeetingIds.has(item.id) || titleMatches(item) ? [item] : [];
        }
        if (item.userFolder && titleMatches(item)) return [item];
        const children = filterItems(item.children ?? []);
        // Always keep the root; hide user folders with nothing matching inside
        return item.userFolder && children.length === 0 ? [] : [{ ...item, children }];
      });

    return filterItems(sidebarItems);
  }, [sidebarItems, searchQuery, searchResults]);


  const handleDelete = async (itemId: string) => {
    console.log('Deleting item:', itemId);
    const payload = {
      meetingId: itemId
    };

    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const result = await invoke('api_delete_meeting', {
        meetingId: itemId,
      }) as { pending?: boolean } | null;
      if (result?.pending) {
        // A recording file is in use: the meeting stays, marked, until the file can be removed
        setMeetings(meetings.map((m: CurrentMeeting) =>
          m.id === itemId && !m.title.endsWith(' (deleting)') ? { ...m, title: `${m.title} (deleting)` } : m));
        toast.warning("Meeting is being deleted", {
          description: "A recording file is in use by another program. It is removed when the file is free, at the latest the next time the app starts."
        });
      } else {
        setMeetings(meetings.filter((m: CurrentMeeting) => m.id !== itemId));
        toast.success("Meeting deleted", {
          description: "Its notes, transcript, summary and recording files are removed"
        });
      }

      // If deleting the active meeting, navigate to home
      if (currentMeeting?.id === itemId) {
        setCurrentMeeting({ id: 'intro-call', title: '+ New Call' });
        router.push('/');
      }
    } catch (error) {
      console.error('Failed to delete meeting:', error);
      toast.error("Failed to delete meeting", {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleDeleteConfirm = () => {
    if (deleteModalState.itemId) {
      handleDelete(deleteModalState.itemId);
    }
    setDeleteModalState({ isOpen: false, itemId: null });
  };

  editModalIdRef.current = editModalState.isOpen ? editModalState.meetingId : null;

  // Handle modal editing of meeting names
  const handleEditStart = (meetingId: string, currentTitle: string) => {
    setEditModalState({
      isOpen: true,
      meetingId: meetingId,
      currentTitle: currentTitle
    });
    setEditingTitle(currentTitle);
  };

  // "Suggest a name" (plan PL1): the chosen model's name, in the person's format, into the box
  const handleSuggestName = async () => {
    const meetingId = editModalState.meetingId;
    if (!meetingId) return;
    setSuggesting(true);
    try {
      const name = await invoke<string>('api_suggest_meeting_name', { meetingId });
      // Only into the box it was asked for, if that box is still open
      if (editModalIdRef.current === meetingId) setEditingTitle(name);
    } catch (error) {
      toast.error(typeof error === 'string' ? error : 'A name could not be suggested');
    } finally {
      setSuggesting(false);
    }
  };

  const handleEditConfirm = async () => {
    const newTitle = editingTitle.trim();
    const meetingId = editModalState.meetingId;

    if (!meetingId) return;

    // Prevent empty titles
    if (!newTitle) {
      toast.error("Meeting title cannot be empty");
      return;
    }

    try {
      await invoke('api_save_meeting_title', {
        meetingId: meetingId,
        title: newTitle,
      });

      // Update local state
      const updatedMeetings = meetings.map((m: CurrentMeeting) =>
        m.id === meetingId ? { ...m, title: newTitle } : m
      );
      setMeetings(updatedMeetings);

      // Update current meeting if it's the one being edited
      if (currentMeeting?.id === meetingId) {
        setCurrentMeeting({ id: meetingId, title: newTitle });
      }

      toast.success("Meeting title updated successfully");

      // Close modal and reset state
      setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
      setEditingTitle('');
    } catch (error) {
      console.error('Failed to update meeting title:', error);
      toast.error("Failed to update meeting title", {
        description: error instanceof Error ? error.message : String(error)
      });
    }
  };

  const handleEditCancel = () => {
    setEditModalState({ isOpen: false, meetingId: null, currentTitle: '' });
    setEditingTitle('');
  };

  const toggleFolder = (folderId: string) => {
    // Normal toggle behavior for all folders
    const newExpanded = new Set(expandedFolders);
    if (newExpanded.has(folderId)) {
      newExpanded.delete(folderId);
    } else {
      newExpanded.add(folderId);
    }
    setExpandedFolders(newExpanded);
  };

  // Expose setShowModelSettings to window for Rust tray to call
  useEffect(() => {
    (window as any).openSettings = () => {
      setShowModelSettings(true);
    };

    // Cleanup on unmount
    return () => {
      delete (window as any).openSettings;
    };
  }, []);

  const [appVersion, setAppVersion] = useState('');
  useEffect(() => {
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion()).then(setAppVersion).catch(() => {});
  }, []);

  /** Enter or Space on a focused clickable row does what a click does (rows are not buttons
   *  because they contain buttons of their own). */
  const activateOnKey = (e: React.KeyboardEvent<HTMLElement>) => {
    if ((e.key === 'Enter' || e.key === ' ') && e.target === e.currentTarget) {
      e.preventDefault();
      e.currentTarget.click();
    }
  };

  const renderCollapsedIcons = () => {
    if (!isCollapsed) return null;

    const isHomePage = pathname === '/';
    const isMeetingPage = pathname?.includes('/meeting-details');
    const isSettingsPage = pathname === '/settings';

    return (
      <TooltipProvider>
        <div className="flex flex-col items-center space-y-3 mt-3">
          <Logo isCollapsed={isCollapsed} />
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                onClick={toggleCollapse}
                aria-label="Show meeting list"
                aria-expanded={false}
                className="flex h-9 w-9 items-center justify-center rounded-control text-ink-muted hover:bg-surface-hover hover:text-ink transition-colors"
              >
                <PanelLeftOpen className="w-5 h-5" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Show meeting list</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label="Home"
                onClick={() => router.push('/')}
                className={`p-2 rounded-control transition-colors duration-150 ${isHomePage ? 'bg-surface-hover' : 'hover:bg-surface-hover'
                  }`}
              >
                <Home className="w-5 h-5 text-ink-muted" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Home</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label="People"
                onClick={() => router.push('/people')}
                className={`p-2 rounded-control transition-colors duration-150 ${pathname === '/people' ? 'bg-surface-hover' : 'hover:bg-surface-hover'
                  }`}
              >
                <Users className="w-5 h-5 text-ink-muted" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>People</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label="New meeting"
                onClick={() => void startNotesMeeting()}
                className="p-2 bg-accent-strong hover:bg-accent-strong-hover active:bg-accent-strong-pressed rounded-control transition-colors duration-150"
              >
                <NotebookPen className="w-5 h-5 text-on-accent" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>New meeting</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label={isRecording ? 'Recording in progress' : 'Start recording'}
                onClick={handleRecordingToggle}
                disabled={isRecording}
                className={`p-2 ${isRecording ? 'bg-danger cursor-not-allowed' : 'hover:bg-surface-hover'} rounded-control transition-colors duration-150`}
              >
                {isRecording ? (
                  <Square className="w-5 h-5 text-on-accent" />
                ) : (
                  <Mic className="w-5 h-5 text-ink-muted" />
                )}
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>{isRecording ? "Recording in progress..." : "Start Recording"}</p>
            </TooltipContent>
          </Tooltip>

          {betaFeatures.importAndRetranscribe && (
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  aria-label="Import audio"
                onClick={() => openImportDialog()}
                  className="p-2 rounded-control transition-colors duration-150 hover:bg-surface-hover"
                >
                  <Upload className="w-5 h-5 text-ink-muted" />
                </button>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>Import Audio</p>
              </TooltipContent>
            </Tooltip>
          )}

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label="Meeting notes"
                onClick={() => {
                  if (isCollapsed) toggleCollapse();
                  toggleFolder('meetings');
                }}
                className={`p-2 rounded-control transition-colors duration-150 ${isMeetingPage ? 'bg-surface-hover' : 'hover:bg-surface-hover'
                  }`}
              >
                <NotebookPen className="w-5 h-5 text-ink-muted" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Meeting Notes</p>
            </TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label="Settings"
                onClick={() => router.push('/settings')}
                className={`p-2 rounded-control transition-colors duration-150 ${isSettingsPage ? 'bg-surface-hover' : 'hover:bg-surface-hover'
                  }`}
              >
                <Settings className="w-5 h-5 text-ink-muted" />
              </button>
            </TooltipTrigger>
            <TooltipContent side="right">
              <p>Settings</p>
            </TooltipContent>
          </Tooltip>

          <Info isCollapsed={isCollapsed} />
        </div>
      </TooltipProvider>
    );
  };

  // Find matching transcript snippet for a meeting item
  const findMatchingSnippet = (itemId: string) => {
    if (!searchQuery.trim() || !searchResults.length) return null;
    return searchResults.find(result => result.id === itemId);
  };

  const renderItem = (item: SidebarItem, depth = 0) => {
    // While searching, folders with hits open themselves so the matches are visible
    const isExpanded = expandedFolders.has(item.id) || (!!searchQuery.trim() && !!item.userFolder);
    const paddingLeft = `${depth * 12 + 8}px`;
    // The last-opened meeting is not "active" while the People page is showing
    const isActive = item.type === 'file' && currentMeeting?.id === item.id && pathname !== '/people';
    const isMeetingItem = item.id.includes('-') && !item.id.startsWith('intro-call');

    // Check if this item has a matching transcript snippet
    const matchingResult = isMeetingItem ? findMatchingSnippet(item.id) : null;
    const hasTranscriptMatch = !!matchingResult;

    if (isCollapsed) return null;

    return (
      <div key={item.id}>
        <div
          className={`flex items-center transition-all duration-150 group ${item.type === 'folder' && depth === 0
            ? 'p-3 text-lg font-semibold h-10 mx-3 mt-3 rounded-lg'
            : `px-2 min-h-8 py-1 my-px rounded-control text-sm ${isActive ? 'bg-surface-hover text-ink font-medium' :
              hasTranscriptMatch ? 'bg-warning-tint' : 'text-ink hover:bg-surface-hover'
            } cursor-pointer`
            }`}
          style={item.type === 'folder' && depth === 0 ? {} : { paddingLeft }}
          role="button"
          tabIndex={0}
          aria-expanded={item.type === 'folder' ? isExpanded : undefined}
          aria-current={isActive ? 'page' : undefined}
          onKeyDown={activateOnKey}
          onClick={() => {
            if (item.type === 'folder') {
              toggleFolder(item.id);
            } else {
              setCurrentMeeting({ id: item.id, title: item.title });
              const basePath = item.id.startsWith('intro-call') ? '/' :
                item.id.includes('-') ? `/meeting-details?id=${item.id}` : `/notes/${item.id}`;
              router.push(basePath);
            }
          }}
        >
          {item.type === 'folder' && item.userFolder ? (
            <>
              {isExpanded ? (
                <FolderOpen className="w-4 h-4 mr-2 flex-shrink-0 text-ink-muted" />
              ) : (
                <Folder className="w-4 h-4 mr-2 flex-shrink-0 text-ink-muted" />
              )}
              <span className="flex-1 truncate" title={item.title}>{item.title}</span>
              <span className="text-xs text-ink-muted mx-1">{item.children?.length ?? 0}</span>
              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity duration-150">
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setFolderDialog({ mode: 'rename', folderId: item.id, initialName: item.title });
                  }}
                  className="hover:text-accent-strong p-1 rounded-md hover:bg-accent-tint flex-shrink-0"
                  aria-label="Rename folder"
                >
                  <Pencil className="w-4 h-4" />
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    setDeleteFolderState({ id: item.id, name: item.title, count: item.children?.length ?? 0 });
                  }}
                  className="hover:text-danger p-1 rounded-md hover:bg-danger-tint flex-shrink-0"
                  aria-label="Delete folder"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>
              {isExpanded ? (
                <ChevronDown className="w-4 h-4 ml-1 text-ink-muted flex-shrink-0" />
              ) : (
                <ChevronRight className="w-4 h-4 ml-1 text-ink-muted flex-shrink-0" />
              )}
            </>
          ) : item.type === 'folder' ? (
            <>
              {item.id === 'meetings' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : item.id === 'notes' ? (
                <Calendar className="w-4 h-4 mr-2" />
              ) : null}
              <span className={depth === 0 ? "" : "font-medium"}>{item.title}</span>
              <div className="ml-auto">
                {isExpanded ? (
                  <ChevronDown className="w-4 h-4 text-ink-muted" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-ink-muted" />
                )}
              </div>
              {searchQuery && item.id === 'meetings' && isSearching && (
                <span className="ml-2 text-xs text-accent-strong animate-pulse">Searching...</span>
              )}
            </>
          ) : (
            <div className="flex flex-col w-full">
              <div className="flex items-center w-full">
                {isMeetingItem ? (
                  <File className="w-4 h-4 mr-2 flex-shrink-0 text-ink-muted" />
                ) : (
                  <div className="flex-shrink-0 flex items-center justify-center w-6 h-6 rounded-full mr-2 bg-accent-tint">
                    <Plus className="w-3.5 h-3.5 text-accent-strong" />
                  </div>
                )}
                <span className="flex-1 truncate" title={item.title}>{item.title}</span>
                {isMeetingItem && (
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 focus-within:opacity-100 transition-opacity duration-150">
                    <MoveToFolderMenu
                      meetingId={item.id}
                      currentFolderId={meetings.find(m => m.id === item.id)?.folder_id}
                      onMoved={(folderId) => { if (folderId) expandFolder(folderId); }}
                      onNewFolder={(meetingId) => setFolderDialog({ mode: 'create', moveMeetingId: meetingId })}
                    />
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleEditStart(item.id, item.title);
                      }}
                      className="hover:text-accent-strong p-1 rounded-md hover:bg-accent-tint flex-shrink-0"
                      aria-label="Edit meeting title"
                    >
                      <Pencil className="w-4 h-4" />
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteModalState({ isOpen: true, itemId: item.id });
                      }}
                      className="hover:text-danger p-1 rounded-md hover:bg-danger-tint flex-shrink-0"
                      aria-label="Delete meeting"
                    >
                      <Trash2 className="w-4 h-4" />
                    </button>
                  </div>
                )}
              </div>

              {/* Show transcript match snippet if available */}
              {hasTranscriptMatch && (
                <div className="mt-1 ml-8 text-xs text-ink-muted bg-warning-tint p-1.5 rounded border border-warning/40 line-clamp-2">
                  <span className="font-medium text-warning">Match:</span> {matchingResult.matchContext}
                </div>
              )}
            </div>
          )}
        </div>
        {item.type === 'folder' && isExpanded && item.children && (
          <div className="ml-1">
            {item.children.map(child => renderItem(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="fixed top-0 left-0 h-screen z-40">
      <div
        className={`h-screen bg-surface border-r border-line flex flex-col transition-all duration-300 ${isCollapsed ? 'w-16' : 'w-64'
          }`}
      >
        {/*  Header with traffic light spacing */}
        <div className="flex-shrink-0 h-22 flex items-center">

          {/* Title container */}



          <div className="flex-1">
            {!isCollapsed && (
              <div className="px-3 pt-3 space-y-3">
                <div className="flex items-center justify-between">
                  <Logo isCollapsed={isCollapsed} />
                  <button
                    onClick={toggleCollapse}
                    aria-label="Hide meeting list"
                    aria-expanded={true}
                    title="Hide meeting list"
                    className="flex h-8 w-8 items-center justify-center rounded-control text-ink-muted hover:bg-surface-hover hover:text-ink transition-colors"
                  >
                    <PanelLeftClose className="w-4 h-4" />
                  </button>
                </div>

                <button
                  onClick={() => void startNotesMeeting()}
                  className="w-full flex items-center gap-2 h-9 px-3 rounded-control text-sm font-medium transition-colors bg-accent-strong text-on-accent hover:bg-accent-strong-hover active:bg-accent-strong-pressed"
                >
                  <NotebookPen className="w-4 h-4" />
                  <span>New meeting</span>
                </button>

                <button
                  onClick={handleRecordingToggle}
                  disabled={isRecording}
                  className={`w-full flex items-center gap-2 h-9 px-3 rounded-control text-sm font-medium transition-colors ${isRecording ? 'bg-danger-tint text-danger cursor-not-allowed' : 'border border-line bg-paper text-ink hover:bg-surface-hover'}`}
                >
                  {isRecording ? <Square className="w-4 h-4" /> : <Mic className="w-4 h-4" />}
                  <span>{isRecording ? 'Recording in progress' : 'Record now'}</span>
                </button>

                <div className="relative mb-1">
                  <InputGroup >
                    <InputGroupInput placeholder='Search meeting content...' value={searchQuery}
                      onChange={(e) => handleSearchChange(e.target.value)}
                    />
                    <InputGroupAddon>
                      <SearchIcon />
                    </InputGroupAddon>
                    {searchQuery &&
                      <InputGroupAddon align={'inline-end'}>
                        <InputGroupButton
                          onClick={() => handleSearchChange('')}
                        >
                          <X />
                        </InputGroupButton>
                      </InputGroupAddon>
                    }
                  </InputGroup>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Main content - scrollable area */}
        <div className="flex-1 flex flex-col min-h-0">
          {/* Fixed navigation items */}
          <div className="flex-shrink-0">
            {!isCollapsed && (
              <div
                role="button"
                tabIndex={0}
                onKeyDown={activateOnKey}
                onClick={() => router.push('/')}
                className={`flex items-center gap-2 h-8 px-2 mx-2 rounded-control text-sm font-medium cursor-pointer transition-colors mt-3 ${pathname === '/' ? 'bg-surface-hover text-ink' : 'text-ink-muted hover:bg-surface-hover hover:text-ink'}`}
              >
                <Home className="w-4 h-4" />
                <span>Home</span>
              </div>
            )}
            {!isCollapsed && (
              <div
                role="button"
                tabIndex={0}
                onKeyDown={activateOnKey}
                onClick={() => router.push('/people')}
                className={`flex items-center gap-2 h-8 px-2 mx-2 rounded-control text-sm font-medium cursor-pointer transition-colors mt-0.5 ${pathname === '/people' ? 'bg-surface-hover text-ink' : 'text-ink-muted hover:bg-surface-hover hover:text-ink'}`}
              >
                <Users className="w-4 h-4" />
                <span>People</span>
              </div>
            )}
          </div>

          {/* Content area */}
          <div className="flex-1 flex flex-col min-h-0">
            {renderCollapsedIcons()}
            {/* Meeting Notes folder header - fixed */}
            {!isCollapsed && (
              <div className="flex-shrink-0">
                {filteredSidebarItems.filter(item => item.type === 'folder').map(item => (
                  <div key={item.id}>
                    <div
                      className="flex items-center gap-2 h-8 px-2 mx-2 mt-4 text-xs font-semibold uppercase tracking-wide text-ink-muted"
                    >
                      <span>{item.title}</span>
                      {searchQuery && item.id === 'meetings' && isSearching && (
                        <span className="ml-2 text-xs text-accent-strong animate-pulse">Searching...</span>
                      )}
                      <button
                        onClick={() => setFolderDialog({ mode: 'create' })}
                        className="ml-auto p-1 rounded-md text-ink-muted hover:text-accent-strong hover:bg-accent-tint"
                        aria-label="New folder"
                        title="New folder"
                      >
                        <FolderPlus className="w-4 h-4" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}

            {/* Scrollable meeting items */}
            {!isCollapsed && (
              <div className="flex-1 overflow-y-auto custom-scrollbar min-h-0">
                {filteredSidebarItems
                  .filter(item => item.type === 'folder' && expandedFolders.has(item.id) && item.children)
                  .map(item => (
                    <div key={`${item.id}-children`} className="mx-2">
                      {item.children!.map(child => renderItem(child, 1))}
                    </div>
                  ))}
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        {!isCollapsed && (

          <div className="flex-shrink-0 p-2 border-t border-line">
            {betaFeatures.importAndRetranscribe && (
              <button
                onClick={() => openImportDialog()}
                className="w-full flex items-center gap-2 h-8 px-2 rounded-control text-sm text-ink-muted hover:bg-surface-hover hover:text-ink transition-colors"
              >
                <Upload className="w-4 h-4" />
                <span>Import Audio</span>
              </button>
            )}

            <button
              onClick={() => router.push('/settings')}
              className="w-full flex items-center gap-2 h-8 px-2 rounded-control text-sm text-ink-muted hover:bg-surface-hover hover:text-ink transition-colors"
            >
              <Settings className="w-4 h-4" />
              <span>Settings</span>
            </button>
            <Info isCollapsed={isCollapsed} />
            <div className="px-2 pt-1 text-xs text-ink-muted">
              {BRAND.name}{appVersion && ` ${appVersion}`}
            </div>
          </div>
        )}
      </div>

      {/* Confirmation Modal for Delete */}
      <ConfirmationModal
        isOpen={deleteModalState.isOpen}
        text="Are you sure you want to delete this meeting? This action cannot be undone."
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteModalState({ isOpen: false, itemId: null })}
      />

      {/* Folder: create / rename */}
      {folderDialog && (
        <FolderNameDialog
          title={folderDialog.mode === 'rename' ? 'Rename Folder' : 'New Folder'}
          confirmLabel={folderDialog.mode === 'rename' ? 'Save' : 'Create'}
          initialName={folderDialog.initialName}
          onSubmit={handleFolderDialogSubmit}
          onClose={() => setFolderDialog(null)}
        />
      )}

      {/* Folder: confirm delete (meetings are kept) */}
      <ConfirmationModal
        isOpen={!!deleteFolderState}
        text={deleteFolderState
          ? `Delete the folder "${deleteFolderState.name}"? ${deleteFolderState.count === 0
            ? 'It is empty.'
            : deleteFolderState.count === 1
              ? 'Its 1 meeting will not be deleted; it moves back to Meeting Notes.'
              : `Its ${deleteFolderState.count} meetings will not be deleted; they move back to Meeting Notes.`}`
          : ''}
        onConfirm={handleDeleteFolderConfirm}
        onCancel={() => setDeleteFolderState(null)}
      />

      {/* Edit Meeting Title Modal */}
      <Dialog open={editModalState.isOpen} onOpenChange={(open) => {
        if (!open) handleEditCancel();
      }}>
        <DialogContent className="sm:max-w-[425px]">
          <VisuallyHidden>
            <DialogTitle>Edit Meeting Title</DialogTitle>
          </VisuallyHidden>
          <div className="py-4">
            <h3 className="text-lg font-semibold mb-4">Edit Meeting Title</h3>
            <div className="space-y-4">
              <div>
                <label htmlFor="meeting-title" className="block text-sm font-medium text-ink mb-2">
                  Meeting Title
                </label>
                <input
                  id="meeting-title"
                  type="text"
                  value={editingTitle}
                  onChange={(e) => setEditingTitle(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') {
                      handleEditConfirm();
                    } else if (e.key === 'Escape') {
                      handleEditCancel();
                    }
                  }}
                  className="w-full px-3 py-2 border border-line-strong rounded-md focus:outline-none focus:ring-2 focus:ring-accent focus:border-transparent"
                  placeholder="Enter meeting title"
                  autoFocus
                />
              </div>
              <button
                type="button"
                onClick={handleSuggestName}
                disabled={suggesting}
                className="inline-flex items-center gap-2 min-h-9 px-3 text-sm rounded-control border border-line-strong bg-surface text-ink hover:bg-surface-hover disabled:opacity-60"
              >
                <Sparkles className="w-4 h-4" aria-hidden="true" />
                {suggesting ? 'Suggesting a name...' : 'Suggest a name'}
              </button>
            </div>
          </div>
          <DialogFooter>
            <button
              onClick={handleEditCancel}
              className="px-4 py-2 text-sm font-medium text-ink bg-surface hover:bg-surface-hover rounded-md transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleEditConfirm}
              className="px-4 py-2 text-sm font-medium text-on-accent bg-accent-strong hover:bg-accent-strong-hover rounded-md transition-colors"
            >
              Save
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
};

export default Sidebar;
