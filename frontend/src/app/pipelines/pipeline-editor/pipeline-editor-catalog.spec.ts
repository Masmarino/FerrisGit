import { pipelineTemplates, tileById } from './pipeline-catalog';
import { FILE_URL, PROFILE_URL, Role, answerRender, buttonNamed, cardNames, fileBody, lanes, opened, settle, setup, stageNames, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor catalogue', () => {
  verifyRequestsAfterEach();

  describe('adding jobs from tiles', () => {
    it('opens the catalogue for the stage whose button was clicked', async () => {
      const ctx = opened();
      await answerRender(ctx);

      Array.from(lanes(ctx.el)[0].querySelectorAll('button')).find((b) => text(b) === 'Ajouter un job')!.click();
      ctx.fixture.detectChanges();

      expect(ctx.internals.pickerStage()).toBe('build');
      expect(text(ctx.el.ownerDocument.querySelector('fg-pipeline-tile-picker .tile-picker__intro'))).toContain('étape build');
    });

    it('adds the chosen tile at the end of the stage, opens it, and says so', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');

      ctx.internals.chooseTile(tileById('go-test'));
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'test']);
      expect(ctx.internals.selected()).toBe('test');
      expect(ctx.internals.pickerStage()).toBeNull();
      expect(ctx.internals.announcement()).toBe("Job test ajouté à l'étape test");
      await answerRender(ctx);
    });
  });

  describe('adding a tile with questions', () => {
    it('builds the job from the answers', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');

      ctx.internals.chooseTile(tileById('k8s-image'), { namespace: 'prod', deployment: 'web', mode: 'restart' });
      ctx.fixture.detectChanges();

      const job = ctx.internals.state().jobs.find((j) => j.name === 'update-k8s') as unknown as { stage: string; image: string; script: string[] };
      expect(job.stage).toBe('test');
      expect(job.image).toBe('alpine:3.20');
      expect(job.script).toContain('kubectl rollout restart deployment/web -n prod');
      await answerRender(ctx);
    });
  });

  describe('starting from a template', () => {
    async function emptyRepository() {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null));
      ctx.fixture.detectChanges();
      return ctx;
    }

    it('offers the templates while the pipeline has no job', async () => {
      const ctx = await emptyRepository();
      await answerRender(ctx);

      expect(ctx.el.querySelectorAll('.starters__card')).toHaveLength(pipelineTemplates().length);
    });

    it('lays the template out as stages and jobs, and stops offering templates', async () => {
      const ctx = await emptyRepository();
      await answerRender(ctx);

      ctx.internals.chooseTemplate(pipelineTemplates().find((t) => t.id === 'rust'));
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['check', 'test']);
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['format', 'clippy']);
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['test']);
      expect(ctx.el.querySelector('.starters__card')).toBeNull();
      await answerRender(ctx);
    });

    it('does not offer templates over a pipeline that has jobs', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(ctx.el.querySelector('fg-pipeline-starters')).toBeNull();
    });
  });

  describe('help', () => {
    it('keeps every help bubble of the board within reach of a screen reader, not inside a hidden part', async () => {
      const ctx = opened();
      await answerRender(ctx);

      const bubbles = Array.from(ctx.el.querySelectorAll('.pipeline-editor__board fg-help-tip button'));
      expect(bubbles.length).toBeGreaterThan(0);
      expect(bubbles.filter((button) => button.closest('[aria-hidden="true"]'))).toEqual([]);
    });

    it('explains the pipeline, each stage and the new-stage form in bubbles', async () => {
      const ctx = opened();
      await answerRender(ctx);

      const labels = Array.from(ctx.el.querySelectorAll('fg-help-tip button')).map((b) => b.getAttribute('aria-label'));

      expect(labels).toEqual(expect.arrayContaining(['Aide : Comment ça marche', 'Aide : Une étape', 'Aide : Ajouter une étape']));
      expect(labels.filter((label) => label === 'Aide : Une étape')).toHaveLength(2);
    });

    it('explains the YAML instead when the YAML is shown', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.setMode('yaml');
      ctx.fixture.detectChanges();

      expect(Array.from(ctx.el.querySelectorAll('fg-help-tip button')).map((b) => b.getAttribute('aria-label'))).toContain('Aide : Le YAML');
    });
  });

  describe('a pipeline made for the repository', () => {
    /** A Rust workspace with an Angular app in `web`, as the server would read it. */
    const PROFILE = {
      projects: [
        { kind: 'rust', dir: '', evidence: ['Cargo.toml'], workspace: true, toolchain: '1.86', sqlxOffline: false, sqlxPostgres: false },
        { kind: 'node', dir: 'web', evidence: ['web/package.json', 'web/package-lock.json'], packageManager: 'npm', nodeVersion: '22', scripts: { test: 'ng test', build: 'ng build' }, framework: 'angular', testRunner: 'vitest' },
      ],
      dockerfiles: [''],
      helmCharts: [],
    };

    async function emptyRepository(role: Role = 'contributor') {
      const ctx = setup(role);
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null));
      ctx.fixture.detectChanges();
      return ctx;
    }

    it('reads the repository once the pipeline is empty, and proposes what it found first', async () => {
      const ctx = await emptyRepository();
      expect(text(ctx.el.querySelector('fg-pipeline-starters [role="status"]'))).toBe('Lecture du dépôt');

      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      ctx.fixture.detectChanges();

      const proposal = ctx.el.querySelector('[data-prediction]')!;
      expect(text(proposal.querySelector('h2'))).toBe('Pour ce dépôt : Rust et Angular');
      expect(text(proposal)).toContain("D'après les fichiers de la branche main");
      expect(Array.from(proposal.querySelectorAll('.starters__evidence code'), text)).toEqual(['Cargo.toml', 'web/package.json', 'web/package-lock.json']);
      expect(Array.from(proposal.querySelectorAll('.starters__stage'), text)).toEqual(['check', 'test', 'build']);
      expect(text(proposal.querySelector('.starters__notes'))).toContain('Un Dockerfile à la racine');
      expect(text(ctx.el.querySelector('#starters-title'))).toBe("Ou partir d'un modèle");
      await answerRender(ctx);
    });

    it('lays the proposal out on the board, and undoing takes it back', async () => {
      const ctx = await emptyRepository();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      await answerRender(ctx);
      ctx.fixture.detectChanges();

      buttonNamed(ctx.el, 'Utiliser cette pipeline').click();
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['check', 'test', 'build']);
      expect(cardNames(lanes(ctx.el)[1])).toEqual(['rust-test', 'web-test']);
      expect(ctx.internals.state().jobs.find((j) => j.name === 'web-test')?.image).toBe('node:22');
      expect(ctx.el.querySelector('fg-pipeline-starters')).toBeNull();
      await answerRender(ctx);

      ctx.internals.undo();
      expect(ctx.internals.state().jobs).toEqual([]);
      await answerRender(ctx);
    });

    it('keeps the templates alone when the repository cannot be read or holds nothing known', async () => {
      const ctx = await emptyRepository();

      ctx.http.expectOne(PROFILE_URL).flush(null, { status: 500, statusText: 'Server Error' });
      ctx.fixture.detectChanges();

      expect(ctx.el.querySelector('[data-prediction]')).toBeNull();
      expect(text(ctx.el.querySelector('#starters-title'))).toBe("Partir d'un modèle");
      expect(ctx.el.querySelectorAll('.starters__card')).toHaveLength(pipelineTemplates().length);
      await answerRender(ctx);
    });

    it('proposes nothing with Kubernetes, and says why: a Pod gets no copy of the repository', async () => {
      const ctx = setup('contributor', 'kubernetes');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(null));
      ctx.fixture.detectChanges();

      ctx.http.expectNone(PROFILE_URL);
      expect(ctx.el.querySelector('[data-prediction]')).toBeNull();
      expect(ctx.el.querySelector('.starters__card')).toBeNull();
      expect(text(ctx.el.querySelector('#starters-title'))).toBe("Partir d'un job vide");
      expect(text(ctx.el.querySelector('fg-pipeline-starters [data-kubernetes]'))).toContain('ne reçoit pas de copie du dépôt');
      await answerRender(ctx);
    });

    it('does not read the repository for a pipeline that has jobs until a job is added', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.http.expectNone(PROFILE_URL);

      ctx.internals.pickerStage.set('test');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      ctx.fixture.detectChanges();

      const offered = Array.from(document.querySelectorAll<HTMLElement>('[data-suggestion]'), (tile) => tile.dataset['suggestion']);
      // `unit` already runs cargo test at the root: Rust's tests are not offered again.
      expect(offered).toEqual(['rust-format', 'rust-clippy', 'web-test', 'web-build']);
      expect(text(document.querySelector('[data-suggestion="web-test"]'))).toContain('npm test -- --watch=false');
    });

    it('adds a suggested job to the stage it was asked for, waiting only for what is there before it', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);
      const clippy = { name: 'rust-clippy', stage: 'check', image: 'rust:1.86', script: ['cargo clippy'], variables: [], needs: ['compile', 'rust-format'], tags: [], cache: [] };

      ctx.internals.chooseSuggested(clippy);
      ctx.fixture.detectChanges();

      expect(cardNames(lanes(ctx.el)[1])).toEqual(['unit', 'rust-clippy']);
      // compile is in an earlier stage and stays; rust-format does not exist here.
      expect(ctx.internals.state().jobs.find((j) => j.name === 'rust-clippy')).toMatchObject({ stage: 'test', needs: ['compile'] });
      expect(ctx.internals.selected()).toBe('rust-clippy');
      expect(ctx.internals.suggestions().map((s) => s.job.name)).not.toContain('rust-clippy');
      await answerRender(ctx);
    });

    it('keeps a suggested need on a job of the same stage, which the server takes', async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.pickerStage.set('test');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(PROFILE_URL).flush(PROFILE);

      ctx.internals.chooseSuggested({ name: 'web-build', stage: 'build', image: 'node:22', script: ['npm run build'], variables: [], needs: ['unit'], tags: [], cache: [] });

      expect(ctx.internals.state().jobs.find((j) => j.name === 'web-build')).toMatchObject({ stage: 'test', needs: ['unit'] });
      await answerRender(ctx);
    });

    it('does not read the repository for a reader', async () => {
      const ctx = await emptyRepository('reader');

      ctx.http.expectNone(PROFILE_URL);
    });
  });
});
