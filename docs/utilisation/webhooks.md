# Webhooks

Un webhook prévient un service externe (chat, outil d'audit, CI externe) quand quelque chose se passe dans un dépôt. FerrisGit envoie une requête HTTP `POST` signée, avec un corps JSON, à l'adresse que vous indiquez.

## Qui peut les gérer

Les webhooks se gèrent dans **Réglages > Webhooks** du dépôt, et seuls les Mainteneurs et le propriétaire du dépôt y ont accès. Un dépôt peut avoir 20 webhooks au plus.

## Ajouter un webhook

Dans la carte **Ajouter un webhook** :

- **URL** : une adresse `http` ou `https` ;
- **Secret** : une chaîne de votre choix, qui sert à signer les envois ;
- **Événements** : au moins un, cochés dans les groupes ci-dessous.

L'adresse doit viser un serveur joignable depuis Internet : FerrisGit refuse les adresses qui pointent vers la machine locale (`localhost`, `127.0.0.1`), les réseaux privés (`10.x`, `172.16-31.x`, `192.168.x`), les adresses de lien local (dont `169.254.169.254`, l'adresse des métadonnées cloud), l'espace partagé `100.64.0.0/10` et leurs équivalents IPv6. La vérification se fait à l'enregistrement et à nouveau à chaque envoi, sur l'adresse réellement résolue. Un service interne doit donc être exposé par un relais public.

L'interface permet d'ajouter, de supprimer et de consulter l'historique. L'API permet en plus de modifier l'URL, le secret et les événements, et de désactiver un webhook (voir [l'API des webhooks](/docs/api/webhooks-et-notifications)). Le secret n'est jamais affiché ni renvoyé après sa saisie : pour en changer, passez par l'API ou recréez le webhook.

## Événements disponibles

| Événement | Se déclenche quand… |
|---|---|
| `merge_request_approved` | quelqu'un approuve une demande de fusion |
| `merge_request_changes_requested` | quelqu'un demande des changements |
| `merge_request_commented` | quelqu'un commente une demande de fusion (général, en ligne ou réponse) |
| `merge_request_merged` | une demande de fusion est fusionnée |
| `merge_request_closed` | une demande de fusion est fermée |
| `issue_assigned` | un ticket est assigné |
| `issue_commented` | un ticket est commenté |
| `issue_closed` | un ticket est fermé |
| `pipeline_failed` | une pipeline passe à l'état échoué |
| `collaborator_added` | un collaborateur est ajouté |
| `collaborator_role_changed` | le rôle d'un collaborateur change |
| `collaborator_removed` | un collaborateur est retiré |

Il n'y a pas d'événement pour une poussée, la création d'un ticket ou d'une demande de fusion, une release, ni pour une pipeline réussie. Une revue identique à la précédente de la même personne sur le même commit ne redéclenche pas d'événement.

## Contenu de l'envoi

Le corps est un objet JSON. Le champ `event` donne l'événement, les autres champs sont en camelCase. Exemple pour une demande de fusion fusionnée :

```json
{
  "event": "merge_request_merged",
  "repositoryOwner": "alice",
  "repositoryName": "demo",
  "actorUsername": "bob",
  "mergeRequestId": "6f1c2d3e-4a5b-4c6d-8e7f-9a0b1c2d3e4f",
  "mergeRequestTitle": "Ajouter la page d'accueil"
}
```

Les champs communs à tous les événements sont `event`, `repositoryOwner` et `repositoryName`. `repositoryOwner` est le compte propriétaire enregistré du dépôt (le créateur, pour un dépôt de groupe). Selon la famille :

| Famille | Champs en plus |
|---|---|
| `merge_request_*` | `actorUsername`, `mergeRequestId`, `mergeRequestTitle` |
| `issue_*` | `actorUsername`, `issueId`, `issueTitle` |
| `collaborator_added`, `collaborator_role_changed` | `actorUsername`, `targetUsername`, `role` (`reader`, `contributor` ou `maintainer`) |
| `collaborator_removed` | `actorUsername`, `targetUsername` |
| `pipeline_failed` | `pipelineId`, `commitSha` (pas d'`actorUsername`) |

Le numéro d'un ticket n'est pas dans l'envoi, seulement son identifiant.

### En-têtes envoyés

| En-tête | Valeur |
|---|---|
| `Content-Type` | `application/json` |
| `X-FerrisGit-Signature-256` | `sha256=` suivi de la signature en hexadécimal |
| `X-FerrisGit-Delivery` | un identifiant unique (UUID) de l'envoi |
| `X-FerrisGit-Delivered-At` | la date de l'envoi, au format RFC 3339 |

## Vérifier la signature

La signature est un HMAC-SHA256 du **corps brut** de la requête, avec votre secret comme clé, écrit en hexadécimal minuscule. L'identifiant et la date d'envoi ne sont pas signés. Calculez la signature sur les octets reçus, avant tout décodage JSON, puis comparez-la en temps constant :

```python
import hashlib, hmac

def signature_valide(secret: bytes, corps: bytes, en_tete: str) -> bool:
    attendu = "sha256=" + hmac.new(secret, corps, hashlib.sha256).hexdigest()
    return hmac.compare_digest(attendu, en_tete)
```

Vous pouvez recalculer la signature à la main pour un essai :

```bash
printf '%s' "$CORPS" | openssl dgst -sha256 -hmac "$SECRET"
```

## Fiabilité : délai, réessais

- Chaque envoi est une seule tentative, avec un délai total de 5 secondes. Il n'y a pas de réessai : si votre service est indisponible ou lent, l'événement est perdu.
- Une réponse `2xx` est un succès. Toute autre réponse, y compris une redirection (FerrisGit ne la suit pas), est un échec. Répondez vite, et traitez le travail ensuite.
- Les envois partent en arrière-plan, après l'action de l'utilisateur, 10 au plus à la fois. Un webhook qui échoue ne gêne jamais l'action qui l'a provoqué.
- Un webhook désactivé n'est pas appelé.

## Tester et suivre les envois

Il n'y a pas de bouton d'essai. Pour tester, ajoutez un webhook vers une adresse publique de test, puis provoquez l'événement (commentez un ticket, par exemple).

L'icône d'horloge de chaque webhook ouvre l'**Historique des livraisons** : les 20 dernières, avec le résultat (**Succès** ou **Échec**), l'événement, le code HTTP reçu et, en cas d'échec, la raison (« received 500 Internal Server Error », délai dépassé, adresse refusée, etc.). C'est le premier endroit à regarder quand rien n'arrive.

## Sécurité du secret

Le secret est chiffré avant d'être stocké (AES-256-GCM), avec la clé `SETTINGS_ENCRYPTION_KEY` du serveur. Il n'est déchiffré qu'au moment de signer un envoi. Si cette clé est changée, les secrets existants ne peuvent plus être lus et les envois échouent avec « failed to resolve secret » : recréez alors vos webhooks. Voir [Configuration](/docs/administration/configuration).
