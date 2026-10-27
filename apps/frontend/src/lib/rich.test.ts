import { describe, expect, it } from 'vitest';
import {
  EMPTY_DOC,
  dropUnsafeLinks,
  isEmptyDoc,
  looksLikeMarkdown,
  plainText,
  safeHref,
  toDoc,
} from './rich';

describe('safeHref', () => {
  it('accepts http(s) and mailto only', () => {
    expect(safeHref('https://example.test/a?b=1')).toBe(
      'https://example.test/a?b=1',
    );
    expect(safeHref(' mailto:a@example.test ')).toBe('mailto:a@example.test');
    for (const bad of [
      'javascript:alert(1)',
      'JaVaScRiPt:alert(1)',
      'data:text/html,x',
      'ftp://x',
      '//evil.test',
      'https://x\u0000y',
      '',
      5,
      null,
    ]) {
      expect(safeHref(bad)).toBeNull();
    }
    expect(safeHref(`https://x.test/${'a'.repeat(3000)}`)).toBeNull();
  });
});

describe('toDoc', () => {
  it('turns plain text from before the editor into paragraphs', () => {
    expect(toDoc('one\ntwo')).toEqual({
      type: 'doc',
      content: [
        { type: 'paragraph', content: [{ type: 'text', text: 'one' }] },
        { type: 'paragraph', content: [{ type: 'text', text: 'two' }] },
      ],
    });
    expect(toDoc('a\n\nb').content).toHaveLength(3);
  });

  it('keeps documents and survives anything else', () => {
    const doc = { type: 'doc', content: [{ type: 'paragraph' }] };
    expect(toDoc(doc)).toBe(doc);
    for (const v of [null, undefined, '', 5, [], { type: 'paragraph' }])
      expect(toDoc(v)).toEqual(EMPTY_DOC);
  });
});

describe('plainText / isEmptyDoc', () => {
  it('reads the text of a document', () => {
    expect(plainText(toDoc('hello'))).toBe('hello\n');
    expect(isEmptyDoc(toDoc('  '))).toBe(true);
    expect(isEmptyDoc(null)).toBe(true);
    expect(isEmptyDoc(toDoc('x'))).toBe(false);
  });
});

describe('markdown detection', () => {
  it.each([
    '# Title',
    '- a\n- b',
    '1. one',
    '> quote',
    'some **bold** text',
    'see [docs](https://x.test)',
    '```\ncode\n```',
  ])('recognises %j', (text) => expect(looksLikeMarkdown(text)).toBe(true));
  it.each([
    'plain words',
    'a # b',
    'price is 5 - 3',
    '[x](javascript:alert(1))',
  ])('leaves %j alone', (text) => expect(looksLikeMarkdown(text)).toBe(false));
  it('removes unsafe links but keeps the text', () => {
    const doc = dropUnsafeLinks({
      type: 'text',
      text: 'x',
      marks: [
        { type: 'link', attrs: { href: 'javascript:alert(1)' } },
        { type: 'bold' },
      ],
    });
    expect(doc.marks).toEqual([{ type: 'bold' }]);
  });
});
