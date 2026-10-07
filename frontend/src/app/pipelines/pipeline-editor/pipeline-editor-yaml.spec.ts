import { DefinitionDto, ParsedPipeline } from './pipeline-definitions.service';
import { FILE, PARSE_URL, RENDER_URL, answerRender, cardNames, lanes, opened, parsed, rendered, settle, setup, sleep, stageNames, text, verifyRequestsAfterEach } from './pipeline-editor-testing';

describe('PipelineEditor YAML', () => {
  verifyRequestsAfterEach();

  describe('editing the YAML', () => {
    const TYPED = 'stages: [build]\n# keep me\njobs: {}\n';

    /** Waits out the pause before typed YAML is checked, and answers with `result`. */
    async function answerParse(ctx: ReturnType<typeof setup>, result: ParsedPipeline) {
      await sleep(300);
      // A card change made just before is still on its way to the server: the answer does not matter here.
      ctx.http.match(RENDER_URL).forEach((request) => request.flush(rendered()));
      ctx.http.expectOne(PARSE_URL).flush(result);
      ctx.fixture.detectChanges();
    }

    async function inYaml(result: ParsedPipeline = parsed()) {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.setMode('yaml');
      ctx.fixture.detectChanges();
      await answerParse(ctx, result);
      return ctx;
    }

    it("opens on the repository's own text, comments included, when the cards have not been touched", async () => {
      const ctx = await inYaml();

      expect(ctx.el.querySelector('.pipeline-editor__board')).toBeNull();
      await settle(ctx);
      expect(ctx.el.querySelector<HTMLTextAreaElement>('.pipeline-editor__yaml-input textarea')!.value).toBe(FILE);
    });

    it("opens on the server's writing of the cards once they were changed", async () => {
      const ctx = opened();
      await answerRender(ctx);
      ctx.internals.patchJob('compile', { image: 'changed' });

      ctx.internals.setMode('yaml');
      ctx.http.expectOne(RENDER_URL).flush(rendered({ yaml: 'written by the server\n' }));
      ctx.fixture.detectChanges();
      await answerParse(ctx, parsed());
      await settle(ctx);

      expect(ctx.el.querySelector<HTMLTextAreaElement>('.pipeline-editor__yaml-input textarea')!.value).toBe('written by the server\n');
    });

    it('checks what is typed with the server and lists every problem it finds', async () => {
      const ctx = await inYaml();

      ctx.internals.typeYaml(TYPED);
      await answerParse(ctx, parsed({ problems: [{ code: 'unknown_dependency', message: 'x', job: 'a', dependency: 'b' }] }));

      expect(text(ctx.el.querySelector('.pipeline-editor__issues'))).toContain('Le job « a » dépend de « b », qui n\'existe pas.');
      expect(ctx.internals.canSave()).toBe(false);
    });

    it('can be saved once it is valid and differs from the repository', async () => {
      const ctx = await inYaml();
      expect(ctx.internals.canSave()).toBe(false);

      ctx.internals.typeYaml(TYPED);
      await answerParse(ctx, parsed());

      expect(ctx.internals.canSave()).toBe(true);
    });

    it('cannot go back to the cards while the YAML cannot be read, and says why', async () => {
      const ctx = await inYaml();

      ctx.internals.typeYaml('jobs: [');
      await answerParse(ctx, parsed({ definition: null, problems: [{ code: 'invalid_yaml', message: 'invalid YAML: did not find expected node' }] }));

      expect(ctx.internals.doc.modeOptions().find((option) => option.value === 'cards')?.disabled).toBe(true);
      expect(text(ctx.el.querySelector('.pipeline-editor__issues'))).toContain("Le fichier n'est pas un YAML valide : did not find expected node");
      ctx.internals.setMode('cards');
      expect(ctx.el.querySelector('.pipeline-editor__board')).toBeNull();
      expect(ctx.internals.canSave()).toBe(false);
    });

    it('brings the typed pipeline back as cards, and says what that rewrite dropped', async () => {
      const ctx = await inYaml();
      const typed: DefinitionDto = { stages: ['ship'], jobs: { push: { stage: 'ship', image: 'alpine', script: ['echo hi'], variables: {}, needs: [], tags: [], cache: [] } } };

      ctx.internals.typeYaml(TYPED);
      await answerParse(ctx, parsed({ definition: typed, hasComments: true, ignoredFields: ['include'] }));
      ctx.internals.setMode('cards');
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['ship']);
      expect(cardNames(lanes(ctx.el)[0])).toEqual(['push']);
      const notes = text(ctx.el.querySelector('.pipeline-editor__notes'));
      expect(notes).toContain('Passer aux cartes a réécrit le fichier');
      expect(notes).toContain('commentaires');
      expect(notes).toContain('include');
      await answerRender(ctx);
    });

    it('keeps the repository file as it is when the YAML is left without a change', async () => {
      const ctx = await inYaml();

      ctx.internals.setMode('cards');
      ctx.fixture.detectChanges();
      await settle(ctx);

      expect(stageNames(ctx.el)).toEqual(['build', 'test']);
      expect(ctx.internals.canSave()).toBe(false);
    });
  });
});
