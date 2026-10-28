'use client';

import type { Editor, JSONContent } from '@tiptap/core';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import {
  Bold,
  Code,
  Code2,
  Heading2,
  Italic,
  Link2,
  List,
  ListOrdered,
  Quote,
  Redo2,
  Strikethrough,
  Undo2,
} from 'lucide-react';
import { useTranslations } from 'next-intl';
import { richExtensions, safeHref, toDoc } from '../../lib/rich';
import { AiWriting } from './ai-writing';

/** Formatting buttons for an editor. Pure chrome: the editor itself decides what is allowed. */
export function Toolbar({
  editor,
  history,
}: {
  editor: Editor;
  history: boolean;
}) {
  const t = useTranslations('rich');
  const active = useEditorState({
    editor,
    selector: (s) => ({
      bold: s.editor.isActive('bold'),
      italic: s.editor.isActive('italic'),
      strike: s.editor.isActive('strike'),
      code: s.editor.isActive('code'),
      h2: s.editor.isActive('heading', { level: 2 }),
      bullet: s.editor.isActive('bulletList'),
      ordered: s.editor.isActive('orderedList'),
      quote: s.editor.isActive('blockquote'),
      codeBlock: s.editor.isActive('codeBlock'),
      link: s.editor.isActive('link'),
    }),
  });
  const chain = () => editor.chain().focus();
  const items: {
    key: string;
    label: string;
    icon: React.ReactNode;
    on: boolean;
    run: () => void;
  }[] = [
    {
      key: 'bold',
      label: t('bold'),
      icon: <Bold size={14} />,
      on: active.bold,
      run: () => chain().toggleBold().run(),
    },
    {
      key: 'italic',
      label: t('italic'),
      icon: <Italic size={14} />,
      on: active.italic,
      run: () => chain().toggleItalic().run(),
    },
    {
      key: 'strike',
      label: t('strike'),
      icon: <Strikethrough size={14} />,
      on: active.strike,
      run: () => chain().toggleStrike().run(),
    },
    {
      key: 'code',
      label: t('code'),
      icon: <Code size={14} />,
      on: active.code,
      run: () => chain().toggleCode().run(),
    },
    {
      key: 'h2',
      label: t('heading'),
      icon: <Heading2 size={14} />,
      on: active.h2,
      run: () => chain().toggleHeading({ level: 2 }).run(),
    },
    {
      key: 'bullet',
      label: t('bulletList'),
      icon: <List size={14} />,
      on: active.bullet,
      run: () => chain().toggleBulletList().run(),
    },
    {
      key: 'ordered',
      label: t('orderedList'),
      icon: <ListOrdered size={14} />,
      on: active.ordered,
      run: () => chain().toggleOrderedList().run(),
    },
    {
      key: 'quote',
      label: t('quote'),
      icon: <Quote size={14} />,
      on: active.quote,
      run: () => chain().toggleBlockquote().run(),
    },
    {
      key: 'codeBlock',
      label: t('codeBlock'),
      icon: <Code2 size={14} />,
      on: active.codeBlock,
      run: () => chain().toggleCodeBlock().run(),
    },
    {
      key: 'link',
      label: active.link ? t('unlink') : t('link'),
      icon: <Link2 size={14} />,
      on: active.link,
      run: () => {
        if (active.link) return void chain().unsetLink().run();
        const href = safeHref(window.prompt(t('linkPrompt'), 'https://'));
        if (href) chain().extendMarkRange('link').setLink({ href }).run();
        else chain().run(); // give the focus back to the text either way
      },
    },
  ];
  return (
    // Pressing a button must not take the focus (and the selection) away from the text.
    <div
      role="toolbar"
      aria-label={t('toolbar')}
      tabIndex={-1}
      onMouseDown={(e) => e.preventDefault()}
      className="flex flex-wrap gap-0.5 border-b border-border bg-surface-secondary p-1"
    >
      <AiWriting editor={editor} />
      {items.map((i) => (
        <ToolButton
          key={i.key}
          label={i.label}
          on={i.on}
          onRun={i.run}
          testId={`rich-${i.key}`}
        >
          {i.icon}
        </ToolButton>
      ))}
      {history && (
        <>
          <ToolButton
            label={t('undo')}
            on={false}
            onRun={() => editor.chain().focus().undo().run()}
          >
            <Undo2 size={14} />
          </ToolButton>
          <ToolButton
            label={t('redo')}
            on={false}
            onRun={() => editor.chain().focus().redo().run()}
          >
            <Redo2 size={14} />
          </ToolButton>
        </>
      )}
    </div>
  );
}

/**
 * A plain button: a UI-kit button moves the focus to itself on press, and the next keystroke would
 * then go nowhere instead of into the text. `mousedown` is cancelled so the editor keeps focus.
 */
function ToolButton({
  label,
  on,
  onRun,
  testId,
  children,
}: {
  label: string;
  on: boolean;
  onRun: () => void;
  testId?: string;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      aria-pressed={on}
      data-testid={testId}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onRun}
      className={`flex size-8 items-center justify-center rounded-sm text-muted hover:bg-default-hover ${on ? 'bg-accent-soft text-accent' : ''}`}
    >
      {children}
    </button>
  );
}

export const editorClass =
  'min-h-24 max-h-[60vh] overflow-y-auto px-3 py-2 text-sm focus:outline-none [&_.ProseMirror]:min-h-20 [&_.ProseMirror]:outline-none [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:font-semibold [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:list-decimal [&_ol]:pl-6 [&_blockquote]:border-l-4 [&_blockquote]:pl-3 [&_pre]:rounded [&_pre]:bg-surface-tertiary [&_pre]:p-2 [&_a]:text-accent [&_a]:underline';

/**
 * A rich-text input for values that are saved as a whole (comments, new issues). `onChange` gets
 * the document on every edit. For live shared editing of a saved field see `collab-field.tsx`.
 */
export function RichEditor({
  label,
  value,
  onChange,
  testId,
}: {
  label: string;
  value: unknown;
  onChange: (doc: JSONContent) => void;
  testId?: string;
}) {
  const editor = useEditor({
    extensions: richExtensions(false),
    content: toDoc(value),
    immediatelyRender: false, // the page is rendered on the server first
    editorProps: {
      attributes: {
        'aria-label': label,
        role: 'textbox',
        'aria-multiline': 'true',
      },
    },
    onUpdate: ({ editor: e }) => onChange(e.getJSON()),
  });
  if (!editor)
    return (
      <div
        className="min-h-24 rounded-sm border border-field-border"
        aria-busy
      />
    );
  return (
    <div
      className="overflow-hidden rounded-sm border border-field-border bg-surface focus-within:border-accent"
      data-testid={testId}
    >
      <Toolbar editor={editor} history />
      <EditorContent editor={editor} className={editorClass} />
    </div>
  );
}
