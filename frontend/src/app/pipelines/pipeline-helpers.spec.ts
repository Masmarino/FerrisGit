import { JobSummary } from './pipelines.service';
import { activeStageIndex, durationLabel, formatDuration, groupByStage, isNearBottom } from './pipeline-helpers';

const job = (id: string, stage: string, status: JobSummary['status']): JobSummary => ({
  id,
  stage,
  name: id,
  status,
  needs: [],
  tags: [],
  logs: '',
  createdAt: '2026-01-01T00:00:00Z',
  startedAt: null,
  finishedAt: null,
});

describe('groupByStage', () => {
  it('groups jobs by stage in order of first appearance', () => {
    const groups = groupByStage([job('a', 'prepare', 'success'), job('b', 'check', 'running'), job('c', 'check', 'pending')]);
    expect(groups.map((g) => g.name)).toEqual(['prepare', 'check']);
    expect(groups[1].jobs.map((j) => j.id)).toEqual(['b', 'c']);
  });

  it('returns no group for no jobs', () => {
    expect(groupByStage([])).toEqual([]);
  });
});

describe('activeStageIndex', () => {
  it('is the first stage that is not fully successful', () => {
    const groups = groupByStage([job('a', 'p', 'success'), job('b', 'c', 'running'), job('d', 'r', 'pending')]);
    expect(activeStageIndex(groups)).toBe(1);
  });

  it('is past the last stage when everything succeeded', () => {
    expect(activeStageIndex(groupByStage([job('a', 'p', 'success'), job('b', 'c', 'success')]))).toBe(2);
  });
});

describe('formatDuration', () => {
  it.each([
    [0, '0s'],
    [45_000, '45s'],
    [192_000, '3m 12s'],
    [3_900_000, '1h 5m'],
    [-5_000, '0s'],
  ])('formats %i ms as %s', (ms, expected) => {
    expect(formatDuration(ms)).toBe(expected);
  });
});

describe('durationLabel', () => {
  const now = Date.parse('2026-01-01T00:01:00Z');

  it('is a dash when the start is unknown', () => {
    expect(durationLabel(null, null, now)).toBe('—');
  });

  it('measures start to finish when finished', () => {
    expect(durationLabel('2026-01-01T00:00:00Z', '2026-01-01T00:00:45Z', now)).toBe('45s');
  });

  it('measures start to now while unfinished', () => {
    expect(durationLabel('2026-01-01T00:00:00Z', null, now)).toBe('1m 0s');
  });
});

describe('isNearBottom', () => {
  it('is true within the threshold of the bottom', () => {
    expect(isNearBottom(1000, 890, 100)).toBe(true);
  });

  it('is false once scrolled up', () => {
    expect(isNearBottom(1000, 100, 100)).toBe(false);
  });
});
