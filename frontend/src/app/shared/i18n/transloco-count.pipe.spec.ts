import { TranslocoCountPipe } from './transloco-count.pipe';

describe('TranslocoCountPipe', () => {
  const pipe = new TranslocoCountPipe();

  it('picks the singular or the plural by the count', () => {
    expect(pipe.transform('common.comments', 0)).toBe('0 commentaire');
    expect(pipe.transform('common.comments', 1)).toBe('1 commentaire');
    expect(pipe.transform('common.comments', 3)).toBe('3 commentaires');
  });
});
