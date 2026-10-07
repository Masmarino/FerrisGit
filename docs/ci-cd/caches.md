# Caches

Un cache conserve des fichiers (dépendances téléchargées, dossier de compilation) d'un job à l'autre et d'une pipeline à l'autre. FerrisGit ne fournit des caches que sur le moteur Kubernetes.

> **Attention** : avec les runners Docker, la clé `cache` est validée puis ignorée : rien n'est conservé entre deux jobs, chaque job repart d'un conteneur et d'un dossier de travail neufs. Le reste de cette page ne concerne que Kubernetes.

## Déclarer un cache

Dans un job, `cache` est une liste de clés :

```yaml
jobs:
  deps:
    stage: build
    image: node:22
    cache: [npm]
    variables:
      npm_config_cache: /ferrisgit-cache/npm
    script:
      - npm config get cache
```

Chaque clé :

- ne contient que des lettres minuscules ASCII, des chiffres et des tirets (`^[a-z0-9-]+$`), et n'est pas vide. Sinon, le fichier est refusé (voir [Référence](/docs/ci-cd/reference-yaml#erreurs-de-validation)) ;
- désigne un volume distinct. Un job peut déclarer plusieurs clés.

## Où retrouver le cache dans le job

Le volume de la clé `ma-cle` est monté dans le conteneur à l'emplacement `/ferrisgit-cache/ma-cle`. FerrisGit ne définit aucune variable d'environnement et ne redirige aucun outil : c'est à votre script (ou à des variables `variables`) de demander à l'outil d'écrire dans ce dossier, comme `CARGO_HOME`, `CARGO_TARGET_DIR` ou `npm_config_cache` dans les exemples.

Le dossier est celui d'un volume qui peut appartenir à un autre utilisateur que celui de l'image : si l'écriture est refusée, vérifiez les droits côté cluster.

## Portée

- Un cache appartient à un **dépôt** : la clé `cargo-target` de deux dépôts désigne deux volumes différents, et deux dépôts ne partagent jamais de cache.
- Dans un dépôt, tous les jobs, de toutes les pipelines, de toutes les branches, qui déclarent la même clé voient le même volume. Il n'y a pas de cache par branche.
- Le volume est partagé en lecture et écriture (accès `ReadWriteMany`) et FerrisGit n'y pose aucun verrou : deux jobs parallèles qui écrivent la même clé peuvent se gêner. Séparez les clés, ou ordonnez les jobs avec `needs` ou en les plaçant dans des étapes différentes.

## Stockage

Sur Kubernetes, FerrisGit crée au premier usage une `PersistentVolumeClaim` (PVC) par dépôt et par clé :

| Propriété | Valeur |
|---|---|
| Nom | `ferrisgit-cache-<identifiant du dépôt>-<clé>` |
| Namespace | celui des Pods de jobs (voir [Moteur Kubernetes](/docs/ci-cd/kubernetes#namespace)) |
| Mode d'accès | `ReadWriteMany`, sans repli sur `ReadWriteOnce` |
| Taille demandée | 5 Gio (valeur fixe) |
| `StorageClass` | celle du réglage **StorageClass pour le cache (RWX requis)** |

Une PVC existante est réutilisée telle quelle. FerrisGit n'attend pas qu'elle soit liée à un volume : si la `StorageClass` ne sait pas fournir du `ReadWriteMany`, la PVC reste `Pending`, le Pod du job aussi, et le job reste **En cours** (voir [Dépannage](/docs/ci-cd/depannage#pod-kubernetes-bloqué)).

### Régler la StorageClass

Un administrateur la renseigne dans **Admin > Réglages > Exécution**, carte **Kubernetes**, champ **StorageClass pour le cache (RWX requis)**. Le réglage s'enregistre quand vous validez la saisie.

- Si le champ est vide, tout job qui déclare `cache` échoue avant le lancement de son Pod. Les jobs sans `cache` ne sont pas concernés.
- Quand le cluster a une `StorageClass` par défaut et que FerrisGit a le droit de la lire, l'interface la propose (« Détecté automatiquement depuis le cluster ») sans l'appliquer. Vérifiez qu'elle supporte `ReadWriteMany` avant de la saisir.

Pour la liste des droits nécessaires, voir [Moteur Kubernetes](/docs/ci-cd/kubernetes#droits-rbac).

## Invalidation

FerrisGit n'invalide ni ne purge jamais un cache : il n'y a ni expiration, ni limite de taille, ni suppression automatique.

- **Repartir de zéro avec une nouvelle clé** : renommez la clé dans le fichier (`cargo-target` devient `cargo-target-v2`). Une nouvelle PVC est créée ; l'ancienne reste dans le cluster.
- **Supprimer le cache** : supprimez la PVC avec `kubectl`, par un compte du cluster qui en a le droit. Le rôle de FerrisGit n'a pas le droit de supprimer une PVC. Elle sera recréée, vide, au prochain job qui déclare la clé.

```bash
kubectl -n <namespace> get pvc
kubectl -n <namespace> delete pvc ferrisgit-cache-<identifiant du dépôt>-<clé>
```

FerrisGit ne supprime jamais de PVC, pas même quand vous supprimez un dépôt.

## Exemple

Ce job écrit une ligne dans son cache à chaque exécution et affiche tout l'historique : relancé, il montre les lignes des passages précédents (sur Kubernetes seulement).

```yaml ferrisgit-ci
stages: [demo]

jobs:
  cache-demo:
    stage: demo
    image: alpine:3.20
    cache: [demo-cache]
    script:
      - mkdir -p /ferrisgit-cache/demo-cache
      - echo "passage du $(date)" >> /ferrisgit-cache/demo-cache/historique.txt
      - cat /ferrisgit-cache/demo-cache/historique.txt
```

D'autres exemples (Rust, npm) sont dans [Exemples de pipelines](/docs/ci-cd/exemples).
