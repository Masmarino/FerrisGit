# Authentification

Cette page décrit le parcours de connexion par l'API, la double authentification, la gestion du compte et les jetons Git.

## Le parcours de connexion

La double authentification est obligatoire : un mot de passe seul ne donne jamais de session. Le parcours a toujours deux temps.

1. `POST /api/auth/login` vérifie le mot de passe et répond avec un **jeton MFA en attente** (`mfaToken`), pas avec une session.
2. Vous présentez ce jeton à une route de vérification du second facteur. Elle répond avec le **JWT de session** (`token`).

La réponse de l'étape 1 indique le chemin à suivre :

| `mfaSetupRequired` | `mfaHasTotp` | `mfaHasPasskey` | Ce qu'il faut faire |
|---|---|---|---|
| `false` | `true` | `false` | Vérifier un code TOTP ou un code de secours : `POST /api/auth/mfa/verify`. |
| `false` | `false` | `true` | Vérifier une passkey : `POST /api/auth/mfa/passkey/start` puis `finish`. Un code de secours reste possible via `verify`. |
| `false` | `true` | `true` | Au choix, l'un des deux chemins ci-dessus. |
| `true` | `false` | `false` | Le compte n'a aucun second facteur. Il faut en enrôler un (TOTP ou passkey) avant d'avoir une session. |

Le jeton MFA vit 5 minutes. Il n'est consommé que par un succès : une mauvaise réponse ne le brûle pas, mais elle compte dans la limite de 10 tentatives par 5 minutes et par compte. Un jeton déjà utilisé, expiré, falsifié ou rendu caduc par un changement de mot de passe répond toujours la même erreur : `401 invalid or expired token`.

L'enrôlement obligatoire suit les mêmes règles pour un compte qui vient d'être activé (par invitation ou après une inscription) : la première connexion renvoie un jeton MFA avec `mfaSetupRequired: true`.

> **Note** : un script ne peut pas utiliser une passkey, qui exige un navigateur ou une clé matérielle. Pour l'automatisation, enrôlez un TOTP et calculez le code avec un outil compatible (`oathtool`, par exemple).

## Exemple de bout en bout avec un compte TOTP

L'exemple suppose que le compte a déjà un TOTP, dont vous avez gardé le secret en base32 (`TOTP_SECRET`). Un code TOTP n'est accepté qu'une fois : un code déjà utilisé dans la même fenêtre de 30 secondes est refusé, il faut alors attendre la fenêtre suivante.

```bash
BASE=https://git.example.com

# 1. Mot de passe : on obtient un jeton MFA
MFA_TOKEN=$(curl -s -X POST "$BASE/api/auth/login" \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"mot-de-passe-d-alice"}' | jq -r .mfaToken)

# 2. Second facteur : on obtient le JWT de session
CODE=$(oathtool --totp -b "$TOTP_SECRET")
JWT=$(curl -s -X POST "$BASE/api/auth/mfa/verify" \
  -H 'Content-Type: application/json' \
  -d "{\"mfaToken\":\"$MFA_TOKEN\",\"code\":\"$CODE\"}" | jq -r .token)

# 3. Appels authentifiés
curl -s "$BASE/api/auth/me" -H "Authorization: Bearer $JWT"
curl -s "$BASE/api/repositories" -H "Authorization: Bearer $JWT"
```

## Durée de vie, déconnexion et révocation

- Le JWT est valable 12 heures par défaut. L'administrateur règle cette durée entre 1 et 720 heures (`jwtTtlHours`, voir [`PUT /api/admin/settings`](/docs/api/administration)). Le nouveau réglage s'applique aux jetons émis ensuite.
- Il n'y a pas de route de déconnexion : se déconnecter, c'est oublier le JWT côté client.
- Chaque compte porte un compteur de révocation. Un JWT émis avant un changement de ce compteur répond `401 token has been revoked`. Le compteur change quand :
  - vous changez votre mot de passe (la route vous rend un nouveau JWT) ;
  - vous supprimez votre TOTP ou une passkey ;
  - un administrateur réinitialise votre mot de passe ou votre double authentification ;
  - le mot de passe est défini par un lien de réinitialisation.
