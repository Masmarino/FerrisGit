import { DefinitionDto } from './pipeline-definitions.service';
import { FILE, FILE_URL, PARSE_URL, RENDER_URL, answerRender, fileBody, opened, parsed, rendered, setup, nextRequest, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor secrets', () => {
  verifyRequestsAfterEach();

  describe('variables and secrets', () => {
    const SECRETS_URL = '/api/repositories/repo-1/ci-variables';

    /** A maintainer opens a pipeline whose jobs read DEPLOY_TOKEN and API_KEY; the repository has API_KEY and UNUSED. */
    async function asMaintainer() {
      const ctx = setup('maintainer');
      ctx.fixture.detectChanges();
      ctx.http.expectOne(FILE_URL).flush(fileBody(FILE));
      const withSecrets: DefinitionDto = {
        stages: ['build', 'test'],
        jobs: {
          compile: { stage: 'build', image: 'rust:1', script: ['curl $API_KEY'], variables: {}, needs: [], tags: [], cache: [] },
          unit: { stage: 'test', image: 'rust:1', script: ['curl "$DEPLOY_TOKEN" $API_KEY'], variables: {}, needs: [], tags: [], cache: [] },
        },
      };
      ctx.http.expectOne(PARSE_URL).flush(parsed({ definition: withSecrets }));
      ctx.fixture.detectChanges();
      ctx.http.expectOne(SECRETS_URL).flush([{ id: '1', key: 'API_KEY', masked: true }, { id: '2', key: 'UNUSED', masked: true }]);
      ctx.fixture.detectChanges();
      await answerRender(ctx);
      return ctx;
    }

    it('loads the secrets of the repository for a maintainer', async () => {
      const ctx = await asMaintainer();

      expect(ctx.internals.secrets()).toEqual(['API_KEY', 'UNUSED']);
    });

    it('does not ask for them when the person is only a contributor', async () => {
      const ctx = opened();
      await answerRender(ctx);

      expect(ctx.internals.secrets()).toBeNull();
    });

    it('knows which secrets the jobs read that are missing, and which jobs read the existing ones', async () => {
      const ctx = await asMaintainer();

      expect(ctx.internals.missingSecretUses()).toEqual([{ name: 'DEPLOY_TOKEN', jobs: ['unit'] }]);
      expect(ctx.internals.existingSecretUses()).toEqual([
        { name: 'API_KEY', jobs: ['compile', 'unit'] },
        { name: 'UNUSED', jobs: [] },
      ]);
      expect(text(ctx.el.querySelector('.pipeline-editor__secrets-badge'))).toBe('1 à créer');
    });

    it('opens the secrets panel from the toolbar', async () => {
      const ctx = await asMaintainer();

      Array.from(ctx.el.querySelectorAll('button')).find((b) => text(b) === 'Variables et secrets')!.click();
      ctx.fixture.detectChanges();
      ctx.http.expectOne(SECRETS_URL).flush([]);

      expect(ctx.el.ownerDocument.querySelector('fg-pipeline-secrets')).not.toBeNull();
    });

    it('turns a variable that is a secret into a secret of the repository, and removes it from the job', async () => {
      const ctx = await asMaintainer();
      ctx.internals.patchJob('compile', { variables: [{ key: 'REGISTRY_TOKEN', value: 'abc' }, { key: 'MODE', value: 'fast' }] });

      ctx.internals.convertToSecret('compile', 0);
      const request = ctx.http.expectOne(SECRETS_URL);
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ key: 'REGISTRY_TOKEN', value: 'abc', masked: true });
      request.flush({ id: '3', key: 'REGISTRY_TOKEN', masked: true });
      ctx.http.expectOne((r) => r.url === SECRETS_URL && r.method === 'GET').flush([{ id: '3', key: 'REGISTRY_TOKEN', masked: true }]);

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')).toMatchObject({ variables: [{ key: 'MODE', value: 'fast' }] });
      expect(ctx.internals.secrets()).toEqual(['REGISTRY_TOKEN']);
      (await nextRequest(ctx, RENDER_URL)).flush(rendered());
    });

    it('removes the variable by its name even if the variables changed meanwhile, as a step of its own', async () => {
      const ctx = await asMaintainer();
      ctx.internals.patchJob('compile', { variables: [{ key: 'REGISTRY_TOKEN', value: 'abc' }, { key: 'MODE', value: 'fast' }] });

      ctx.internals.convertToSecret('compile', 0);
      ctx.internals.patchJob('compile', { variables: [{ key: 'EXTRA', value: '1' }, { key: 'REGISTRY_TOKEN', value: 'abc' }, { key: 'MODE', value: 'fast' }] });
      ctx.http.expectOne(SECRETS_URL).flush({ id: '3', key: 'REGISTRY_TOKEN', masked: true });
      ctx.http.expectOne((r) => r.url === SECRETS_URL && r.method === 'GET').flush([{ id: '3', key: 'REGISTRY_TOKEN', masked: true }]);

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.variables).toEqual([{ key: 'EXTRA', value: '1' }, { key: 'MODE', value: 'fast' }]);
      expect(ctx.internals.undoTip()).toMatch(/^Annuler : passage de REGISTRY_TOKEN en secret /);
      ctx.internals.undo();
      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.variables.map((v) => v.key)).toEqual(['EXTRA', 'REGISTRY_TOKEN', 'MODE']);
      (await nextRequest(ctx, RENDER_URL)).flush(rendered());
    });

    it('keeps the variable in the job when the secret could not be saved', async () => {
      const ctx = await asMaintainer();
      ctx.internals.patchJob('compile', { variables: [{ key: 'REGISTRY_TOKEN', value: 'abc' }] });

      ctx.internals.convertToSecret('compile', 0);
      ctx.http.expectOne(SECRETS_URL).flush(null, { status: 500, statusText: 'Server Error' });

      expect(ctx.internals.state().jobs.find((j) => j.name === 'compile')?.variables).toEqual([{ key: 'REGISTRY_TOKEN', value: 'abc' }]);
      (await nextRequest(ctx, RENDER_URL)).flush(rendered());
    });
  });
});
