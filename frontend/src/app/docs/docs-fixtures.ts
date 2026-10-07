// A small made-up documentation for the docs specs and stories: an index and its pages.
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { DocsIndex, DocsService } from './docs.service';

export const DOCS_INDEX: DocsIndex = {
  sections: [
    {
      slug: 'demarrer',
      title: 'Démarrer',
      pages: [
        { slug: 'presentation', title: 'Présentation', description: 'Ce qu’est FerrisGit.' },
        { slug: 'prise-en-main', title: 'Prise en main', description: 'Créer son compte et pousser un premier dépôt.' },
      ],
    },
    {
      slug: 'ci-cd',
      title: 'CI/CD',
      pages: [
        { slug: 'premiers-pas', title: 'Premiers pas', description: 'Lancer sa première pipeline.' },
        { slug: 'reference-yaml', title: 'Référence de .ferrisgit-ci.yml', description: 'Toutes les clés du fichier de pipeline.' },
      ],
    },
    {
      slug: 'administration',
      title: 'Administration',
      pages: [{ slug: 'installation', title: 'Installation', description: 'Démarrer FerrisGit avec Docker Compose.' }],
    },
  ],
};

export const DOCS_PAGES: Record<string, string> = {
  'demarrer/presentation': `# Présentation

FerrisGit est une plateforme Git que vous hébergez vous-même : dépôts, demandes de fusion, tickets, wikis et intégration continue.

## Ce qu'il sait faire

- Héberger des dépôts Git, publics ou privés.
- Relire le code avec les [demandes de fusion](/docs/demarrer/prise-en-main).
- Lancer des pipelines : voir la [référence](/docs/ci-cd/reference-yaml#variables).

## Ce qu'il ne fait pas

> **Note** : l'interface est uniquement en français.

Pas de registre de paquets, pas de pages statiques.
`,
  'demarrer/prise-en-main': `# Prise en main

Créez votre compte, activez la double authentification, puis poussez un premier dépôt.

## Créer un compte

Ouvrez la page d'inscription.

## Pousser un dépôt

\`\`\`bash
git remote add origin https://ferrisgit.example.com/alice/hello.git
git push -u origin main
\`\`\`
`,
  'ci-cd/premiers-pas': `# Premiers pas

Ajoutez un fichier \`.ferrisgit-ci.yml\` à la racine du dépôt.

## Un premier job

\`\`\`yaml ferrisgit-ci
stages: [test]
test:
  stage: test
  image: rust:1
  script:
    - cargo test
\`\`\`
`,
  'ci-cd/reference-yaml': `# Référence de .ferrisgit-ci.yml

Toutes les clés du fichier de pipeline, dans l'ordre où on les rencontre.

## Clés globales

| Clé | Type | Rôle |
| --- | --- | --- |
| \`stages\` | liste | L'ordre des étapes. |
| \`variables\` | table | Variables communes à tous les jobs. |

## Variables

Les variables d'environnement d'un job viennent de \`variables\` et des variables chiffrées du dépôt.

> **Attention** : une variable chiffrée n'est jamais affichée dans les journaux, mais un script peut la recopier.

### Variables prédéfinies

\`CI_COMMIT_SHA\`, \`CI_PIPELINE_ID\`.

## Cache

Le cache conserve des fichiers d'une pipeline à l'autre.
`,
  'administration/installation': `# Installation

Démarrez FerrisGit avec Docker Compose.

## Prérequis

Docker et un nom de domaine.

## Démarrer

\`\`\`bash
docker compose up -d
\`\`\`
`,
};

const notFound = () => throwError(() => new HttpErrorResponse({ status: 404, statusText: 'Not Found' }));

/** A `DocsService` answering from the fixtures, or with the given behaviours. */
export function fakeDocsService(options: { index?: () => Observable<DocsIndex>; page?: (key: string) => Observable<string> } = {}): Pick<DocsService, 'index' | 'page'> {
  return {
    index: options.index ?? (() => of(DOCS_INDEX)),
    page: (section: string, page: string) => {
      const key = `${section}/${page}`;
      if (options.page) {
        return options.page(key);
      }
      return key in DOCS_PAGES ? of(DOCS_PAGES[key]) : notFound();
    },
  };
}

export const LOADING_DOCS = () => NEVER;
export const FAILING_DOCS = () => throwError(() => new HttpErrorResponse({ status: 500, statusText: 'Server Error' }));
