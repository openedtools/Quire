'use client';

import React, { useState } from 'react';
import { Check, Folder, FolderInput, FolderPlus, NotebookPen } from 'lucide-react';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Command, CommandGroup, CommandItem, CommandList, CommandSeparator } from '@/components/ui/command';
import { useSidebar } from './SidebarProvider';

interface MoveToFolderMenuProps {
  meetingId: string;
  currentFolderId?: string | null;
  /** Called after a successful move so the sidebar can reveal the target folder. */
  onMoved: (folderId: string | null) => void;
  /** Ask the sidebar to create a folder and then file this meeting in it. */
  onNewFolder: (meetingId: string) => void;
}

/** Hover button on a meeting row: pick the folder it belongs in. */
export function MoveToFolderMenu({ meetingId, currentFolderId, onMoved, onNewFolder }: MoveToFolderMenuProps) {
  const { folders, moveMeetingToFolder } = useSidebar();
  const [open, setOpen] = useState(false);

  const choose = async (folderId: string | null) => {
    if (folderId !== (currentFolderId ?? null) && !(await moveMeetingToFolder(meetingId, folderId))) return;
    setOpen(false);
    onMoved(folderId);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          onClick={(e) => e.stopPropagation()}
          className="hover:text-accent-strong p-1 rounded-md hover:bg-accent-tint flex-shrink-0"
          aria-label="Move to folder"
          title="Move to folder"
        >
          <FolderInput className="w-4 h-4" />
        </button>
      </PopoverTrigger>
      {/* The popover is portaled but React events still bubble to the meeting row, which would open the meeting */}
      <PopoverContent align="start" className="w-56 p-0" onClick={(e) => e.stopPropagation()}>
        <Command>
          <CommandList>
            <CommandGroup heading="Move to">
              <CommandItem value="__none__" onSelect={() => choose(null)}>
                <Check className={`mr-2 h-4 w-4 ${currentFolderId ? 'opacity-0' : 'opacity-100'}`} />
                <NotebookPen className="mr-2 h-4 w-4 text-ink-muted" />
                <span className="truncate">No folder</span>
              </CommandItem>
              {folders.map((f) => (
                <CommandItem key={f.id} value={f.name} onSelect={() => choose(f.id)}>
                  <Check className={`mr-2 h-4 w-4 ${currentFolderId === f.id ? 'opacity-100' : 'opacity-0'}`} />
                  <Folder className="mr-2 h-4 w-4 text-ink-muted" />
                  <span className="truncate">{f.name}</span>
                </CommandItem>
              ))}
            </CommandGroup>
            <CommandSeparator />
            <CommandGroup>
              <CommandItem
                value="__new__"
                onSelect={() => {
                  setOpen(false);
                  onNewFolder(meetingId);
                }}
              >
                <FolderPlus className="mr-2 h-4 w-4 text-accent-strong" />
                New folder…
              </CommandItem>
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
