# Déploiement sur Kubernetes

FerrisGit se déploie dans un cluster avec le chart Helm du dépôt, `helm/ferrisgit`. Il installe le serveur, sa base PostgreSQL, le compte de service du moteur d'exécution Kubernetes, les règles réseau et l'`Ingress`. Le chart remplace les manifestes `k8s/` que le dépôt contenait auparavant (voir [Migrer depuis les anciens manifestes](/docs/administration/deploiement-kubernetes#migrer-depuis-les-anciens-manifestes)). Il n'est pas publié dans un dépôt de charts : vous le déployez depuis une copie du dépôt, ou par la CI.

Les noms ci-dessous supposent une release Helm appelée `ferrisgit`, dans le namespace `ferrisgit`. C'est ce que fait la CI ; gardez-les, la documentation et les commandes s'y réfèrent.

## Ce que le chart déploie

| Ressource | Nom | Rôle |
|---|---|---|
| `Deployment` | `ferrisgit` | Le serveur : une réplique, stratégie `Recreate`, conteneur `ferrisgit-server` sur le port 8080. |
| `Service` | `ferrisgit` | Le port 8080 du serveur. |
| `PersistentVolumeClaim` | `ferrisgit-storage` | Les dépôts Git, les wikis et les fichiers des releases, monté sur `/data` (20 Gio par défaut, `ReadWriteOnce`). |
| `Deployment`, `Service` | `ferrisgit-postgres` | PostgreSQL 18, une réplique, stratégie `Recreate`, port 5432. |
| `PersistentVolumeClaim` | `ferrisgit-postgres-data` | La base de données (10 Gio par défaut, `ReadWriteOnce`). |
| `Secret` | `ferrisgit-secrets` | Le mot de passe de la base, `JWT_SECRET`, `SETTINGS_ENCRYPTION_KEY`, le mot de passe du premier administrateur et `DATABASE_URL`. |
| `ServiceAccount`, `Role`, `RoleBinding` | `ferrisgit-ci` | Les droits du moteur d'exécution Kubernetes (voir plus bas). Absents avec `kubernetesExecutor.enabled=false`. |
| `ClusterRole`, `ClusterRoleBinding` | `ferrisgit-ferrisgit-storageclasses` | Lecture des `StorageClass`, pour pré-remplir un réglage. Absents avec `kubernetesExecutor.detectStorageClass=false`. |
| `NetworkPolicy` | `ferrisgit`, `ferrisgit-postgres` | Seul le Pod de FerrisGit joint PostgreSQL ; le Pod de FerrisGit n'accepte que le port 8080. Absentes avec `networkPolicy.enabled=false`. |
| `Ingress` | `ferrisgit` | L'accès public en HTTPS. Absent avec `ingress.enabled=false`. |

Le Pod du serveur :

- tourne avec l'uid 100 et le gid 101, ceux de l'utilisateur `ferrisgit` de l'image, sans élévation de privilèges, sans capacité Linux, avec le profil seccomp `RuntimeDefault` et un système de fichiers racine en lecture seule. Il n'écrit que dans `/data` (le volume) et `/tmp` (un `emptyDir`). `HOME` vaut `/tmp` : les commandes `git` que le serveur lance (`git init`, `git config`, `git http-backend`) y trouvent un répertoire personnel inscriptible, et leurs fichiers temporaires y vont aussi ;
- utilise le `ServiceAccount` `ferrisgit-ci`, ce qui permet au [moteur Kubernetes](/docs/ci-cd/kubernetes) de créer des Pods sans autre configuration ;
- démarre avec trois sondes. Les migrations de la base s'appliquent avant que le serveur écoute : la sonde de démarrage (`GET /healthz`, toutes les 5 secondes, 60 échecs permis) laisse donc 5 minutes avant le premier succès. Ensuite, la sonde de vivacité appelle `GET /healthz` et la sonde de disponibilité `GET /readyz`. `/healthz` ne dépend pas de la base ; `/readyz` répond `503` quand elle ne répond pas, ce qui retire le Pod du `Service` sans le redémarrer. Voir [Administration](/docs/api/administration) ;
- demande 100m de CPU et 256 Mio de mémoire, avec une limite mémoire de 2 Gio ;
- laisse 30 secondes à l'arrêt (`terminationGracePeriodSeconds`). Sur `SIGTERM`, le serveur finit les requêtes en cours et abandonne au bout de 10 secondes les connexions encore ouvertes, comme les flux d'événements des pipelines.

> **Attention** : FerrisGit garde en mémoire le corps entier d'un `git push` pendant qu'il le reçoit, jusqu'à la taille maximale réglée dans [Réglages de l'instance](/docs/administration/reglages) (500 Mio par défaut), et jusqu'à 8 envois en même temps. Avec la limite de 2 Gio du chart, plusieurs gros push simultanés peuvent faire tuer le Pod pour manque de mémoire. Relevez `resources.limits.memory` si vous acceptez de gros envois.

## Ce que le cluster doit fournir

- **Traefik** comme contrôleur d'entrée, avec dans le namespace `traefik` les `Middleware` `https` (redirection vers HTTPS) et `headers` (en-têtes de sécurité) et le `TLSOption` `mintls13` (TLS 1.3 au minimum). Le chart y fait référence mais ne les crée pas ; s'ils manquent, Traefik ignore la route. Pour ne pas les utiliser, mettez `ingress.middlewares=""` et `ingress.tlsOptions=""`. Le chart n'utilise volontairement pas de middleware de limitation de débit : un clone ou un push est une rafale de requêtes qu'un tel middleware rejetterait.
- **Le résolveur de certificats `letsencrypt-http`** dans la configuration de Traefik (défaut du chart), ou un `ClusterIssuer` cert-manager (voir [Le certificat](/docs/administration/deploiement-kubernetes#le-certificat)).
- **Un enregistrement DNS A** de l'hôte public (`ingress.host`) vers l'adresse du load balancer de Traefik. Le défaut de `ingress.host`, `app.ferrisgit.pro`, est celui de l'auteur : remplacez-le.
- **Un CNI qui applique les `NetworkPolicy`.** Sinon elles sont acceptées mais sans effet : PostgreSQL reste joignable depuis tout le cluster.
- **Le réseau des Pods du contrôleur d'entrée**, pour `ferrisgit.trustedProxyCidrs` (voir [Installer](/docs/administration/deploiement-kubernetes#installer)).
- **Une `StorageClass` par défaut**, ou `storage.storageClassName` et `postgres.storage.storageClassName`.
- Facultatif : un compte de service et un `Role` pour que la CI GitHub déploie (voir [Les droits de la CI](/docs/administration/deploiement-kubernetes#les-droits-de-la-ci)).

## Installer

Depuis une copie du dépôt, avec un compte qui peut créer les ressources du chart :

```bash
helm upgrade --install ferrisgit ./helm/ferrisgit \
  --namespace ferrisgit --create-namespace \
  --set image.tag=0.1.1 \
  --set ingress.host=git.example.com \
  --set-string ferrisgit.trustedProxyCidrs=10.42.0.0/16
kubectl -n ferrisgit rollout status deployment/ferrisgit
```

Au premier déploiement, le serveur peut redémarrer une ou deux fois le temps que PostgreSQL soit prêt : s'il ne parvient pas à se connecter à la base au démarrage, il s'arrête et Kubernetes le relance. À la fin, `helm` affiche des notes qui reprennent l'adresse, l'état du certificat, les ressources Traefik attendues et la commande qui donne le mot de passe de l'administrateur.

Les valeurs que vous avez le plus souvent à régler (la liste complète, commentée, est dans `helm/ferrisgit/values.yaml`) :

| Valeur | Défaut | Rôle |
|---|---|---|
| `image.tag` | aucun, obligatoire | La version à déployer, par exemple `0.1.1`. |
| `image.digest` | vide | Un `sha256:…` : le Pod exécute exactement cette image, quoi que l'étiquette désigne plus tard. La CI le renseigne. |
| `image.repository` | `masmarino/ferrisgit` | L'image. Pour un registre privé, `imagePullSecrets` liste les Secrets de registre. |
| `ingress.host` | `app.ferrisgit.pro` | L'hôte public. |
| `ferrisgit.publicUrl` | vide | L'adresse publique (schéma et hôte, sans chemin). Vide : `https://<ingress.host>`. Les passkeys exigent qu'elle soit l'origine du navigateur. |
| `ferrisgit.trustedProxyCidrs` | vide | Le réseau des Pods du contrôleur d'entrée, plages CIDR séparées par des virgules. |
| `ferrisgit.bootstrapAdminUsername` | `admin` | Le nom du premier administrateur. Vide : pas de compte créé. |
| `storage.size`, `postgres.storage.size` | `20Gi`, `10Gi` | Taille des volumes. |
| `kubernetesExecutor.enabled` | `true` | Le `ServiceAccount` et le `Role` du moteur d'exécution. Désactivé, le Pod n'a pas de jeton d'API et seuls les runners Docker fonctionnent. |
| `kubernetesExecutor.namespace` | vide | Le namespace des Pods de jobs. Vide : celui de la release. Un autre namespace doit exister, et le réglage `k8s_namespace` doit recevoir la même valeur. |
| `persistence.keepOnUninstall` | `true` | Garde les deux volumes quand vous désinstallez. |
| `terminationGracePeriodSeconds` | `30` | Le délai laissé à l'arrêt. |

**`ferrisgit.trustedProxyCidrs`.** Les limites de débit par adresse IP (connexion, inscription, activation) s'appuient sur l'en-tête `X-Forwarded-For` quand la connexion vient d'une adresse listée ici. Mettez-y uniquement le réseau des Pods de votre contrôleur d'entrée, jamais un réseau depuis lequel des clients ou des charges de travail non fiables peuvent se connecter. Vide, tous les visiteurs partagent un seul quota, celui de l'adresse du contrôleur : avec l'`Ingress`, le chart refuse alors de s'afficher, sauf si `ferrisgit.allowSharedRateLimit=true`. Pour plusieurs plages en ligne de commande : `--set-string ferrisgit.trustedProxyCidrs='10.42.0.0/16\,10.43.0.0/16'`, ou un fichier de valeurs. Voir [Configuration](/docs/administration/configuration).

**Le premier administrateur.** Le serveur le crée au démarrage, seulement tant que la table des utilisateurs est vide, avec `FERRISGIT_BOOTSTRAP_ADMIN_USERNAME` (`ferrisgit.bootstrapAdminUsername`) et le mot de passe de la clé `BOOTSTRAP_ADMIN_PASSWORD` du Secret :

```bash
kubectl -n ferrisgit get secret ferrisgit-secrets -o jsonpath='{.data.BOOTSTRAP_ADMIN_PASSWORD}' | base64 -d
```

Au premier accès, FerrisGit vous mène à la configuration de la double authentification : voir [Installation](/docs/administration/installation).

## Les secrets

Le chart écrit dans le Secret `ferrisgit-secrets` quatre valeurs, que vous pouvez laisser vides pour qu'il les génère à la première installation :

| Valeur du chart | Clé du Secret | Contrainte |
|---|---|---|
| `secrets.postgresPassword` | `POSTGRES_PASSWORD` | Aucune ; la chaîne de connexion `DATABASE_URL` en est déduite avec l'encodage nécessaire. PostgreSQL ne lit ce mot de passe qu'en initialisant un volume vide : une fois stocké, il est conservé. |
| `secrets.jwtSecret` | `JWT_SECRET` | 32 caractères au minimum. |
| `secrets.settingsEncryptionKey` | `SETTINGS_ENCRYPTION_KEY` | Exactement 32 caractères. Elle chiffre les variables CI, les secrets de webhooks, le mot de passe SMTP et les secrets TOTP. **Elle ne peut pas être changée** : il n'y a pas de rotation, et un changement rend illisible tout ce qui est chiffré. |
| `secrets.bootstrapAdminPassword` | `BOOTSTRAP_ADMIN_PASSWORD` | 8 caractères au minimum. |

À chaque mise à jour, le chart retrouve les valeurs déjà stockées avec la fonction Helm `lookup`. Une valeur explicite l'emporte sur la valeur stockée, sauf pour le mot de passe de la base. Le Secret porte `helm.sh/resource-policy: keep` : `helm uninstall` ne le supprime pas.

> **Attention** : avec `helm template`, `helm install --dry-run`, Argo CD ou Flux, `lookup` ne voit pas le Secret du cluster et chaque rendu inventerait de nouvelles valeurs (un mot de passe de base que personne ne connaît, une clé de chiffrement qui rend les secrets stockés illisibles). Dans ces cas, fixez les quatre valeurs vous-même, avec un fichier de valeurs : `helm/ferrisgit/values-secrets.example.yaml` en est le modèle, et `values-secrets.yaml` est ignoré par git.

Ne supprimez jamais le Secret tant que le volume `ferrisgit-postgres-data` existe, et sauvegardez-le avec vos autres secrets : voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour).

## Le certificat

Trois modes, selon deux valeurs :

| `ingress.certResolver` | `ingress.clusterIssuer` | Effet |
|---|---|---|
| `letsencrypt-http` (défaut) | vide (défaut) | Traefik obtient le certificat par son propre résolveur ACME (HTTP-01). L'`Ingress` porte l'annotation `traefik.ingress.kubernetes.io/router.tls.certresolver` et son bloc `tls` n'a pas de `secretName` : Traefik garde le certificat dans son propre stockage. Il n'existe pas d'objet `Certificate` à consulter ; regardez l'`Ingress` et les journaux de Traefik. |
| vide | un `ClusterIssuer` existant | cert-manager émet le certificat dans le Secret `ingress.tlsSecretName` (par défaut `ferrisgit-tls`). Suivez-le avec `kubectl get certificate -n ferrisgit`. |
| vide | vide | Aucun certificat n'est demandé : le Secret `ingress.tlsSecretName` doit déjà exister. |

Renseigner les deux valeurs fait échouer le rendu avec un message explicite. Le résolveur de Traefik est le défaut parce que, dans le cluster d'où vient ce chart, le HTTP-01 de cert-manager ne fonctionne pas : Traefik intercepte lui-même `/.well-known/acme-challenge/` sur le point d'entrée `web` pour son résolveur. Pour passer à cert-manager, ajoutez ces deux options à votre commande `helm upgrade` (ou, pour la CI, renseignez la variable GitHub `FERRISGIT_CLUSTER_ISSUER`) : `--set ingress.certResolver= --set ingress.clusterIssuer=letsencrypt-prod`.

Le résolveur ne vaut qu'avec `ingress.className=traefik` : avec une autre classe, le rendu échoue tant que `ingress.certResolver` n'est pas vide.

## Les mises à jour par la CI

Quand vous poussez un tag `vX.Y.Z` dont le commit est sur `main`, le workflow `.github/workflows/ci-cd.yml` construit et analyse l'image, la publie sur Docker Hub, puis son job **Deploy to Kubernetes** met la release `ferrisgit` à jour :

```bash
helm upgrade --install ferrisgit ./helm/ferrisgit \
  --namespace ferrisgit --create-namespace \
  --values values-ci.json \
  --set image.repository=masmarino/ferrisgit \
  --set image.tag=<version> --set image.digest=<sha256:…> \
  --atomic --cleanup-on-fail --timeout 10m
```

- **Le digest.** Le déploiement est épinglé au digest de l'image que le job de construction vient de pousser : une étiquette déplacée ensuite ne change rien. Sans identifiants Docker Hub (`DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN`), il n'y a pas de digest et le job s'arrête.
- **`--atomic`.** Si la mise à jour échoue (Pod qui ne devient pas prêt en 10 minutes, migration en erreur), Helm revient à la release précédente. Cela ne défait pas les migrations déjà appliquées : voir [Revenir en arrière](/docs/administration/deploiement-kubernetes#revenir-en-arrière).
- **La concurrence.** Deux déploiements ne tournent jamais en même temps, et un déploiement en cours n'est jamais annulé à mi-chemin.
- **Le tag sur `main`.** Le commit du tag doit être sur `main`, vérifié avant la publication de l'image puis de nouveau avant le déploiement. Le job utilise l'environnement GitHub `production` : vous pouvez y exiger une validation manuelle.
- **Ce qu'il faut configurer dans GitHub.** Le secret `KUBE_CONFIG` : un kubeconfig encodé en base64, dont le compte peut ce que décrit la section suivante. La variable `TRUSTED_PROXY_CIDRS` (obligatoire : le déploiement s'arrête sans elle). Les variables facultatives `FERRISGIT_INGRESS_HOST`, `FERRISGIT_CERT_RESOLVER` et `FERRISGIT_CLUSTER_ISSUER` ; renseigner `FERRISGIT_CLUSTER_ISSUER` passe en mode cert-manager, et renseigner les deux est refusé. Tout le reste vient de `values.yaml` à ce commit.
- **Le contrôle du chart.** Le job **Helm chart lint** passe `helm lint --strict` et `helm template` sur le chart à chaque poussée et chaque demande de fusion, avec les valeurs par défaut, avec cert-manager, et avec plusieurs options désactivées ; il vérifie aussi que renseigner les deux modes de certificat est refusé.

Une mise à jour coupe le service le temps du remplacement : la stratégie `Recreate` arrête l'ancien Pod avant de démarrer le nouveau, car les volumes sont `ReadWriteOnce`. Un déploiement qui échoue après l'arrêt de l'ancien Pod laisse le service arrêté jusqu'au retour arrière de `--atomic`. Gardez une seule réplique : les dépôts Git vivent sur un volume local, et les limites de débit sont comptées en mémoire par processus.

## Revenir en arrière

`helm rollback`, `--atomic` ou le redéploiement d'une ancienne étiquette **n'annulent pas les migrations**. Le serveur applique à chaque démarrage les migrations qu'il embarque (`sqlx::migrate!`), sans option pour tolérer les inconnues : une base qui enregistre une migration que la version ne connaît pas fait échouer le démarrage, avec `failed to run migrations: VersionMissing(<numéro>)` dans les journaux, et le Pod redémarre en boucle. C'est aussi le cas du retour arrière de `--atomic` quand la nouvelle version a échoué après avoir appliqué ses migrations. Un fichier de migration modifié après coup donne `VersionMismatch`.

- Si la nouvelle version n'a appliqué aucune migration (voir les notes de la version dans le CHANGELOG), `helm rollback ferrisgit` ou le redéploiement de l'ancienne étiquette suffit.
- Sinon, restaurez la sauvegarde de la base faite avant la mise à jour, avec le stockage correspondant, puis déployez l'ancienne version. Ce qui a été écrit depuis la mise à jour est perdu. Voir [Sauvegardes et mises à jour](/docs/administration/sauvegardes-et-mises-a-jour).

Sauvegardez donc avant chaque version qui apporte une migration, y compris quand la CI déploie seule.

## Désinstaller

`helm uninstall ferrisgit -n ferrisgit` supprime les ressources du chart, sauf le Secret `ferrisgit-secrets` et les deux volumes `ferrisgit-storage` et `ferrisgit-postgres-data` (`persistence.keepOnUninstall`). Une réinstallation les retrouve et reprend où vous en étiez. Pour effacer les données, supprimez-les à la main, volumes d'abord, Secret ensuite :

```bash
kubectl -n ferrisgit delete pvc ferrisgit-storage ferrisgit-postgres-data
kubectl -n ferrisgit delete secret ferrisgit-secrets
```

Avec `persistence.keepOnUninstall=false`, la désinstallation supprime les volumes et tout ce qu'ils contiennent. Les volumes de cache des jobs (créés par FerrisGit, pas par le chart) ne sont jamais supprimés automatiquement.

## Les droits du moteur d'exécution

Le `Role` `ferrisgit-ci` donne au Pod du serveur, dans le seul namespace des jobs : `get`, `list`, `watch`, `create` et `delete` sur les Pods et leurs journaux, et `get`, `list` et `create` sur les `PersistentVolumeClaim` (les caches). Il ne donne aucun accès aux Secrets. Un `ClusterRole` en lecture seule (`get` et `list` sur les `StorageClass`) sert seulement à pré-remplir le réglage de la classe de cache ; `kubernetesExecutor.detectStorageClass=false` ne le crée pas. Ces droits ne servent que si vous choisissez le moteur Kubernetes dans [Réglages de l'instance](/docs/administration/reglages). Pour le détail du moteur, voir [Moteur Kubernetes](/docs/ci-cd/kubernetes).

## Les droits de la CI

Pour que la CI déploie, donnez-lui un compte de service dédié, sans `cluster-admin`. Voici ce dont `helm upgrade --install --atomic` a besoin dans le namespace `ferrisgit`, d'après les ressources que les modèles du chart créent. Le `Secret` est la ressource sensible : Helm y stocke ses releases (`sh.helm.release.v1.*`) et le chart y lit `ferrisgit-secrets` avec `lookup`, donc ce compte peut lire les secrets du namespace.

```yaml
apiVersion: v1
kind: ServiceAccount
metadata: { name: ferrisgit-deployer, namespace: ferrisgit }
---
apiVersion: rbac.authorization.k8s.io/v1
kind: Role
metadata: { name: ferrisgit-deployer, namespace: ferrisgit }
rules:
  # Les releases Helm, et le Secret de l'application (lookup, mise à jour).
  - apiGroups: [""]
    resources: ["secrets"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
  - apiGroups: [""]
    resources: ["services", "serviceaccounts"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
  # Pas de delete : un faux pas de la CI ne supprime pas de volume. Les volumes portent de toute façon
  # helm.sh/resource-policy: keep.
  - apiGroups: [""]
    resources: ["persistentvolumeclaims"]
    verbs: ["get", "list", "create", "update", "patch"]
  # Attente de --atomic, et droits que le Role ferrisgit-ci accorde : Kubernetes interdit de créer un
  # rôle plus puissant que le sien.
  - apiGroups: [""]
    resources: ["pods", "pods/log"]
    verbs: ["get", "list", "watch", "create", "delete"]
  - apiGroups: ["apps"]
    resources: ["deployments"]
    verbs: ["get", "list", "watch", "create", "update", "patch", "delete"]
  - apiGroups: ["apps"]
    resources: ["replicasets"]
    verbs: ["get", "list", "watch"]
  - apiGroups: ["networking.k8s.io"]
    resources: ["ingresses", "networkpolicies"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
  - apiGroups: ["rbac.authorization.k8s.io"]
    resources: ["roles", "rolebindings"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: RoleBinding
metadata: { name: ferrisgit-deployer, namespace: ferrisgit }
subjects:
  - { kind: ServiceAccount, name: ferrisgit-deployer, namespace: ferrisgit }
roleRef: { kind: Role, name: ferrisgit-deployer, apiGroup: rbac.authorization.k8s.io }
```

Avec `kubernetesExecutor.detectStorageClass=true` (le défaut), le chart crée aussi un `ClusterRole` et un `ClusterRoleBinding`, ressources du cluster entier, et pour la même raison de non-escalade le compte doit pouvoir lire les `StorageClass`. Soit vous ajoutez ceci, soit vous mettez `detectStorageClass: false` dans `values.yaml` (le réglage de la classe de cache n'est alors plus pré-rempli) :

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata: { name: ferrisgit-deployer }
rules:
  - apiGroups: ["rbac.authorization.k8s.io"]
    resources: ["clusterroles", "clusterrolebindings"]
    verbs: ["get", "list", "create", "update", "patch", "delete"]
  - apiGroups: ["storage.k8s.io"]
    resources: ["storageclasses"]
    verbs: ["get", "list"]
---
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRoleBinding
metadata: { name: ferrisgit-deployer }
subjects:
  - { kind: ServiceAccount, name: ferrisgit-deployer, namespace: ferrisgit }
roleRef: { kind: ClusterRole, name: ferrisgit-deployer, apiGroup: rbac.authorization.k8s.io }
```

Trois précisions :

- Le namespace `ferrisgit` doit exister avant le premier déploiement. L'option `--create-namespace` de la CI demande de créer un namespace, un droit du cluster entier que ce compte n'a pas : faites la première installation vous-même, avec un compte administrateur. Les mises à jour suivantes n'ont pas besoin de ce droit.
- Si `kubernetesExecutor.namespace` désigne un autre namespace, le compte doit aussi pouvoir gérer les `Role` et `RoleBinding` dans celui-ci, avec les droits sur les Pods et les PVC ci-dessus.
- Ces droits sont déduits des modèles du chart et du comportement de Helm ; ils n'ont pas été éprouvés sur un cluster. Au premier déploiement, un message `forbidden` de Helm indique le verbe manquant : ajoutez-le.

Construisez ensuite un kubeconfig qui utilise le jeton de ce compte (par exemple celui d'un Secret de type `kubernetes.io/service-account-token`), encodez-le en base64 et placez-le dans le secret GitHub `KUBE_CONFIG`.

## Migrer depuis les anciens manifestes

Les anciens manifestes `k8s/` créaient dans le namespace `ferrisgit` des ressources dont certaines portent le même nom que celles du chart, d'autres non. Le tableau compare les noms réels (relevés dans les anciens manifestes et dans la sortie de `helm template ferrisgit ./helm/ferrisgit --namespace ferrisgit …`) :

| Ressource | Ancien nom | Nom du chart | Que faire |
|---|---|---|---|
| `Secret` | `ferrisgit-secrets` | `ferrisgit-secrets` | **Adopter** : il contient les clés qui déchiffrent vos données. |
| `PersistentVolumeClaim` | `ferrisgit-storage` | `ferrisgit-storage` | **Adopter** : ce sont vos dépôts. |
| `PersistentVolumeClaim` | `ferrisgit-postgres-data` | `ferrisgit-postgres-data` | **Adopter** : c'est votre base. |
| `Deployment` du serveur | `ferrisgit-server` | `ferrisgit` | Supprimer : le nom et le sélecteur changent, un `Deployment` ne peut pas être adopté. |
| `Service` du serveur | `ferrisgit-server` | `ferrisgit` | Supprimer. |
| `Deployment` de la base | `ferrisgit-postgres` | `ferrisgit-postgres` | Supprimer : même nom, mais le sélecteur (`app: ferrisgit-postgres`) change et Kubernetes ne le permet pas. |
| `Service` de la base | `ferrisgit-postgres` | `ferrisgit-postgres` | Supprimer. |
| `ServiceAccount`, `Role`, `RoleBinding` | `ferrisgit-ci` | `ferrisgit-ci` | Supprimer (sans état). |
| `Ingress` | `ferrisgit` | `ferrisgit` | Supprimer. |

Les noms du Secret et des volumes coïncident parce que la release s'appelle `ferrisgit`. Avec un autre nom de release, ils deviendraient `<release>-secrets`, `<release>-storage` et `<release>-postgres-data` et ne retrouveraient plus vos données : gardez `ferrisgit`.

> **Attention** : n'utilisez pas `kubectl delete -f` avec les anciens manifestes. Ils contiennent les volumes et le namespace, et supprimer un volume supprime vos données. Supprimez les ressources une par une, comme ci-dessous.

La procédure coupe le service entre l'étape 4 et la fin. Les données ne sont pas touchées : les volumes et le Secret ne sont jamais supprimés.

1. **Sauvegardez**, et gardez le résultat hors du cluster :

   ```bash
   kubectl -n ferrisgit get secret ferrisgit-secrets -o yaml > ferrisgit-secrets.yaml
   kubectl -n ferrisgit exec deploy/ferrisgit-postgres -- pg_dump -U ferrisgit -Fc ferrisgit > ferrisgit.dump
   kubectl -n ferrisgit exec deploy/ferrisgit-server -- tar czf - -C /data . > ferrisgit-storage.tgz
   ```

2. **Relevez ce que l'ancien déploiement utilisait** :

   ```bash
   kubectl -n ferrisgit get deploy ferrisgit-server \
     -o jsonpath='{range .spec.template.spec.containers[0].env[*]}{.name}={.value}{"\n"}{end}'
   kubectl -n ferrisgit get pvc ferrisgit-storage ferrisgit-postgres-data \
     -o custom-columns=NOM:.metadata.name,TAILLE:.spec.resources.requests.storage,CLASSE:.spec.storageClassName
   ```

   Notez `TRUSTED_PROXY_CIDRS` et `PUBLIC_URL` (l'hôte), ainsi que la taille et la classe de chaque volume. Vérifiez aussi la version de l'image qui tourne : déployez une version égale ou plus récente, jamais plus ancienne (voir [Revenir en arrière](/docs/administration/deploiement-kubernetes#revenir-en-arrière)).

3. **Contrôlez les noms que le chart produit**, avec les valeurs que vous allez utiliser :

   ```bash
   helm template ferrisgit ./helm/ferrisgit --namespace ferrisgit \
     --set image.tag=0.1.1 --set-string ferrisgit.trustedProxyCidrs=<TRUSTED_PROXY_CIDRS> \
     | grep -E '^kind:|^  name:'
   ```

   Le Secret et les volumes doivent s'appeler `ferrisgit-secrets`, `ferrisgit-storage` et `ferrisgit-postgres-data`.

4. **Supprimez les anciennes charges** (le service s'arrête ici). Supprimer un `Deployment` ne supprime ni les volumes ni le Secret :

   ```bash
   kubectl -n ferrisgit delete deployment ferrisgit-server ferrisgit-postgres
   kubectl -n ferrisgit delete service ferrisgit-server ferrisgit-postgres
   kubectl -n ferrisgit delete ingress ferrisgit
   kubectl -n ferrisgit delete rolebinding,role,serviceaccount ferrisgit-ci
   kubectl -n ferrisgit wait --for=delete pod -l 'app in (ferrisgit-server, ferrisgit-postgres)' --timeout=120s
   ```

5. **Adoptez le Secret et les deux volumes** : Helm n'accepte de reprendre une ressource existante que si elle porte son label et ses annotations.

   ```bash
   for r in secret/ferrisgit-secrets pvc/ferrisgit-storage pvc/ferrisgit-postgres-data; do
     kubectl -n ferrisgit label "$r" app.kubernetes.io/managed-by=Helm --overwrite
     kubectl -n ferrisgit annotate "$r" \
       meta.helm.sh/release-name=ferrisgit meta.helm.sh/release-namespace=ferrisgit --overwrite
   done
   ```

6. **Installez le chart**, avec des volumes de la même taille et de la même classe que l'existant : Helm applique la spécification rendue sur le volume adopté, et un volume ne peut pas être réduit, ni agrandi si sa classe ne le permet pas. Reprenez les valeurs relevées à l'étape 2 (les anciens volumes faisaient 10 Gio et 5 Gio, contre 20 Gio et 10 Gio par défaut dans le chart) :

   ```bash
   helm upgrade --install ferrisgit ./helm/ferrisgit --namespace ferrisgit \
     --set image.tag=0.1.1 \
     --set ingress.host=<hôte> \
     --set-string ferrisgit.trustedProxyCidrs=<TRUSTED_PROXY_CIDRS> \
     --set storage.size=10Gi --set postgres.storage.size=5Gi \
     --set storage.storageClassName=<classe> --set postgres.storage.storageClassName=<classe> \
     --atomic --timeout 10m
   ```

   Le chart retrouve les valeurs du Secret adopté avec `lookup` et n'y ajoute que `DATABASE_URL`. Si une clé manque dans l'ancien Secret, le chart en génère une ; si `BOOTSTRAP_ADMIN_PASSWORD` fait moins de 8 caractères, il refuse : passez `--set secrets.bootstrapAdminPassword=…` (le serveur n'en tient compte que sur une base sans utilisateur). L'ancienne image venait d'un registre privé : le chart utilise `masmarino/ferrisgit` sur Docker Hub ; pour un autre registre, ajoutez `--set image.repository=…` et `imagePullSecrets`.

7. **Vérifiez que le Secret n'a pas changé**, en comparant avec la sauvegarde de l'étape 1 :

   ```bash
   for k in POSTGRES_PASSWORD JWT_SECRET SETTINGS_ENCRYPTION_KEY; do
     now=$(kubectl -n ferrisgit get secret ferrisgit-secrets -o jsonpath="{.data.$k}")
     grep -q "^  $k: $now$" ferrisgit-secrets.yaml && echo "$k : identique" || echo "$k : DIFFÉRENT"
   done
   ```

   Les trois lignes doivent dire « identique ». Sinon, ne laissez pas l'application tourner : remettez le Secret d'origine (`kubectl apply -f ferrisgit-secrets.yaml`, après avoir retiré les champs `resourceVersion` et `uid`) et relancez le déploiement.

8. **Contrôlez le service** : `kubectl -n ferrisgit rollout status deployment/ferrisgit`, connexion avec un compte existant, clone d'un dépôt, et la page **Admin**, **Santé**. Le compte administrateur de départ n'est pas recréé : le serveur l'ignore dès que la table des utilisateurs n'est plus vide.

Si l'installation échoue avant que les données aient été touchées, revenez en arrière sans perte : `helm uninstall ferrisgit -n ferrisgit` laisse le Secret et les volumes en place, puis réappliquez les anciens manifestes depuis l'historique git du dépôt (avant la suppression de `k8s/`).

Les prérequis du cluster (middlewares et `TLSOption` Traefik) comptent aussi pour cette migration : l'ancien `Ingress` n'utilisait que le middleware `headers` et aucun `TLSOption`, le chart ajoute `https` et `mintls13`.
