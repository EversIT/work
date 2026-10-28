'use client';

import type { Editor, JSONContent } from '@tiptap/core';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import {
  Bold,
  Code,
  Code2,
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

const LANGUAGES = [
  'plaintext',
  'bash',
  'css',
  'go',
  'html',
  'java',
  'javascript',
  'json',
  'python',
  'rust',
  'sql',
  'typescript',
  'yaml',
];

/**
 * Formatting bar of an editor, laid out like Jira's: text style, emphasis, lists, link and
 * insert actions, then undo/redo. Pure chrome: the editor itself decides what is allowed.
 */
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
      heading: [1, 2, 3].find((level) =>
        s.editor.isActive('heading', { level }),
      ),
      bullet: s.editor.isActive('bulletList'),
      ordered: s.editor.isActive('orderedList'),
      quote: s.editor.isActive('blockquote'),
      codeBlock: s.editor.isActive('codeBlock'),
      language: String(
        s.editor.getAttributes('codeBlock').language ?? 'plaintext',
      ),
      link: s.editor.isActive('link'),
    }),
  });
  const chain = () => editor.chain().focus();
  const style = active.heading ? String(active.heading) : 'p';
  const setStyle = (v: string) => {
    if (v === 'p') chain().setParagraph().run();
    else
      chain()
        .setHeading({ level: Number(v) as 1 | 2 | 3 })
        .run();
  };
  const setLink = () => {
    if (active.link) return void chain().unsetLink().run();
    const href = safeHref(window.prompt(t('linkPrompt'), 'https://'));
    if (href) chain().extendMarkRange('link').setLink({ href }).run();
    else chain().run(); // give the focus back to the text either way
  };
  const selectClass =
    'h-8 rounded-sm bg-transparent px-2 text-sm text-foreground hover:bg-default-hover focus:outline-none';
  return (
    // Pressing a button must not take the focus (and the selection) away from the text;
    // a select needs the mouse event to open.
    <div
      role="toolbar"
      aria-label={t('toolbar')}
      tabIndex={-1}
      onMouseDown={(e) => {
        if (!(e.target instanceof HTMLSelectElement)) e.preventDefault();
      }}
      className="flex flex-wrap items-center gap-0.5 border-b border-border bg-surface px-2 py-1"
    >
      <AiWriting editor={editor} />
      <select
        aria-label={t('textStyle')}
        data-testid="rich-style"
        value={style}
        onChange={(e) => setStyle(e.target.value)}
        className={`${selectClass} w-32`}
      >
        <option value="p">{t('normalText')}</option>
        <option value="1">{t('heading1')}</option>
        <option value="2">{t('heading2')}</option>
        <option value="3">{t('heading3')}</option>
      </select>
      <Divider />
      <ToolButton
        label={t('bold')}
        on={active.bold}
        onRun={() => chain().toggleBold().run()}
        testId="rich-bold"
      >
        <Bold size={16} />
      </ToolButton>
      <ToolButton
        label={t('italic')}
        on={active.italic}
        onRun={() => chain().toggleItalic().run()}
        testId="rich-italic"
      >
        <Italic size={16} />
      </ToolButton>
      <ToolButton
        label={t('strike')}
        on={active.strike}
        onRun={() => chain().toggleStrike().run()}
        testId="rich-strike"
      >
        <Strikethrough size={16} />
      </ToolButton>
      <ToolButton
        label={t('code')}
        on={active.code}
        onRun={() => chain().toggleCode().run()}
        testId="rich-code"
      >
        <Code size={16} />
      </ToolButton>
      <Divider />
      <ToolButton
        label={t('bulletList')}
        on={active.bullet}
        onRun={() => chain().toggleBulletList().run()}
        testId="rich-bullet"
      >
        <List size={16} />
      </ToolButton>
      <ToolButton
        label={t('orderedList')}
        on={active.ordered}
        onRun={() => chain().toggleOrderedList().run()}
        testId="rich-ordered"
      >
        <ListOrdered size={16} />
      </ToolButton>
      <Divider />
      <ToolButton
        label={active.link ? t('unlink') : t('link')}
        on={active.link}
        onRun={setLink}
        testId="rich-link"
      >
        <Link2 size={16} />
      </ToolButton>
      <ToolButton
        label={t('quote')}
        on={active.quote}
        onRun={() => chain().toggleBlockquote().run()}
        testId="rich-quote"
      >
        <Quote size={16} />
      </ToolButton>
      <ToolButton
        label={t('codeBlock')}
        on={active.codeBlock}
        onRun={() => chain().toggleCodeBlock().run()}
        testId="rich-codeBlock"
      >
        <Code2 size={16} />
      </ToolButton>
      {active.codeBlock && (
        <select
          aria-label={t('language')}
          data-testid="rich-language"
          value={active.language}
          onChange={(e) =>
            chain()
              .updateAttributes('codeBlock', { language: e.target.value })
              .run()
          }
          className={selectClass}
        >
          {LANGUAGES.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
      )}
      {history && (
        <>
          <Divider />
          <ToolButton
            label={t('undo')}
            on={false}
            onRun={() => editor.chain().focus().undo().run()}
          >
            <Undo2 size={16} />
          </ToolButton>
          <ToolButton
            label={t('redo')}
            on={false}
            onRun={() => editor.chain().focus().redo().run()}
          >
            <Redo2 size={16} />
          </ToolButton>
        </>
      )}
    </div>
  );
}

function Divider() {
  return <span aria-hidden className="mx-1 h-5 w-px bg-border" />;
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
      className={`flex size-8 items-center justify-center rounded-sm text-muted hover:bg-default-hover hover:text-foreground ${on ? 'bg-accent-soft text-accent' : ''}`}
    >
      {children}
    </button>
  );
}

export const editorClass =
  'min-h-24 max-h-[60vh] overflow-y-auto px-3 py-2 text-sm focus:outline-none [&_.ProseMirror]:min-h-20 [&_.ProseMirror]:outline-none [&_h1]:text-xl [&_h1]:font-semibold [&_h2]:text-lg [&_h2]:font-semibold [&_h3]:font-semibold [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:list-decimal [&_ol]:pl-6 [&_blockquote]:border-l-4 [&_blockquote]:pl-3 [&_pre]:my-2 [&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:border [&_pre]:border-separator [&_pre]:bg-surface-secondary [&_pre]:p-3 [&_pre]:font-mono [&_pre]:text-xs [&_:not(pre)>code]:rounded-sm [&_:not(pre)>code]:bg-surface-tertiary [&_:not(pre)>code]:px-1 [&_:not(pre)>code]:font-mono [&_a]:text-accent [&_a]:underline';

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
