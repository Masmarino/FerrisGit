export const APP_URL = 'https://app.ferrisgit.pro'
export const DOCS_URL = `${APP_URL}/docs`
export const GITHUB_URL = 'https://github.com/Masmarino/FerrisGit'
export const MILESTONES_URL = `${GITHUB_URL}/milestones`
export const README_ROADMAP_URL = `${GITHUB_URL}#roadmap`
export const README_LICENSE_URL = `${GITHUB_URL}#license`
export const ARTIFERRIS_URL = 'https://github.com/Masmarino/ArtiFerris'

// Deep links into the documentation, which is written in French.
export const DOCS = {
  install: `${DOCS_URL}/administration/installation`,
  kubernetes: `${DOCS_URL}/administration/deploiement-kubernetes`,
  security: `${DOCS_URL}/administration/securite`,
  ciReference: `${DOCS_URL}/ci-cd/reference-yaml`,
  api: `${DOCS_URL}/api/introduction`,
} as const
