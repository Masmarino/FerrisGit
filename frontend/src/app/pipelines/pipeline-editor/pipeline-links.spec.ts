import { linkPath } from './pipeline-links';

describe('linkPath', () => {
  it("draws the pipeline page's curve, from the right edge of one job to the left edge of the next", () => {
    const from = { left: 0, top: 0, width: 100, height: 40 };
    const to = { left: 200, top: 100, width: 100, height: 40 };

    expect(linkPath(from, to)).toBe('M 100 20 C 150 20, 150 120, 200 120');
  });

  it('keeps a curve readable between close stages', () => {
    const path = linkPath({ left: 0, top: 0, width: 100, height: 40 }, { left: 110, top: 0, width: 100, height: 40 });

    expect(path).toBe('M 100 20 C 124 20, 86 20, 110 20');
  });
});
