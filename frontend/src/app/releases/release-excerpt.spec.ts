import { plainExcerpt } from './release-excerpt';

describe('plainExcerpt', () => {
  it('returns an empty string for empty or blank notes', () => {
    expect(plainExcerpt('')).toBe('');
    expect(plainExcerpt('  \n\n ')).toBe('');
  });

  it('drops the markdown syntax and keeps the words, on one line', () => {
    const notes = '## Nouveautés\n\n- **Rapide** : un `cache` _partagé_\n- Voir [la doc](https://example.com/docs)\n\n> Merci à tous';
    expect(plainExcerpt(notes)).toBe('Nouveautés Rapide : un cache partagé Voir la doc Merci à tous');
  });

  it('drops images, html tags, code fences and numbered list markers', () => {
    const notes = '![capture](shot.png)\n<details>Détails</details>\n```bash\ncargo build\n```\n1. Premier\n2) Second\n---';
    expect(plainExcerpt(notes)).toBe('Détails cargo build Premier Second');
  });

  it('drops task list boxes', () => {
    expect(plainExcerpt('- [ ] Notes à rédiger\n- [x] Tag posé')).toBe('Notes à rédiger Tag posé');
  });

  it('keeps underscores and asterisks inside words', () => {
    expect(plainExcerpt('Variable FERRISGIT_DATA_DIR et 2*3')).toBe('Variable FERRISGIT_DATA_DIR et 2*3');
  });

  it('ends with an ellipsis when the API cut the notes (240 characters)', () => {
    const cut = 'a'.repeat(240);
    expect(plainExcerpt(cut)).toBe(`${cut}…`);
    expect(plainExcerpt('a'.repeat(239))).toBe('a'.repeat(239));
  });
});
