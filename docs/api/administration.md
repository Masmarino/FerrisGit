# Administration

Cette page décrit les routes réservées aux administrateurs : comptes utilisateurs, réglages de l'instance, SMTP, statistiques et santé. Elles demandent toutes un JWT d'administrateur (voir [Authentification](/docs/api/authentification)). Un utilisateur qui n'est pas administrateur reçoit `401` avec `{ "error": "admin access required" }`. Le statut d'administrateur est relu en base à chaque requête : une rétrogradation prend effet immédiatement, sans révoquer les sessions.

Les routes d'administration des runners (`/api/admin/runners`) sont décrites avec les pipelines, dans [Pipelines et runners](/docs/api/ci-cd). Pour l'usage dans l'interface, voir [Utilisateurs et invitations](/docs/administration/utilisateurs) et [Réglages de l'instance](/docs/administration/reglages).

> **Note** : le serveur enregistre des événements de sécurité (connexions, échecs, actions d'administration), mais aucune route de l'API ne permet de consulter ce journal.

## Utilisateurs

Un compte invité mais pas encore activé a l'état `invited`. Une ligne d'utilisateur de la liste a cette forme :

```json
{
  "id": "0b6c…",
  "username": "alice",
  "email": "alice@example.com",
  "isAdmin": false,
  "createdAt": "2026-03-01T10:00:00Z",
  "state": "active",
  "invitationExpiresAt": null,
  "mfaEnabled": true
}
```

`state` vaut `active` ou `invited`. `invitationExpiresAt` n'est renseigné que pour un compte invité. `mfaEnabled` dit si le compte a un TOTP confirmé ou au moins une passkey.

### `GET /api/admin/users`

Liste tous les comptes, sans pagination, dans la limite de 1000 lignes. Réponse 200 : un tableau de lignes.

### `POST /api/admin/users`

Crée un compte directement, avec un mot de passe que vous choisissez. Le compte n'est pas administrateur et devra enrôler un second facteur à sa première connexion. Pour qu'il choisisse lui-même son mot de passe, utilisez plutôt l'invitation.

Corps (tous les champs sont obligatoires) :

| Champ | Type | Description |
|---|---|---|
| `username` | texte | Lettres, chiffres, `-` et `_`. |
| `email` | texte | Adresse e-mail. |
| `password` | texte | 8 caractères au minimum. |

Réponse 200 : `{ "id", "username", "email", "isAdmin", "createdAt" }`.

Erreurs : 400 `password must be at least 8 characters`, 400 `username must be non-empty and alphanumeric/-/_ only`, 409 `username already taken`, 409 `username collides with an existing root group`.

### `POST /api/admin/users/invite`

Invite une personne par son adresse e-mail. Le compte est créé sans nom ni mot de passe utilisable, et un lien d'activation valable 24 heures est envoyé. Le lien est de la forme `<PUBLIC_URL>/invitation#token=…` : la personne y choisit son nom d'utilisateur et son mot de passe (voir [`POST /api/auth/activate`](/docs/api/authentification)).

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `email` | texte | oui | Adresse e-mail valide. |
| `isAdmin` | booléen | non | Crée un administrateur. `false` par défaut. |

Réponse 200 :

```json
{
  "user": { "id": "…", "username": null, "email": "carol@example.com", "isAdmin": false, "createdAt": "…", "state": "invited", "invitationExpiresAt": "2026-03-02T10:00:00Z", "mfaEnabled": false },
  "emailSent": true
}
```

Si l'e-mail n'a pas pu partir (SMTP absent ou en panne), la route répond quand même 200 avec `emailSent: false`, `emailError` (la raison) et `activationUrl`, que vous transmettez par un autre moyen. Ces deux champs n'existent que dans ce cas. Configurez le SMTP avec `PUT /api/admin/settings/smtp`.

`username` vaut `null` tant que la personne n'a pas choisi son nom, ici comme dans la liste des utilisateurs.

Erreurs : 400 adresse invalide, 409 `email already in use`.

```bash
curl -s -X POST "$BASE/api/admin/users/invite" -H "Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' \
  -d '{"email":"carol@example.com"}'
```

### `POST /api/admin/users/{id}/invitation`

Renvoie une invitation à un compte qui n'a pas encore activé le sien. Le lien précédent cesse de fonctionner. Réponse 200 : la même forme que `invite`.

Erreurs : 400 `user is already active`, 404 `user`.

### `POST /api/admin/users/{id}/reset-password`

Réinitialise le mot de passe d'un compte actif. Les sessions du compte sont révoquées et son mot de passe actuel cesse de marcher tout de suite. Un e-mail lui envoie un lien valable 1 heure (`<PUBLIC_URL>/reset-password#token=…`), qui sert à `POST /api/auth/reset-password`. Les facteurs de double authentification ne changent pas.

Réponse 200 : `{ "emailSent": true }`. Si l'e-mail n'a pas pu partir, ou si l'adresse du compte n'est pas une vraie boîte (celle de l'administrateur initial, en `@localhost`, par exemple), la réponse ajoute `emailError` et `resetUrl` à transmettre vous-même.

Erreurs : 400 `use your account settings to change your own password` (votre propre compte), 400 `the user has not activated their account yet; resend the invitation instead`, 404 `user`.

### `DELETE /api/admin/users/{id}/mfa`

Supprime tous les facteurs d'un compte (TOTP, passkeys, codes de secours) et révoque ses sessions et ses jetons MFA en attente. À sa prochaine connexion, il devra enrôler un nouveau facteur. Un e-mail le prévient. Réponse 204. La route est idempotente. Erreur : 404 `user`.

### `PUT /api/admin/users/{id}/admin`

Accorde ou retire le statut d'administrateur, y compris à vous-même.

Corps : `{ "isAdmin": true }`. Réponse 204, même si la valeur ne change pas.

Erreurs : 404 `user`, 409 `cannot remove the last administrator` (il doit toujours rester un administrateur actif ; un compte invité ou en attente de réinitialisation ne compte pas).

### `GET /api/admin/users/{id}/repositories`

Liste les dépôts personnels d'un utilisateur, sans jamais lire leur contenu. Les dépôts de groupe qu'il a créés n'y figurent pas.

Réponse 200 :

```json
[ { "id": "…", "name": "blog", "description": "…", "visibility": "private", "createdAt": "…", "sizeBytes": 204800 } ]
```

`sizeBytes` vaut `null` si la taille n'a pas pu être calculée. Erreur : 404 `user`.

### `DELETE /api/admin/users/{id}`

Supprime un compte et tout ce qu'il possède : dépôts personnels (avec leurs fichiers), jetons, appartenances, facteurs, étoiles, notifications, ainsi que les tickets, commentaires, relectures et pipelines qu'il a créés. Ce qu'il a écrit ailleurs (demandes de fusion et commentaires sur les dépôts des autres, releases) reste, attribué à un utilisateur supprimé. Un dépôt de groupe qu'il avait créé reste au groupe et vous est confié.

Réponse 204. Erreurs : 400 `you cannot delete your own account`, 404 `user`, 409 `cannot remove the last administrator`, 409 `the user is the last maintainer of the group <chemin>; promote another member first`.

## Réglages de l'instance

Les réglages ont leurs valeurs par défaut tant qu'un administrateur ne les change pas. Voir [Réglages de l'instance](/docs/administration/reglages).

### `GET /api/admin/settings`

Renvoie les réglages.

Réponse 200 :

```json
{
  "executionEngine": "docker-runners",
  "k8sNamespace": null,
  "k8sCacheStorageClass": null,
  "runnerRegistrationTokenConfigured": false,
  "logRetentionDays": null,
  "maxConcurrentJobs": null,
  "jwtTtlHours": 12,
  "maxPushSizeMb": 500,
  "registrationEnabled": false,
  "publicPagesEnabled": true,
  "seoIndexingEnabled": false,
  "detectedK8sNamespace": null,
  "detectedK8sDefaultStorageClass": null
}
```

| Champ | Description |
|---|---|
| `executionEngine` | Moteur d'exécution des jobs : `docker-runners` (défaut) ou `kubernetes`. |
| `k8sNamespace`, `k8sCacheStorageClass` | Réglages du moteur Kubernetes. `null` si non définis. |
| `runnerRegistrationTokenConfigured` | Un jeton d'enregistrement de runners est défini. Sa valeur n'est jamais renvoyée. |
| `logRetentionDays` | Durée de conservation, en jours, des journaux des jobs terminés. `null` : les journaux sont conservés sans limite (défaut). |
| `maxConcurrentJobs` | Plafond de jobs « en cours » à la fois, appliqué quand un runner Docker demande un job. `null` : pas de plafond (défaut). Sans effet sur le moteur Kubernetes. |
| `jwtTtlHours` | Durée de vie des JWT de session, en heures (12 par défaut). |
| `maxPushSizeMb` | Taille maximale d'un `git push`, en Mio (500 par défaut). |
| `registrationEnabled` | Inscription libre ouverte (`false` par défaut). |
| `publicPagesEnabled` | Pages publiques activées (`true` par défaut). |
| `seoIndexingEnabled` | Indexation par les moteurs de recherche autorisée (`false` par défaut). Elle n'a d'effet que si les pages publiques sont activées. |
| `detectedK8sNamespace`, `detectedK8sDefaultStorageClass` | Valeurs détectées au démarrage, pour pré-remplir les réglages Kubernetes. |

### `PUT /api/admin/settings`

Modifie les réglages. Tous les champs sont facultatifs : un champ absent garde sa valeur.

Les champs `k8sNamespace`, `k8sCacheStorageClass`, `runnerRegistrationToken`, `logRetentionDays` et `maxConcurrentJobs` acceptent `null`, qui efface la valeur. `runnerRegistrationToken` prend en entrée le jeton en clair ; le serveur n'en garde que l'empreinte. Les autres champs sont ceux du tableau ci-dessus, sauf `runnerRegistrationTokenConfigured`, `detectedK8sNamespace` et `detectedK8sDefaultStorageClass`, qui sont en lecture seule.

Un nouveau `jwtTtlHours` s'applique aux JWT émis ensuite, sans toucher aux sessions en cours.

`logRetentionDays` et `maxConcurrentJobs` sont des entiers de 1 ou plus. `logRetentionDays` est appliqué par un balayage qui s'exécute au démarrage du serveur, puis toutes les 24 heures : le texte des journaux des jobs terminés depuis plus longtemps est effacé (les pipelines, les jobs et leurs statuts restent, et `GET /api/pipelines/{id}` renseigne alors `logsPurgedAt`, voir [Pipelines et runners](/docs/api/ci-cd)). `runnerRegistrationToken` ne peut pas être une chaîne vide ; `null` supprime le jeton et désactive l'auto-enregistrement des runners.

Réponse 200 : les réglages à jour, comme `GET`.

Erreurs : 400 `jwt_ttl_hours must be between 1 and 720`, 400 `unknown execution engine: …`, 400 `log_retention_days must be at least 1`, 400 `max_concurrent_jobs must be at least 1`, 400 `runner_registration_token must not be empty`.

```bash
curl -s -X PUT "$BASE/api/admin/settings" -H "Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' \
  -d '{"registrationEnabled":true,"maxConcurrentJobs":4}'
```

### `GET /api/admin/settings/smtp`

Renvoie la configuration SMTP. Le mot de passe n'est jamais renvoyé, seulement `passwordSet`.

Réponse 200 :

```json
{
  "configured": true,
  "host": "smtp.example.com",
  "port": 587,
  "security": "starttls",
  "username": "ferrisgit",
  "passwordSet": true,
  "fromAddress": "git@example.com",
  "fromName": "FerrisGit"
}
```

Sans configuration, `configured` vaut `false` et les autres champs prennent leurs valeurs par défaut (hôte vide, port 587, `starttls`, nom d'expéditeur `FerrisGit`).

### `PUT /api/admin/settings/smtp`

Enregistre la configuration SMTP, qui remplace la précédente.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `host` | texte | oui | Nom d'hôte ou adresse IP, 253 caractères au plus, sans espace. |
| `port` | entier | oui | De 1 à 65535. |
| `security` | texte | oui | `none`, `starttls` ou `tls`. |
| `fromAddress` | texte | oui | Adresse d'expéditeur valide. |
| `username` | texte | non | Identifiant. Vide : pas d'authentification. |
| `password` | texte | non | Mot de passe. Absent ou vide, il garde le mot de passe déjà enregistré. |
| `fromName` | texte | non | Nom d'expéditeur, 100 caractères au plus. `FerrisGit` si vide. |

Un identifiant exige un mot de passe (celui du corps, ou celui déjà enregistré). Le mot de passe est stocké chiffré.

Réponse 200 : la configuration, comme `GET`. Erreurs : 400 `host must be a hostname or an IP address`, 400 `port must be between 1 and 65535`, 400 `unknown SMTP security mode: …`, 400 `from_address must be a valid e-mail address`, 400 `from_name is too long`, 400 `a password is required when a username is set`.

### `POST /api/admin/settings/smtp/test`

Envoie un e-mail de test avec la configuration enregistrée.

Corps : `{ "to": "vous@example.com" }`.

Réponse 200 : `{ "sent": true }`. Un échec d'envoi (identifiants refusés, serveur injoignable, SMTP non configuré) n'est pas une erreur HTTP : la réponse est `200` avec `{ "sent": false, "error": "…" }`, car révéler ce problème est le but de la route. Seule une adresse de destinataire invalide répond 400 `recipient must be a valid e-mail address`.

## Statistiques et santé

### `GET /api/admin/stats`

Chiffres globaux. Réponse 200 :

```json
{ "totalUsers": 12, "totalRepositories": 48, "pipelinesLast7Days": 130 }
```

### `GET /api/admin/metrics/history`

Historique des chiffres globaux. Le serveur enregistre un instantané toutes les heures.

Paramètre de requête : `days`, la profondeur en jours (30 par défaut, ramené entre 1 et 365).

Réponse 200 :

```json
[ { "recordedAt": "2026-03-01T10:00:00Z", "totalUsers": 12, "totalRepositories": 48, "totalStorageBytes": 1073741824 } ]
```

### `GET /api/admin/health`

État de la base de données et du stockage.

Réponse 200 :

```json
{
  "database": { "status": "up", "detail": null, "responseTimeMs": 3, "activeConnections": 2, "maxConnections": 10, "serverVersion": "PostgreSQL 18.0" },
  "storage": { "status": "up", "detail": null, "usedBytes": 1073741824, "freeBytes": 53687091200, "totalBytes": 107374182400 },
  "uptimeSeconds": 86400
}
```

`status` vaut `up` ou `down` ; en cas de `down`, `detail` donne la raison. `serverVersion` peut être `null`. La route répond 200 même quand un composant est `down`.

### `GET /health`

Sonde de disponibilité du serveur, hors de `/api`. Route anonyme. Elle répond `200` avec le texte brut `ok`, sans toucher à la base de données : elle prouve que le processus répond, pas que ses dépendances vont bien. Pour l'état de la base et du stockage, utilisez `GET /api/admin/health`.

### `GET /healthz`

Sonde de vivacité du serveur, hors de `/api`. Même réponse que `GET /health` : `200` avec le texte brut `ok` tant que le processus répond, sans toucher à la base de données. Route anonyme, sans limitation de débit. Comme elle ignore la base, elle reste à `200` pendant une panne de celle-ci : une sonde de vivacité Kubernetes ne redémarre donc pas le Pod pour ça. Le chart Helm l'utilise pour les sondes de démarrage et de vivacité.

```bash
curl -i https://git.example.com/healthz
```

### `GET /readyz`

Sonde de disponibilité du serveur, hors de `/api`. Route anonyme, sans limitation de débit. Elle envoie une requête triviale à la base de données et attend la réponse 2 secondes au plus :

- `200` avec le texte brut `ok` si la base répond dans ce délai ;
- `503` avec le texte brut `unavailable` si elle est injoignable, en erreur ou trop lente. La réponse ne donne aucun détail sur la cause ; pour le détail de la base, voyez `GET /api/admin/health` ci-dessus.

C'est la route à brancher sur une sonde de disponibilité : un Pod dont la base ne répond pas sort du `Service` sans être redémarré. Le chart Helm l'utilise.

```bash
curl -i https://git.example.com/readyz
```
