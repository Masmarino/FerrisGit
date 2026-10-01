/** The API cuts `notesExcerpt` at this many characters (Unicode scalar values, trimmed). */
const NOTES_EXCERPT_LENGTH = 240;

/**
 * The notes excerpt as plain text on one line: a light strip of markdown syntax (`##`, `**`, list markers, link
 * targets), not a parser. Safe because the text is interpolated, never rendered as HTML.
 */
export function plainExcerpt(markdown: string): string {
  const text = markdown
    // Fenced code markers (the code itself stays), HTML tags, images, then links, which keep their text.
    .replace(/^\s*(```|~~~).*$/gm, ' ')
    .replace(/<[^>]*>/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    // Line-leading syntax: headings, quotes, bullets (and task boxes), numbered items, horizontal rules.
    .replace(/^\s*#{1,6}\s+/gm, '')
    .replace(/^\s*>\s?/gm, '')
    .replace(/^\s*[-*+]\s+(\[[ xX]\]\s+)?/gm, '')
    .replace(/^\s*\d+[.)]\s+/gm, '')
    .replace(/^\s*([-*_]\s*){3,}$/gm, ' ')
    // Emphasis and inline code markers, only where they open or close a span (not inside a word).
    .replace(/(^|[\s(])(\*{1,3}|_{1,3}|`+|~~)(?=\S)/g, '$1')
    .replace(/(\S)(\*{1,3}|_{1,3}|`+|~~)(?=$|[\s.,;:!?)])/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
  if (!text) {
    return '';
  }
  return [...markdown].length >= NOTES_EXCERPT_LENGTH ? `${text}…` : text;
}
