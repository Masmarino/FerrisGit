import { TestBed } from '@angular/core/testing';
import { jobTiles, ParamValues, JobTile, tileCategories } from './pipeline-catalog';
import { PipelineTilePicker } from './pipeline-tile-picker';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('PipelineTilePicker', () => {
  function setup(secrets: string[] | null = null, dockerInJobs = true) {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(PipelineTilePicker);
    fixture.componentRef.setInput('stage', 'test');
    fixture.componentRef.setInput('secrets', secrets);
    fixture.componentRef.setInput('dockerInJobs', dockerInJobs);
    const chosen: { tile: JobTile; values: ParamValues }[] = [];
    fixture.componentInstance.chosen.subscribe((choice) => chosen.push(choice));
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement, chosen };
  }

  it('says where the job is going', () => {
    const { el } = setup();

    expect(text(el.querySelector('.tile-picker__intro'))).toContain('étape test');
  });

  it('groups every tile under its category, in the order of the categories', () => {
    const { el } = setup();

    const headings = Array.from(el.querySelectorAll('.tile-picker__heading')).map(text);

    expect(headings).toEqual(tileCategories().filter((c) => jobTiles().some((t) => t.category === c.id)).map((c) => c.title));
    expect(el.querySelectorAll('.tile-picker__tile')).toHaveLength(jobTiles().length);
  });

  it('hands over the tile that was clicked', () => {
    const { el, chosen } = setup();

    el.querySelector<HTMLButtonElement>('[data-tile="go-test"] .tile-picker__choose')!.click();

    expect(chosen.map((c) => c.tile.id)).toEqual(['go-test']);
    expect(chosen[0].values).toEqual({});
  });

  it('explains each tile in a bubble of its own', () => {
    const { el } = setup();

    const tip = el.querySelector<HTMLButtonElement>('[data-tile="http-deploy"] fg-help-tip button')!;

    expect(tip.getAttribute('aria-label')).toBe('Aide : Déployer par un appel HTTP');
  });

  it('shows the secrets a tile needs that the repository lacks', () => {
    const { el } = setup(['DEPLOY_URL']);

    expect(text(el.querySelector('[data-tile="http-deploy"] [data-note="secrets"]'))).toBe('Secrets à créer : DEPLOY_TOKEN');
    expect(el.querySelector('[data-tile="go-test"] gbt-badge')).toBeNull();
  });

  it('gives each missing secret a badge of its own, so that a long list wraps instead of leaving the tile', () => {
    const { el } = setup([]);

    const note = el.querySelector('[data-tile="docker-build"] [data-note="secrets"]')!;
    expect(Array.from(note.querySelectorAll('gbt-badge'), text)).toEqual(['REGISTRY_USER', 'REGISTRY_PASSWORD']);
    // Read aloud as separate words, not as one run of names.
    expect(note.textContent!.replace(/\s+/g, ' ').trim()).toBe('Secrets à créer : REGISTRY_USER REGISTRY_PASSWORD');
  });

  it('shows nothing about secrets when it cannot know which exist', () => {
    const { el } = setup(null);

    expect(el.querySelector('gbt-badge')).toBeNull();
  });

  describe('tiles with questions', () => {
    it('asks them before making the job, and hands over the answers', () => {
      const { fixture, el, chosen } = setup();

      el.querySelector<HTMLButtonElement>('[data-tile="ssh-run"] .tile-picker__choose')!.click();
      fixture.detectChanges();
      expect(chosen).toEqual([]);
      expect(el.querySelector('fg-pipeline-tile-form')).not.toBeNull();
      expect(el.querySelector('.tile-picker__tiles')).toBeNull();

      Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Ajouter le job')!.click();

      expect(chosen.map((c) => c.tile.id)).toEqual(['ssh-run']);
      expect(chosen[0].values).toMatchObject({ host: 'vm.example.com', user: 'deploy', port: '22' });
    });

    it('goes back to the tiles without making anything', () => {
      const { fixture, el, chosen } = setup();
      el.querySelector<HTMLButtonElement>('[data-tile="k8s-apply"] .tile-picker__choose')!.click();
      fixture.detectChanges();

      Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Retour')!.click();
      fixture.detectChanges();

      expect(chosen).toEqual([]);
      expect(el.querySelectorAll('.tile-picker__tile')).toHaveLength(jobTiles().length);
    });
  });

  describe("with the job's own Docker", () => {
    it('says when the jobs of this instance have none, and still offers the tile for a remote daemon', () => {
      const { fixture, el } = setup(null, false);
      const docker = el.querySelector<HTMLButtonElement>('[data-tile="docker-build"] .tile-picker__choose')!;

      expect(text(el.querySelector('[data-tile="docker-build"] [data-note="docker"]'))).toBe(
        "Docker désactivé Les jobs n'ont pas Docker sur cette instance : demandez-le à un administrateur, ou choisissez un démon distant.",
      );
      expect(docker.disabled).toBe(false);
      docker.click();
      fixture.detectChanges();
      expect(el.querySelector('fg-pipeline-tile-form')).not.toBeNull();
    });

    it('says nothing when the jobs have Docker, and never greys a tile', () => {
      const { el } = setup(null, true);

      expect(el.querySelector('[data-note="docker"]')).toBeNull();
      expect(el.querySelectorAll('.tile-picker__choose:disabled')).toHaveLength(0);
    });
  });
});
