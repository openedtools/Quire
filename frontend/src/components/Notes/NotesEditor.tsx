'use client';

import { useEffect } from 'react';
import type { Block, PartialBlock } from '@blocknote/core';
import { useCreateBlockNote } from '@blocknote/react';
import { BlockNoteView } from '@blocknote/shadcn';
import '@blocknote/shadcn/style.css';
import { useTheme } from '@/contexts/ThemeContext';
import { EditorKeyboardExit } from '../BlockNoteEditor/EditorKeyboardExit';
import { onAppendToNotes } from '@/lib/meetingEvents';

export interface NotesContent {
  /** Editor JSON, stored so the notes reopen exactly as written */
  blocks: string;
  /** The same notes as Markdown, used by the summary and by Ask */
  markdown: string;
}

/** Parse stored editor JSON; anything unreadable starts an empty editor rather than failing. */
export function parseBlocks(json: string | null | undefined): Block[] | undefined {
  if (!json) return undefined;
  try {
    const parsed = JSON.parse(json);
    return Array.isArray(parsed) && parsed.length > 0 ? (parsed as Block[]) : undefined;
  } catch {
    return undefined;
  }
}

interface NotesEditorProps {
  initialBlocks?: Block[];
  onChange: (content: NotesContent) => void;
  /** Put the cursor in the notes at once, for a meeting just created (spec F-NM-1). */
  autoFocus?: boolean;
}

/** A plain rich-text editor for the person's own notes (spec F-NOTE-1). */
export default function NotesEditor({ initialBlocks, onChange, autoFocus = false }: NotesEditorProps) {
  const editor = useCreateBlockNote({ initialContent: initialBlocks as PartialBlock[] | undefined });
  const { theme } = useTheme();

  useEffect(() => {
    if (autoFocus) editor.focus();
  }, [editor, autoFocus]);

  useEffect(() => {
    const unsubscribe = editor.onChange(async () => {
      const markdown = await editor.blocksToMarkdownLossy(editor.document);
      onChange({ blocks: JSON.stringify(editor.document), markdown });
    });
    return () => {
      if (typeof unsubscribe === 'function') unsubscribe();
    };
  }, [editor, onChange]);

  // "Add to note" from Ask appends to the end of the notes (spec F-QA-6)
  useEffect(
    () =>
      onAppendToNotes(async (markdown) => {
        const blocks = await editor.tryParseMarkdownToBlocks(markdown);
        const last = editor.document[editor.document.length - 1];
        if (last) editor.insertBlocks(blocks, last, 'after');
        else editor.replaceBlocks(editor.document, blocks);
      }),
    [editor],
  );

  return (
    <EditorKeyboardExit>
      <BlockNoteView editor={editor} theme={theme} />
    </EditorKeyboardExit>
  );
}
