import { TestBed } from '@angular/core/testing';
import { CodeView, languageForFileName } from './code-view';

describe('languageForFileName', () => {
  it('maps common extensions to their highlight.js language name', () => {
    expect(languageForFileName('main.rs')).toBe('rust');
    expect(languageForFileName('app.component.ts')).toBe('typescript');
    expect(languageForFileName('src/lib/foo.py')).toBe('python');
    expect(languageForFileName('docs/README.md')).toBe('markdown');
  });

  it('returns null for an unrecognized extension', () => {
    expect(languageForFileName('data.xyz')).toBeNull();
  });

  it('returns null for a file with no extension', () => {
    expect(languageForFileName('Makefile')).toBeNull();
  });
});

describe('CodeView', () => {
  function setup(content: string, fileName: string) {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(CodeView);
    fixture.componentRef.setInput('content', content);
    fixture.componentRef.setInput('fileName', fileName);
    return { fixture };
  }

  it('renders the code inside a <pre><code>', () => {
    const { fixture } = setup('fn main() {}', 'main.rs');
    fixture.detectChanges();

    const code = fixture.nativeElement.querySelector('pre code');
    expect(code).toBeTruthy();
    expect(code.textContent).toContain('fn main()');
  });

  it('still renders content for a file type highlight.js does not recognize', () => {
    const { fixture } = setup('some raw content', 'data.xyz');
    fixture.detectChanges();

    const code = fixture.nativeElement.querySelector('pre code');
    expect(code.textContent).toContain('some raw content');
  });

  it('renders no line-number gutter by default', () => {
    const { fixture } = setup('a\nb\nc\n', 'notes.txt');
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.code-view__line-numbers')).toBeNull();
  });

  describe('with lineNumbers', () => {
    function numbers(content: string): string[] {
      const { fixture } = setup(content, 'main.rs');
      fixture.componentRef.setInput('lineNumbers', true);
      fixture.detectChanges();
      const gutter = fixture.nativeElement.querySelector('.code-view__line-numbers') as HTMLElement | null;
      expect(gutter).toBeTruthy();
      return (gutter!.textContent ?? '').split('\n');
    }

    it('numbers every line of the file, 1 to N', () => {
      expect(numbers('fn main() {\n    println!("hi");\n}')).toEqual(['1', '2', '3']);
    });

    it('does not number the empty "line" after a trailing newline', () => {
      expect(numbers('a\nb\nc\n')).toEqual(['1', '2', '3']);
    });

    it('counts Windows line endings once per line', () => {
      expect(numbers('a\r\nb\r\n')).toEqual(['1', '2']);
    });

    it('numbers a single line for an empty file', () => {
      expect(numbers('')).toEqual(['1']);
    });

    it('keeps the numbers out of the accessibility tree and still renders the code', () => {
      const { fixture } = setup('fn main() {}\n', 'main.rs');
      fixture.componentRef.setInput('lineNumbers', true);
      fixture.detectChanges();

      const el = fixture.nativeElement as HTMLElement;
      expect(el.querySelector('.code-view__line-numbers')?.getAttribute('aria-hidden')).toBe('true');
      expect(el.querySelector('pre code')?.textContent).toContain('fn main()');
    });
  });
});
