import { commitTitle, shortSha } from './commit-format';

describe('shortSha', () => {
  it('keeps the first 7 characters', () => {
    expect(shortSha('abcdef1234567')).toBe('abcdef1');
  });
});

describe('commitTitle', () => {
  it('keeps the first line, trimmed', () => {
    expect(commitTitle('Fix the diff  \n\nLonger explanation')).toBe('Fix the diff');
  });
});
