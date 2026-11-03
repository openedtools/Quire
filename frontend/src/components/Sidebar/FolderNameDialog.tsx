'use client';

import React, { useState } from 'react';
import { Dialog, DialogContent, DialogFooter, DialogTitle } from '@/components/ui/dialog';
import { VisuallyHidden } from '@/components/ui/visually-hidden';

interface FolderNameDialogProps {
  title: string;
  confirmLabel: string;
  initialName?: string;
  /** Return true when saved; the dialog stays open on false so the name can be corrected. */
  onSubmit: (name: string) => Promise<boolean>;
  onClose: () => void;
}

/** Mount it only while open so the name field starts fresh each time. */
export function FolderNameDialog({ title, confirmLabel, initialName = '', onSubmit, onClose }: FolderNameDialogProps) {
  const [name, setName] = useState(initialName);
  const [isSaving, setIsSaving] = useState(false);
  const trimmed = name.trim();

  const submit = async () => {
    if (!trimmed || isSaving) return;
    setIsSaving(true);
    try {
      if (await onSubmit(trimmed)) onClose();
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent className="sm:max-w-[425px]">
        <VisuallyHidden>
          <DialogTitle>{title}</DialogTitle>
        </VisuallyHidden>
        <div className="py-4">
          <h3 className="text-lg font-semibold mb-4">{title}</h3>
          <label htmlFor="folder-name" className="block text-sm font-medium text-ink mb-2">
            Folder name
          </label>
          <input
            id="folder-name"
            type="text"
            value={name}
            maxLength={80}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void submit();
            }}
            className="w-full px-3 py-2 border border-line-strong rounded-md focus:outline-none focus:ring-2 focus:ring-accent focus:border-transparent"
            placeholder="e.g. Weekly syncs"
            autoFocus
          />
        </div>
        <DialogFooter>
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm font-medium text-ink bg-surface hover:bg-surface-hover rounded-md transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={() => void submit()}
            disabled={!trimmed || isSaving}
            className="px-4 py-2 text-sm font-medium text-on-accent bg-accent-strong hover:bg-accent-strong-hover rounded-md transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {confirmLabel}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
