import { Component, inject, input, OnInit, signal } from '@angular/core';
import { Skeleton } from '@masmarino/gabarit';
import { LanguageStat, RepositoriesService } from '../repositories.service';

const LANGUAGE_COLORS = ['#dea584', '#f1e05a', '#3572a5', '#e34c26', '#563d7c', '#00add8', '#701516'];

function hashString(value: string): number {
  let hash = 0;
  for (let i = 0; i < value.length; i++) {
    hash = (hash * 31 + value.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

/** Fixed colours for every language name the backend can send, plus `"Other"`. Hashing into 7 colours would collide with up to 17 names, so the hash is only a fallback. */
const LANGUAGE_COLOR_MAP: Record<string, string> = {
  Rust: '#dea584',
  TypeScript: '#3178c6',
  JavaScript: '#f1e05a',
  HTML: '#e34c26',
  CSS: '#563d7c',
  Python: '#3572a5',
  Go: '#00add8',
  Java: '#b07219',
  C: '#555555',
  'C++': '#f34b7d',
  Ruby: '#701516',
  Shell: '#89e051',
  SQL: '#e38c00',
  Markdown: '#083fa1',
  JSON: '#292929',
  YAML: '#cb171e',
  Other: '#8b8b8b',
};

export function colorForLanguage(name: string): string {
  return LANGUAGE_COLOR_MAP[name] ?? LANGUAGE_COLORS[hashString(name) % LANGUAGE_COLORS.length];
}

const PERCENT_FORMAT = new Intl.NumberFormat('fr-FR', { minimumFractionDigits: 1, maximumFractionDigits: 1 });

function formatPercent(value: number): string {
  return `${PERCENT_FORMAT.format(value)}\u00a0%`;
}

@Component({
  selector: 'fg-language-bar',
  standalone: true,
  imports: [Skeleton],
  templateUrl: './language-bar.html',
  styleUrl: './language-bar.scss',
})
export class LanguageBar implements OnInit {
  repositoryId = input.required<string>();
  ref = input.required<string>();

  private repositories = inject(RepositoriesService);
  /** `null` while loading. A failed request shows as "no languages". */
  protected languages = signal<LanguageStat[] | null>(null);

  protected colorFor = colorForLanguage;
  protected formatPercent = formatPercent;

  ngOnInit(): void {
    this.repositories.getLanguages(this.repositoryId(), this.ref()).subscribe({
      next: (response) => this.languages.set(response.languages ?? []),
      error: () => this.languages.set([]),
    });
  }
}
