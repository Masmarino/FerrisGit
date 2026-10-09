import { BuilderJob, BuilderState } from './pipeline-builder-model';
import { boardLanes, boardLinks } from './pipeline-board';

const job = (name: string, stage: string, extra: Partial<BuilderJob> = {}): BuilderJob => ({ name, stage, image: 'rust:1', script: ['cargo build'], variables: [], needs: [], tags: [], cache: [], ...extra });

const STATE: BuilderState = {
  stages: ['build', 'test', 'deploy'],
  jobs: [job('compile', 'build'), job('unit', 'test', { needs: ['compile'], script: ['cd crates', '', 'cargo test'] }), job('ship', 'deploy', { needs: ['unit'] })],
};

describe('pipeline board', () => {
  it('lays out a column per stage, a card per job, and what a column can do', () => {
    const lanes = boardLanes(STATE, [], null);

    expect(lanes.map((lane) => [lane.stage, lane.cards.map((card) => card.job.name), lane.canMoveBefore, lane.canMoveAfter, lane.removable])).toEqual([
      ['build', ['compile'], false, true, false],
      ['test', ['unit'], true, true, false],
      ['deploy', ['ship'], true, false, false],
    ]);
    expect(lanes[1].cards[0]).toMatchObject({ mainCommand: 'cargo test', setupCommands: 1, moveTargets: ['build', 'deploy'], menuLabel: 'Actions du job unit' });
  });

  it('counts the problems of each job, and only those that name one', () => {
    const lanes = boardLanes(STATE, ['unit', 'unit', null, undefined], null);

    expect(lanes.map((lane) => lane.cards[0].problemCount)).toEqual([0, 2, 0]);
  });

  it('says how each card is tied to the pointed job', () => {
    const lanes = boardLanes(STATE, [], 'unit');

    expect(lanes.map((lane) => lane.cards[0].relation)).toEqual(['waited', null, 'waiting']);
  });

  it('draws a tie per need, bringing out those of the pointed job and marking those the server refuses', () => {
    const moved: BuilderState = { ...STATE, jobs: [...STATE.jobs.slice(0, 2), job('ship', 'build', { needs: ['unit', 'gone'] })] };

    expect(boardLinks(STATE, 'compile')).toEqual([
      { from: 'compile', to: 'unit', highlighted: true, invalid: false },
      { from: 'unit', to: 'ship', highlighted: false, invalid: false },
    ]);
    expect(boardLinks(moved, null)).toContainEqual({ from: 'unit', to: 'ship', highlighted: false, invalid: true });
    expect(boardLinks(moved, null).some((link) => link.from === 'gone')).toBe(false);

    const sameStage: BuilderState = { ...STATE, jobs: [job('compile', 'build'), job('lint', 'build', { needs: ['compile'] })] };
    expect(boardLinks(sameStage, null)).toEqual([{ from: 'compile', to: 'lint', highlighted: false, invalid: false }]);
  });
});