- Un compte supprimé répond `401 user no longer exists`.

## Jetons Git

Un jeton Git, ou jeton d'accès personnel (préfixe `fg_`), sert uniquement à Git en HTTP, comme mot de passe de l'authentification Basic. Il n'ouvre **pas** l'API : l'extracteur d'authentification de l'API n'accepte que des JWT de session, et un `fg_…` répond `401 invalid or expired token`. Les routes qui créent et listent ces jetons demandent, elles, un JWT. Voir [Cloner et pousser](/docs/utilisation/cloner-et-pousser) et [Compte et sécurité](/docs/utilisation/compte-et-securite).

## Connexion

### `POST /api/auth/login`

Vérifie le mot de passe. Route anonyme, limitée à 10 requêtes par minute et par adresse IP.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `username` | texte | oui | Nom d'utilisateur. La casse et les espaces autour sont tolérés. |
| `password` | texte | oui | Mot de passe. |

Réponse 200 :

```json
{
  "token": null,
  "mfaToken": "eyJ…",
  "mfaSetupRequired": false,
  "mfaHasTotp": true,
  "mfaHasPasskey": false
}
```

`token` existe dans la réponse mais vaut toujours `null` : la session ne s'obtient qu'après le second facteur.

Erreurs : 401 `invalid username or password` (nom inconnu ou mot de passe faux, indistinguables ; c'est aussi la réponse pour un compte invité qui n'a pas encore activé son compte), 429 `too many login attempts, try again later`.

```bash
curl -s -X POST "$BASE/api/auth/login" -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"mot-de-passe-d-alice"}'
```

### `POST /api/auth/mfa/verify`

Termine la connexion avec un code TOTP ou un code de secours. Route anonyme, autorisée par le jeton MFA du corps.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `mfaToken` | texte | oui | Jeton reçu de `login`. |
| `code` | texte | l'un des deux | Code TOTP à 6 chiffres. |
| `backupCode` | texte | l'un des deux | Code de secours (32 caractères hexadécimaux). |

Envoyez exactement un des deux champs `code` et `backupCode`. Les deux ou aucun répondent 401 `invalid code`. Un code de secours est à usage unique ; les espaces et la casse sont ignorés. Un code de secours ne marche que si le compte a au moins un facteur.

Réponse 200 : `{ "token": "<JWT>" }`.

Erreurs : 401 `invalid or expired token` (jeton MFA), 401 `invalid code` (code faux, déjà utilisé ou sans facteur correspondant), 429 `too many attempts, try again later`.

### `POST /api/auth/mfa/passkey/start`

Démarre la vérification par passkey. Route anonyme, limitée à 30 requêtes par 5 minutes et par adresse IP. Elle ne compte pas dans le budget par compte : fermer l'invite du navigateur ne bloque personne.

Corps : `{ "mfaToken": "…" }`.

Réponse 200 :

```json
{ "challengeId": "7b0d…", "publicKey": { "challenge": "…", "rpId": "git.example.com" } }
```

`publicKey` est l'objet d'options WebAuthn à passer à `navigator.credentials.get({ publicKey })`. Gardez `challengeId` pour l'étape suivante.

Erreurs : 401 `invalid or expired token`, 429 `too many attempts, try again later`, 503 `passkeys are not available on this server`. Les passkeys sont indisponibles quand l'adresse publique (`PUBLIC_URL`) est une adresse IP, ou du HTTP simple sur un hôte autre que `localhost`. Le champ `passkeysAvailable` de `GET /api/auth/config` (décrite plus bas) le dit d'avance.

### `POST /api/auth/mfa/passkey/finish`

