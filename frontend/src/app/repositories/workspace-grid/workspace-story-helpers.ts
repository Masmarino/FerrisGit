// Storybook fixtures and layout guards for the workspace grid and the pages built on it.
import { waitFor } from 'storybook/test';
import { Repository } from '../repositories.service';
import { repositoryFixture } from '../repository-fixtures';
import { GroupMembership } from '../../groups/groups.service';
import { daysAgo, hoursAgo } from '../../shared/layout/page-story-helpers';

let nextId = 0;
export function repository(fields: Partial<Repository> & Pick<Repository, 'path'>): Repository {
  return repositoryFixture({ id: `repo-${++nextId}`, createdAt: daysAgo(3), ...fields });
}

export const REPOSITORIES: Repository[] = [
  repository({ path: ['camille.martin', 'facturation-api'], description: 'API de facturation et de génération de devis pour les PME.', visibility: 'public', createdAt: hoursAgo(5), starCount: 12, sizeBytes: 18_400_000 }),
  repository({ path: ['camille.martin', 'infra-interne'], description: 'Scripts Terraform et playbooks Ansible de la plateforme.', createdAt: daysAgo(2) }),
  repository({ path: ['acme-france', 'produits', 'web', 'portail-client'], description: 'Portail web des clients Acme.', role: 'maintainer', createdAt: daysAgo(9) }),
  repository({ path: ['julien.dubois', 'outils-ci'], description: 'Images Docker et gabarits de pipelines partagés.', role: 'contributor', visibility: 'public', createdAt: daysAgo(21), starCount: 0 }),
  repository({ path: ['communaute', 'guide-contribution'], role: 'reader', visibility: 'public', createdAt: daysAgo(60), starCount: 31 }),
];

export const MEMBERSHIPS: GroupMembership[] = [
  { id: 'group-1', path: 'acme-france', role: 'maintainer' },
  { id: 'group-2', path: 'acme-france/produits', role: 'maintainer' },
  { id: 'group-3', path: 'acme-france/produits/web', role: 'contributor' },
  { id: 'group-4', path: 'communaute', role: 'reader' },
];

export const LONG_REPOSITORIES: Repository[] = [
  repository({
    path: ['maximilien-de-la-tour-d-auvergne', 'plateforme-de-gestion-des-ressources-humaines-et-de-la-paie-multi-etablissements'],
    description: 'Une description très longue qui explique en détail tout ce que fait ce dépôt, bien au-delà de ce que la ligne peut afficher sans la tronquer.',
    visibility: 'public',
    createdAt: hoursAgo(1),
    starCount: 1234,
    sizeBytes: 2_400_000_000,
  }),
  repository({
    path: ['acme-france', 'produits', 'applications-mobiles', 'equipe-paiements', 'sdk-paiement-sans-contact-ios-et-android'],
    description: 'Pasd’espacesdanscettedescriptionquidoitquandmêmesetronquerproprementsansfairedéborderlaligne',
    role: 'maintainer',
    createdAt: daysAgo(4),
  }),
  ...REPOSITORIES.slice(0, 2),
];

export const LONG_MEMBERSHIPS: GroupMembership[] = [
  { id: 'group-long', path: 'acme-france/produits/applications-mobiles/equipe-paiements-et-facturation-internationale', role: 'maintainer' },
  ...MEMBERSHIPS.slice(0, 2),
];

export const MANY_REPOSITORIES: Repository[] = Array.from({ length: 40 }, (_, index) =>
  repository({
    path: ['camille.martin', `service-${String(index + 1).padStart(2, '0')}`],
    description: index % 3 === 0 ? '' : 'Un micro-service de la plateforme.',
    visibility: index % 4 === 0 ? 'public' : 'private',
    createdAt: daysAgo(index + 1),
  }),
);

const rect = (el: Element) => el.getBoundingClientRect();

/** Layout guards at any viewport: no horizontal overflow, aside beside the main column from 769px and under it below, one-line row titles ending on one edge, all icons registered. */
export function assertWorkspaceLayout(canvas: HTMLElement): void {
  const doc = canvas.ownerDocument.documentElement;
  if (doc.clientWidth === 0) {
    return; // Hidden frame (docs page), nothing to measure.
  }
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);

  const card = canvas.querySelector('gbt-list-card');
  if (!card) throw new Error('grid not rendered yet');

  const layout = canvas.querySelector('gbt-page-layout');
  const main = canvas.querySelector('.gbt-page-layout__main');
  const aside = canvas.querySelector('.gbt-page-layout__aside');
  if (layout && main && aside && rect(aside).height > 0) {
    if (rect(layout).width >= 769) {
      if (rect(aside).left < rect(main).right) throw new Error('the aside should sit beside the main column');
      if (Math.abs(rect(aside).top - rect(main).top) > 1) throw new Error('the aside should start level with the main column');
    } else if (rect(aside).top < rect(main).bottom) {
      throw new Error('the aside should stack under the main column');
    }
  }

  const header = card.querySelector('.gbt-list-card__header');
  if (header && rect(header).height > 0) {
    for (const child of Array.from(header.children)) {
      if (rect(child).right > rect(card).right + 0.5) throw new Error('the header content spills out of the card');
    }
  }

  const rows = Array.from(card.querySelectorAll<HTMLElement>('.workspace-grid__items gbt-list-row'));
  const rights = new Set<number>();
  for (const row of rows) {
    const title = row.querySelector('.gbt-list-row__title > a')!;
    if (rect(title).height > 26) throw new Error(`title on more than one line: ${title.textContent}`);
    const trailing = row.querySelector('.gbt-list-row__trailing')!;
    rights.add(Math.round(rect(trailing).right));
    for (const part of Array.from(row.querySelectorAll('.gbt-list-row__main *'))) {
      if (rect(part).width > 0 && !part.closest('.sr-only') && rect(part).right > rect(card).right + 0.5) {
        throw new Error(`row content spills out of the card: ${part.className}`);
      }
    }
  }
  if (rights.size > 1) throw new Error(`rows end on ${rights.size} different right edges`);

  for (const icon of Array.from(canvas.querySelectorAll('gbt-icon'))) {
    if (rect(icon).width > 0 && !icon.querySelector('svg')) throw new Error(`unregistered icon in "${icon.parentElement?.textContent?.trim()}"`);
  }
}

export async function expectWorkspaceLayout({ canvasElement }: { canvasElement: HTMLElement }): Promise<void> {
  await waitFor(() => assertWorkspaceLayout(canvasElement), { timeout: 3000 });
}
