import StarterKit from '@tiptap/starter-kit';
import { Extension, type Extensions, type JSONContent } from '@tiptap/core';
import { Markdown } from '@tiptap/markdown';
import { Plugin } from '@tiptap/pm/state';

/** Links the server accepts in documents: http(s) and mailto, nothing else. */
export function safeHref(href: unknown): string | null {
  if (typeof href !== 'string') return null;
  const h = href.trim();
  return h.length <= 2048 &&
    /^(https?:\/\/|mailto:)/i.test(h) &&
    ![...h].some((c) => c.charCodeAt(0) < 32)
    ? h
    : null;
}

const MARKDOWN_HINT =
  /^(#{1,6} |[-*+] |\d+[.)] |> |```)|\*\*[^*\n]+\*\*|`[^`\n]+`|~~[^~\n]+~~|\[[^\]\n]+\]\(https?:/m;

/** Plain text that is most likely Markdown: headings, lists, quotes, fences, emphasis, links. */
export function looksLikeMarkdown(text: string): boolean {
  return MARKDOWN_HINT.test(text);
}

/** Drops link marks whose address the server would refuse, so a pasted `[x](javascript:…)` is text. */
export function dropUnsafeLinks(node: JSONContent): JSONContent {
  return {
    ...node,
    ...(node.marks && {
      marks: node.marks.filter(
        (m) => m.type !== 'link' || safeHref(m.attrs?.href) !== null,
      ),
    }),
    ...(node.content && { content: node.content.map(dropUnsafeLinks) }),
  };
}

/** Pasting Markdown text (no HTML on the clipboard) inserts it as formatted content. */
const MarkdownPaste = Extension.create({
  name: 'markdownPaste',
  addProseMirrorPlugins() {
    const editor = this.editor;
    return [
      new Plugin({
        props: {
          handlePaste(_view, event) {
            const data = event.clipboardData;
            const text = data?.getData('text/plain') ?? '';
            if (!text || data?.getData('text/html')) return false;
            if (editor.isActive('codeBlock') || !looksLikeMarkdown(text))
              return false;
            const doc = dropUnsafeLinks(editor.markdown?.parse(text) ?? {});
            return doc.content
              ? editor.commands.insertContent(doc.content)
              : false;
          },
        },
      }),
    ];
  },
});

/**
 * The editor's schema. It is the single place that decides which nodes and marks exist, and it
 * matches what the backend accepts (`rich.rs`): anything else would be refused on save.
 * `collab` turns off the editor's own undo history, which a shared document must not have.
 */
export function richExtensions(collab = false): Extensions {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
      link: {
        openOnClick: false,
        autolink: true,
        protocols: [],
        isAllowedUri: (url) => safeHref(url) !== null,
        HTMLAttributes: {
          rel: 'noopener noreferrer nofollow',
          target: '_blank',
        },
      },
      ...(collab ? { undoRedo: false as const } : {}),
    }),
    Markdown,
    MarkdownPaste,
  ];
}

export type Doc = JSONContent & { type: 'doc' };

export const EMPTY_DOC: Doc = { type: 'doc', content: [{ type: 'paragraph' }] };

/** Stored value -> document: plain text from before the editor becomes paragraphs. */
export function toDoc(value: unknown): Doc {
  if (
    value &&
    typeof value === 'object' &&
    (value as { type?: unknown }).type === 'doc'
  )
    return value as Doc;
  if (typeof value === 'string' && value.length > 0) {
    return {
      type: 'doc',
      content: value
        .split(/\r?\n/)
        .map((line) =>
          line
            ? { type: 'paragraph', content: [{ type: 'text', text: line }] }
            : { type: 'paragraph' },
        ),
    };
  }
  return EMPTY_DOC;
}

export function plainText(node: unknown): string {
  if (typeof node === 'string') return node;
  if (!node || typeof node !== 'object') return '';
  const n = node as JSONContent;
  const own = typeof n.text === 'string' ? n.text : '';
  const kids = (n.content ?? []).map(plainText);
  const block = [
    'paragraph',
    'heading',
    'listItem',
    'blockquote',
    'codeBlock',
  ].includes(n.type ?? '');
  return own + kids.join(block ? '' : '') + (block && kids.length ? '\n' : '');
}

export function isEmptyDoc(value: unknown): boolean {
  return plainText(toDoc(value)).trim() === '';
}