Termine la connexion avec la réponse de la passkey. Route anonyme.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `mfaToken` | texte | oui | Jeton MFA. |
| `challengeId` | UUID | oui | Valeur reçue de `passkey/start`. |
| `credential` | objet | oui | Le `PublicKeyCredential` renvoyé par le navigateur, sérialisé en JSON. |

Réponse 200 : `{ "token": "<JWT>" }`.

Erreurs : 400 `invalid request body` (corps ou credential mal formé), 401 `invalid or expired token`, 401 `invalid code` (échec de la passkey, même réponse qu'un mauvais code TOTP), 429, 503.

## Enrôler un premier facteur

Ces routes servent un compte qui n'a encore aucun second facteur (`mfaSetupRequired: true`). Elles s'autorisent aussi par le jeton MFA, et leur succès vous donne directement la session et les codes de secours.

### `POST /api/auth/mfa/setup/totp/enroll`

Crée un secret TOTP en attente. Route anonyme, autorisée par le jeton MFA.

Corps : `{ "mfaToken": "…" }`.

Réponse 200 :

```json
{
  "secret": "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",
  "otpauthUrl": "otpauth://totp/FerrisGit:alice?secret=…&issuer=FerrisGit&algorithm=SHA1&digits=6&period=30"
}
```

`secret` est en base32 (32 caractères). Le TOTP est du SHA-1, 6 chiffres, période de 30 secondes, avec une tolérance d'un pas de chaque côté. Rappeler la route avant la confirmation remplace le secret en attente. L'enrôlement non confirmé expire au bout de 15 minutes.

Erreurs : 400 `MFA is already set up` (le compte a déjà une passkey), 400 `TOTP is already enrolled`, 401 `invalid or expired token`, 429.

### `POST /api/auth/mfa/setup/totp/confirm`

Confirme le TOTP avec un premier code et ouvre la session.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `mfaToken` | texte | oui | Jeton MFA. |
| `code` | texte | oui | Code produit par l'application à partir du secret. |

Réponse 200 :

```json
{ "token": "<JWT>", "backupCodes": ["3f9c…", "a81d…"] }
```

Il y a 10 codes de secours de 32 caractères hexadécimaux. Ils ne sont montrés qu'ici : gardez-les. Un e-mail prévient le titulaire du compte qu'un facteur a été ajouté, sans faire échouer l'enrôlement s'il ne part pas.

Erreurs : 400 `invalid code` (code faux, ou enrôlement expiré, ici en 400 et non en 401), 400 `no TOTP enrolment to confirm`, 400 `TOTP is already enrolled`, 400 `MFA is already set up`, 401 `invalid or expired token`, 429.

### `POST /api/auth/mfa/setup/passkey/start`

Démarre l'enrôlement d'une première passkey. Route anonyme, limitée comme `passkey/start` (30 par 5 minutes et par IP).

Corps : `{ "mfaToken": "…" }`.

Réponse 200 : `{ "challengeId": "…", "publicKey": { … } }`. `publicKey` est l'objet d'options pour `navigator.credentials.create({ publicKey })`.

Erreurs : 400 `MFA is already set up` (le compte a déjà un facteur), 401 `invalid or expired token`, 429, 503 `passkeys are not available on this server`.

### `POST /api/auth/mfa/setup/passkey/finish`

Enregistre la passkey, génère les codes de secours et ouvre la session.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `mfaToken` | texte | oui | Jeton MFA. |
| `challengeId` | UUID | oui | Valeur reçue de `setup/passkey/start`. |
| `credential` | objet | oui | Le `PublicKeyCredential` de la création, en JSON. |
| `name` | texte | oui | Nom de la passkey, 40 caractères au plus, sans caractère de contrôle. |

Réponse 200 : `{ "token": "<JWT>", "backupCodes": ["…"] }`, comme pour le TOTP.

Erreurs : 400 `invalid request body`, 400 `invalid passkey name`, 400 `invalid passkey`, 400 `MFA is already set up`, 401 `invalid or expired token`, 429, 503.

## Inscription, activation et réinitialisation

### `GET /api/auth/config`

Dit ce que l'instance autorise. Route anonyme.

Réponse 200 :

```json
{ "registrationEnabled": false, "passkeysAvailable": true, "publicPagesEnabled": true }
```

`registrationEnabled` : l'inscription libre est ouverte (désactivée par défaut) et l'instance sait envoyer des e-mails. Sans SMTP configuré, la valeur reste `false` même si l'interrupteur est activé. `passkeysAvailable` : les passkeys fonctionnent sur cette instance. `publicPagesEnabled` : les pages publiques sont activées.

### `POST /api/auth/register`

Demande un compte quand l'inscription libre est activée. Route anonyme, limitée à 10 requêtes par 5 minutes et par adresse IP. Le compte n'est jamais administrateur. Il est créé **inactif** : son mot de passe est inutilisable tant que son titulaire n'a pas suivi le lien envoyé à l'adresse indiquée (voir `POST /api/auth/activate`), ce qui prouve qu'il lit cette adresse.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `username` | texte | oui | 3 à 32 caractères, en minuscules après normalisation, commençant par une lettre, avec uniquement `a-z`, `0-9`, `-` et `_`. |
| `email` | texte | oui | Adresse e-mail valide. Le lien d'activation y est envoyé. |

Il n'y a pas de mot de passe dans la requête : il se choisit à l'activation. Un champ `password` éventuel est ignoré.

Les noms suivants sont réservés : `login`, `register`, `activate`, `reset-password`, `home`, `repositories`, `groups`, `account`, `search`, `runners`, `admin`, `api`, `assets`, `static`, `settings`, `health`, `git`, `explore`, `help`, `about`, `me`, `new`, `notifications`, `users`, `wiki`.

Réponse 204, sans corps ni session. L'e-mail « Confirmez votre inscription à FerrisGit » contient un lien `PUBLIC_URL/activate#token=…`, valable 24 heures et à usage unique. L'envoi est attendu : la réponse n'est un 204 que si le message est parti. Un compte qui n'est pas activé reçoit ensuite un rappel avec un nouveau lien chaque jour, et il est supprimé 7 jours après sa création (voir l'administration des utilisateurs).

Le même nom et la même adresse que ceux d'un compte pas encore activé ne sont pas un conflit : un nouveau lien est envoyé et l'ancien cesse de fonctionner, pour qu'un message perdu ne bloque pas le nom. Dès que le compte est activé, ou si seul le nom ou seule l'adresse correspond à un autre compte, la réponse est un 409.

Erreurs : 400 `registration is disabled` (cette réponse ne dépend pas de ce que vous envoyez), 400 sur une règle non respectée (message précis), 409 `username already taken`, 409 `username collides with an existing root group`, 409 `email already in use`, 429 `too many registration attempts, try again later`, 503 `registration needs e-mail to be configured` (aucun SMTP n'est configuré), 503 `the confirmation e-mail could not be sent, try again later` (le serveur SMTP n'a pas accepté le message ; le détail n'est volontairement pas renvoyé, et le même appel peut être refait).

### `POST /api/auth/activate`

Active un compte créé par un administrateur ou par une inscription libre, et définit son mot de passe, ainsi que son nom d'utilisateur pour une invitation. Route anonyme, limitée à 10 requêtes par 5 minutes et par IP (budget partagé avec `reset-password`).

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `token` | texte | oui | Le jeton du lien reçu par e-mail : 64 caractères hexadécimaux, placé dans le fragment `#token=` de l'URL `/invitation` (invitation d'un administrateur) ou `/activate` (confirmation d'inscription). |
| `password` | texte | oui | 8 caractères au minimum. |
| `username` | texte | pour une invitation | Le nom que choisit la personne invitée : 3 à 32 caractères, mis en minuscules, commençant par une lettre, avec `a-z`, `0-9`, `-` et `_`, hors noms réservés. Exigé quand le compte vient d'une invitation, refusé quand le nom a été choisi à l'inscription. |

Le lien est valable 24 heures. Réponse 204, sans session : connectez-vous ensuite, et enrôlez un second facteur à la première connexion.

Erreurs : 400 `invalid or expired invitation` (jeton inconnu, expiré, déjà utilisé ou mal formé), 400 `password must be at least 8 characters`, 400 `username is required`, 400 `username was chosen at registration`, 400 nom invalide ou réservé, 409 `username already taken`, 409 `username collides with an existing root group` (dans tous ces cas, le lien reste utilisable), 429 `too many activation attempts, try again later`.

### `POST /api/auth/reset-password`

Définit un nouveau mot de passe avec le lien qu'un administrateur a émis. Route anonyme, même limite que `activate`.

Corps : `{ "token": "…", "password": "…" }`, mêmes règles que pour l'activation. Le lien est valable 1 heure.

Réponse 204, sans session. Toutes les sessions du compte sont révoquées, les facteurs de double authentification restent en place. Un e-mail prévient le titulaire que son mot de passe a été défini.

Erreurs : 400 `invalid or expired password reset link`, 400 `password must be at least 8 characters`, 429 `too many password reset attempts, try again later`.

## Mon compte

### `GET /api/auth/me`

Renvoie le compte connecté.

Réponse 200 :

```json
{ "id": "0b6c…", "username": "alice", "email": "alice@example.com", "isAdmin": false, "createdAt": "2026-03-01T10:00:00Z" }
```

### `PATCH /api/auth/me`

Change l'adresse e-mail du compte connecté.

Corps : `{ "email": "nouvelle@example.com" }`. Réponse 200 : le compte, comme `GET /api/auth/me`. Erreur : 400 `email must be a valid address` (vide, ou sans `@`).

### `POST /api/auth/me/password`

Change le mot de passe du compte connecté.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `currentPassword` | texte | oui | Mot de passe actuel. |
| `newPassword` | texte | oui | 8 caractères au minimum. |

Réponse 200 : `{ "token": "<nouveau JWT>" }`. Le changement révoque tous les JWT émis avant, y compris celui de la requête : remplacez-le par le nouveau. Un e-mail prévient le titulaire.

Erreurs : 400 `password must be at least 8 characters`, 400 `current password is incorrect` (en 400, pas en 401, pour qu'un formulaire ne déconnecte pas l'utilisateur), 429 `too many attempts, try again later` (cette route partage le budget de 10 tentatives par 5 minutes de la double authentification).

### `POST /api/auth/logout-all`

Déconnecte le compte partout : tous les JWT émis jusque-là sont révoqués, y compris celui de la requête. Les jetons d'accès Git restent valables ; ils se révoquent un par un. L'événement est consigné dans le journal de sécurité.

Pas de corps. Réponse 204.

## Gérer ses facteurs

Ces routes demandent un JWT. Celles qui modifient un facteur redemandent le mot de passe actuel et comptent dans le budget de 10 tentatives par 5 minutes. Supprimer un facteur révoque tous vos JWT, y compris celui de la requête : reconnectez-vous ensuite.

### `GET /api/me/mfa`

État de la double authentification du compte connecté. Ne compte pas dans la limite.

Réponse 200 :

```json
{
  "totpEnabled": true,
  "backupCodesRemaining": 9,
  "passkeys": [
    { "id": "5f1e…", "name": "Clé du bureau", "createdAt": "2026-03-01T10:00:00Z", "lastUsedAt": null }
  ]
}
```

`backupCodesRemaining` vaut 0 quand le compte n'a aucun facteur. `lastUsedAt` est `null` tant que la passkey n'a pas servi.

### `POST /api/me/mfa/totp/enroll`

Prépare un TOTP pour un compte qui n'en a pas (par exemple s'il ne possède qu'une passkey).

Corps : `{ "currentPassword": "…" }`. Réponse 200 : `{ "secret": "…", "otpauthUrl": "…" }`, comme pour `setup/totp/enroll`.

Erreurs : 400 `current password is incorrect`, 400 `TOTP is already enrolled`, 429.

### `POST /api/me/mfa/totp/confirm`

Confirme le TOTP en attente.

Corps : `{ "code": "123456" }`. Réponse 200 : `{ "backupCodes": ["…"] }` (10 nouveaux codes, qui remplacent les précédents).

Erreurs : 400 `invalid code`, 400 `no TOTP enrolment to confirm`, 400 `TOTP is already enrolled`, 429.

### `POST /api/me/mfa/totp/disable`

Supprime le TOTP.

Corps : `{ "currentPassword": "…" }`. Réponse 204. Les codes de secours sont supprimés aussi si le compte n'a plus aucune passkey. Si le compte n'a plus aucun facteur, il devra en enrôler un à la prochaine connexion.

Erreurs : 400 `current password is incorrect`, 429.

### `POST /api/me/mfa/backup-codes/regenerate`

Génère 10 nouveaux codes de secours, qui remplacent tous les anciens.

Corps : `{ "currentPassword": "…" }`. Réponse 200 : `{ "backupCodes": ["…"] }`.

Erreurs : 400 `current password is incorrect`, 400 `no MFA factor is enrolled`, 429.

### `POST /api/me/mfa/passkeys/register/start`

Démarre l'ajout d'une passkey à un compte connecté.

Corps : `{ "currentPassword": "…" }`. Réponse 200 : `{ "challengeId": "…", "publicKey": { … } }`.

Erreurs : 400 `current password is incorrect`, 400 `invalid request body`, 429, 503 `passkeys are not available on this server`.

### `POST /api/me/mfa/passkeys/register/finish`

Enregistre la passkey.

Corps :

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `challengeId` | UUID | oui | Valeur reçue de `register/start`. |
| `credential` | objet | oui | Le `PublicKeyCredential` de la création, en JSON. |
| `name` | texte | oui | Nom, 40 caractères au plus. |

Réponse 201 : la passkey créée `{ "id", "name", "createdAt", "lastUsedAt" }`.

Erreurs : 400 `invalid request body`, 400 `invalid passkey name`, 400 `invalid passkey`, 400 `too many passkeys` (20 au plus par compte), 429, 503.

### `POST /api/me/mfa/passkeys/{id}/delete`

Supprime une passkey. C'est un `POST`, parce que la route redemande le mot de passe dans le corps.

Chemin : `id`, l'identifiant de la passkey. Corps : `{ "currentPassword": "…" }`. Réponse 204. Cette route marche même quand les passkeys sont indisponibles.

Erreurs : 400 `current password is incorrect`, 404 `passkey` (inconnue, ou celle d'un autre compte ; personne n'est alors déconnecté), 429.

## Jetons Git (gestion)

Ces trois routes gèrent les jetons `fg_…` du compte connecté.

### `POST /api/tokens`

Crée un jeton Git.

Corps : `{ "name": "portable" }`. Le nom est libre.

Réponse 200 :

```json
{ "id": "9a2c…", "name": "portable", "token": "fg_5b0e…" }
```

`token` est la seule fois où la valeur en clair est visible : FerrisGit n'en garde que l'empreinte. Un jeton n'a pas de date d'expiration, il reste valable jusqu'à sa révocation.

```bash
curl -s -X POST "$BASE/api/tokens" -H "Authorization: Bearer $JWT" \
  -H 'Content-Type: application/json' -d '{"name":"portable"}'
```

### `GET /api/tokens`

Liste vos jetons.

Réponse 200 : un tableau d'objets `{ "id", "name", "createdAt", "lastUsedAt" }`. `lastUsedAt` vaut `null` tant que le jeton n'a pas servi. La valeur du jeton n'est jamais renvoyée.

### `DELETE /api/tokens/{id}`

Révoque un jeton : il cesse d'authentifier immédiatement.

Chemin : `id`, l'identifiant du jeton. Réponse 200, corps vide. Erreur : 404 `api token` (inconnu ou appartenant à un autre compte).
