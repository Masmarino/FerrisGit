# Moteur Kubernetes

Avec le moteur Kubernetes, FerrisGit exécute chaque job de pipeline dans un Pod de votre cluster, sans runner à installer. Cette page explique comment l'activer, les droits à donner, ce que FerrisGit crée dans le cluster, et surtout ce que ce moteur ne fait pas encore.

> **Attention** : le moteur Kubernetes n'est pas équivalent aux runners Docker. Le Pod d'un job ne contient pas le dépôt (rien n'est cloné), ne reçoit pas les variables CI du dépôt, ignore `tags` et ne renvoie ses journaux qu'à la fin. Il convient aujourd'hui aux jobs qui n'ont pas besoin du code source. Voir [Limites](#limites).

## Activer le moteur

Un administrateur ouvre **Admin > Réglages**, section **Exécution** :

- carte **Moteur d'exécution** : choisissez **Kubernetes** (l'autre choix est **Docker / runners**). Chaque réglage est enregistré dès que vous le modifiez ;
- carte **Kubernetes** : **Namespace Kubernetes** et **StorageClass pour le cache (RWX requis)**.

Le changement de moteur s'applique aux prochains pipelines. Un pipeline garde le moteur avec lequel il a été créé, même si vous rebasculez ensuite ; ses jobs en attente sont lancés par ce moteur-là. Quand le moteur actif n'est pas Docker, l'entrée **Runners** disparaît du menu.

## Accès au cluster

FerrisGit cherche la configuration du cluster une seule fois, au démarrage du serveur :

- **Serveur dans le cluster** : il utilise le compte de service monté dans son Pod (comportement par défaut de Kubernetes). Rien d'autre à configurer que les droits ci-dessous.
- **Serveur hors du cluster** (Docker Compose, par exemple) : montez un fichier kubeconfig dans le conteneur et indiquez-le avec la variable `KUBECONFIG`.

Si aucune configuration n'est trouvée, le serveur démarre quand même, et les runners Docker continuent de fonctionner. En revanche, avec le moteur Kubernetes sélectionné, chaque job échoue au lancement : le serveur journalise « Kubernetes execution engine is not available on this server (no cluster was reachable at startup) » et le job passe à **Échoué** sans journal. Corrigez l'accès puis **redémarrez le serveur**.

## Namespace

Les Pods sont créés dans le namespace choisi ainsi, par ordre de priorité :

1. le champ **Namespace Kubernetes** des réglages, s'il est rempli ;
2. sinon le namespace détecté au démarrage (celui du Pod du serveur, ou celui du contexte du kubeconfig) ; l'interface l'affiche (« Détecté automatiquement depuis le cluster ») ;
3. sinon `ferrisgit-jobs`.

FerrisGit ne crée pas le namespace : il doit exister, avec les droits ci-dessous.

> **Attention** : si vous modifiez le namespace dans les réglages, **redémarrez le serveur**. L'observateur de Pods lit le namespace une seule fois au démarrage, alors que les nouveaux Pods utilisent la valeur à jour : sans redémarrage, les jobs restent **En cours** sans journal, car personne ne surveille leurs Pods.

## Droits RBAC

FerrisGit n'agit que dans un namespace. Il lui faut, dans ce namespace, un `Role` limité aux Pods, à leurs journaux et aux PVC de cache, lié au compte de service qu'utilise le serveur. Voici le contenu exact du `Role` que crée le chart Helm de FerrisGit (`helm/ferrisgit/templates/rbac.yaml`), pour le namespace `ferrisgit` :

```yaml
apiVersion: v1
kind: ServiceAccount
metadata:
  name: ferrisgit-ci
  namespace: ferrisgit
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata:
  name: ferrisgit-ci
  namespace: ferrisgit
rules:
  - apiGroups: [""]
    resources: ["pods", "pods/log"]
    verbs: ["get", "list", "watch", "create", "delete"]
  - apiGroups: [""]
    resources: ["persistentvolumeclaims"]
    verbs: ["get", "list", "create"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata:
  name: ferrisgit-ci
  namespace: ferrisgit
subjects:
  - kind: ServiceAccount
    name: ferrisgit-ci
    namespace: ferrisgit
roleRef:
  kind: Role
  name: ferrisgit-ci
  apiGroup: rbac.authorization.k8s.io
```

Le Pod du serveur doit tourner avec ce compte de service (`serviceAccountName: ferrisgit-ci`). Ce rôle ne donne aucun accès aux Secrets. Le chart Helm fourni avec FerrisGit le crée déjà (`kubernetesExecutor.enabled`, activé par défaut) : voir [Déploiement sur Kubernetes](/docs/administration/deploiement-kubernetes).

Si les Pods de jobs doivent tourner dans un autre namespace que celui du serveur, créez le `Role` et le `RoleBinding` dans **ce** namespace (en gardant le compte de service du serveur comme sujet), et renseignez ce namespace dans les réglages.

Un droit facultatif : lire les `StorageClass` (ressource à l'échelle du cluster) permet à FerrisGit de proposer la classe par défaut dans l'interface. Il faut pour cela un `ClusterRole` avec `get` et `list` sur `storage.k8s.io/storageclasses`, lié au même compte de service. Sans lui, le champ n'est simplement pas prérempli.

## Un Pod par job

Quand un job est disponible (voir [Ordre de lancement](/docs/ci-cd/reference-yaml#ordre-de-lancement) : première étape, ou étapes précédentes et `needs` réussis), FerrisGit crée un Pod :

| Propriété | Valeur |
|---|---|
| Nom | `ferrisgit-job-<identifiant du job>` |
| Étiquette | `ferrisgit.io/job-id: <identifiant du job>` |
| Redémarrage | `restartPolicy: Never` |
| Conteneur | un seul, nommé `job`, avec l'image du job |
| Commande | `/bin/sh -c "<script>"` (lignes de `script` jointes par ` && `) |
| Variables | uniquement `variables` du job |
| Volumes | un volume monté sur `/ferrisgit-cache/<clé>` par clé de `cache` |

Le job passe à **En cours** dès que le Pod est créé. Il devient **Réussi** si le Pod finit en phase `Succeeded`, **Échoué** s'il finit en `Failed`. L'image doit contenir `/bin/sh`.

Si la création du Pod est refusée par le cluster (quota de Pods dépassé, namespace absent, droits insuffisants, image refusée par une règle d'admission), le job passe directement à **Échoué**, sans journal ; la cause est dans les journaux du serveur. Voir [Dépannage](/docs/ci-cd/depannage#pod-kubernetes-bloqué).

## Ressources

FerrisGit ne fixe aucune demande ni limite de CPU ou de mémoire, ne définit ni `securityContext`, ni compte de service, ni `imagePullSecrets`, ni sélecteur de nœud. Les Pods prennent les valeurs par défaut du namespace :

- un `LimitRange` du namespace peut fournir des demandes et des limites par défaut ;
- si un `ResourceQuota` du namespace exige des demandes et des limites et qu'aucun `LimitRange` n'en fournit, le cluster refuse les Pods ;
- une image privée n'est téléchargeable que si le compte de service par défaut du namespace a un `imagePullSecret`.

## Journaux

FerrisGit lit les journaux du Pod une fois, quand il est terminé (`Succeeded` ou `Failed`), puis les enregistre dans le job. Pendant l'exécution, la page du job affiche « (pas encore de logs) ». Le journal du conteneur mélange sortie standard et sortie d'erreur, sans masquage.

## Nettoyage

- Quand un Pod est terminé, FerrisGit récupère ses journaux, enregistre le résultat, puis supprime le Pod.
- Annuler un job ou un pipeline supprime immédiatement son Pod.
- Les PVC de cache ne sont jamais supprimées par FerrisGit : voir [Caches](/docs/ci-cd/caches#invalidation).

Un Pod terminé dont la suppression échoue est laissé dans le cluster (avertissement dans les journaux du serveur) : cherchez les Pods `ferrisgit-job-*` avec `kubectl get pods -n <namespace>`.

## Limites

- **Pas de dépôt dans le Pod.** Le script s'exécute dans l'image seule. Il doit récupérer lui-même le code dont il a besoin.
- **Pas de variables CI du dépôt.** Les variables saisies dans **Réglages > Variables CI/CD** ne sont pas transmises aux Pods. Seules les `variables` du fichier de pipeline le sont, en clair dans le dépôt.
- **`tags` ignoré.** Les étiquettes n'ont d'effet qu'avec les runners Docker.
- **Journaux à la fin seulement**, sans masquage.
- **Pas de délai maximal.** Un Pod qui n'atteint jamais l'état terminal (non planifiable, image introuvable en `ImagePullBackOff`, volume qui ne peut pas être lié) laisse son job **En cours** indéfiniment. Si vous supprimez un Pod à la main pendant qu'il s'exécute, FerrisGit ne s'en aperçoit pas non plus.
- **Pas de plafond de jobs simultanés** : le réglage **Jobs simultanés (runners Docker)** de **Admin > Réglages > Exécution** ne s'applique qu'aux runners Docker. FerrisGit crée un Pod pour chaque job dès qu'il est prêt, sans limite : ce plafond ne retient pas les jobs d'un pipeline Kubernetes.
- **Un seul namespace**, lu au démarrage pour la surveillance.
- **Une PVC `ReadWriteMany`** par clé de cache : la `StorageClass` doit le permettre. Voir [Caches](/docs/ci-cd/caches).
