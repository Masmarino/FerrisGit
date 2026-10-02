import { Component, computed, inject, input } from '@angular/core';
import { DomSanitizer, SafeHtml } from '@angular/platform-browser';
// Only the languages below (see EXTENSION_TO_LANGUAGE) go on highlight.js core; the full package would add about 170
// unused ones to the repositories chunk.
import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';
import css from 'highlight.js/lib/languages/css';
import dockerfile from 'highlight.js/lib/languages/dockerfile';
import go from 'highlight.js/lib/languages/go';
import ini from 'highlight.js/lib/languages/ini';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';
import python from 'highlight.js/lib/languages/python';
import ruby from 'highlight.js/lib/languages/ruby';
import rust from 'highlight.js/lib/languages/rust';
import scss from 'highlight.js/lib/languages/scss';
import sql from 'highlight.js/lib/languages/sql';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';
import yaml from 'highlight.js/lib/languages/yaml';
import DOMPurify from 'dompurify';

hljs.registerLanguage('bash', bash);
hljs.registerLanguage('c', c);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('css', css);
hljs.registerLanguage('dockerfile', dockerfile);
hljs.registerLanguage('go', go);
hljs.registerLanguage('ini', ini);
hljs.registerLanguage('java', java);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('json', json);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('python', python);
hljs.registerLanguage('ruby', ruby);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('scss', scss);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('yaml', yaml);

const EXTENSION_TO_LANGUAGE: Record<string, string> = {
  rs: 'rust',
  ts: 'typescript',
  tsx: 'typescript',
  js: 'javascript',
  jsx: 'javascript',
  html: 'xml',
  scss: 'scss',
  css: 'css',
  json: 'json',
  md: 'markdown',
  sh: 'bash',
  bash: 'bash',
  yml: 'yaml',
  yaml: 'yaml',
  toml: 'ini',
  sql: 'sql',
  py: 'python',
  rb: 'ruby',
  go: 'go',
  java: 'java',
  c: 'c',
  h: 'c',
  cpp: 'cpp',
  hpp: 'cpp',
  dockerfile: 'dockerfile',
};

/** A highlight.js language from a file extension; `null` falls back to auto-detection. */
export function languageForFileName(fileName: string): string | null {
  const baseName = fileName.split('/').pop() ?? fileName;
  const dotIndex = baseName.lastIndexOf('.');
  const extension = (dotIndex === -1 ? baseName : baseName.slice(dotIndex + 1)).toLowerCase();
  return EXTENSION_TO_LANGUAGE[extension] ?? null;
}

/** Lines a file shows: a trailing newline ends the last line instead of starting a new one, and an empty file has one. */
export function lineCount(content: string): number {
  if (content === '') return 1;
  const breaks = content.split('\n').length - 1;
  return content.endsWith('\n') ? breaks : breaks + 1;
}

/** Highlighted source code. The optional line-number gutter is hidden from screen readers and from copied selections. */
@Component({
  selector: 'fg-code-view',
  standalone: true,
  template: `
    @if (lineNumbers()) {
      <div class="code-view code-view--numbered">
        <pre class="code-view__line-numbers" aria-hidden="true">{{ lineNumberText() }}</pre>
        <!-- Focusable so a keyboard user can scroll long lines sideways. -->
        <pre class="code-view__code" tabindex="0"><code class="hljs" [innerHTML]="renderedHtml()"></code></pre>
      </div>
    } @else {
      <pre class="code-view"><code class="hljs" [innerHTML]="renderedHtml()"></code></pre>
    }
  `,
  styleUrl: './code-view.scss',
})
export class CodeView {
  content = input.required<string>();
  fileName = input.required<string>();
  lineNumbers = input(false);

  private sanitizer = inject(DomSanitizer);

  protected lineNumberText = computed(() => Array.from({ length: lineCount(this.content()) }, (_, i) => i + 1).join('\n'));

  protected renderedHtml = computed<SafeHtml>(() => {
    const language = languageForFileName(this.fileName());
    const result = language && hljs.getLanguage(language) ? hljs.highlight(this.content(), { language }) : hljs.highlightAuto(this.content());
    return this.sanitizer.bypassSecurityTrustHtml(DOMPurify.sanitize(result.value));
  });
}
