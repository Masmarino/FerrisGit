/**
 * What each part of the editor is for, written for someone who has never written a pipeline. Shown in the help bubbles.
 */
export interface HelpText {
  title: string;
  body: string;
}

export const HELP = {
  pipeline: {
    title: 'Comment ça marche',
    body: "Une pipeline est une suite d'étapes. Chaque étape contient des jobs, les petites tâches que FerrisGit fait tourner à chaque push : compiler, tester, publier. Les jobs d'une même étape partent en même temps ; une étape ne commence que lorsque la précédente a réussi.",
  },
  stage: {
    title: 'Une étape',
    body: "Un palier de la pipeline. Les jobs qu'elle contient s'exécutent en parallèle, et la suivante attend qu'ils aient tous réussi. Si un job échoue, les étapes d'après ne partent pas. Faites glisser les cartes d'une colonne à l'autre pour changer l'ordre.",
  },
  newStage: {
    title: 'Ajouter une étape',
    body: "Donnez un nom court (test, build, deploy). Elle se place après les autres ; le menu ⋮ d'une étape permet de la décaler. Une étape vide peut être supprimée.",
  },
  job: {
    title: 'Un job',
    body: "Une tâche, exécutée dans un conteneur : une image, puis des commandes. Cliquez sur la carte pour la régler. Un job qui échoue arrête la pipeline.",
  },
  tiles: {
    title: 'Les tuiles',
    body: "Des jobs prêts à l'emploi pour les besoins courants. Choisissez-en une, elle arrive dans l'étape avec une image et des commandes sensées, que vous pouvez ensuite modifier.",
  },
  name: {
    title: 'Nom du job',
    body: "Il apparaît dans la liste des pipelines et sert aux autres jobs pour l'attendre. Il doit être unique.",
  },
  image: {
    title: 'Image',
    body: "Le conteneur dans lequel les commandes tournent : il apporte les outils (rust:1 pour Cargo, node:22 pour npm, alpine pour des commandes simples). Une image de Docker Hub s'écrit nom:version.",
  },
  script: {
    title: 'Commandes',
    body: "Ce que le job exécute, dans l'ordre, comme si vous les tapiez dans un terminal à la racine du dépôt. Si une commande échoue, le job s'arrête et est marqué échoué.",
  },
  insert: {
    title: 'Insérer une variable',
    body: "Place le nom d'une variable ou d'un secret dans la commande, sous la forme $NOM : elle sera remplacée par sa valeur au moment de l'exécution.",
  },
  variables: {
    title: 'Variables du job',
    body: "Des valeurs que les commandes lisent par leur nom ($NOM). Elles sont écrites dans le fichier du dépôt : tout le monde les lit. Pour un mot de passe ou un jeton, utilisez un secret.",
  },
  secrets: {
    title: 'Secrets',
    body: "Des valeurs confidentielles (jeton, mot de passe) enregistrées chiffrées pour le dépôt, hors du fichier. Tous les jobs les reçoivent comme des variables, et leur valeur est masquée dans les journaux. Il suffit de les nommer dans une commande : $NOM.",
  },
  needs: {
    title: 'Attend la fin de',
    body: "Par défaut un job attend toute l'étape précédente. Cochez des jobs pour qu'il parte dès que seuls ceux-là ont réussi, ou pour ordonner deux jobs d'une même étape.",
  },
  tags: {
    title: 'Étiquettes du runner',
    body: "Un runner est la machine qui exécute le job. Si vous indiquez une étiquette, seul un runner qui la porte le prend : pratique pour réserver un déploiement à une machine précise. Laissez vide pour n'importe quel runner.",
  },
  cache: {
    title: 'Caches',
    body: "Des dossiers conservés d'une pipeline à l'autre pour gagner du temps (les paquets téléchargés, par exemple). Donnez une clé : le même nom partage le même cache. Seul le moteur Kubernetes les applique ; avec des runners Docker ils sont sans effet.",
  },
  yaml: {
    title: 'Le YAML',
    body: "Le fichier que FerrisGit lit vraiment. Les cartes et le YAML décrivent la même chose : modifiez l'un, l'autre suit. Dans la vue YAML, vos commentaires sont conservés.",
  },
  propose: {
    title: 'Proposer la modification',
    body: "Rien n'est écrit sur la branche principale : le fichier part sur une nouvelle branche avec une merge request, que quelqu'un relit puis fusionne.",
  },
} as const satisfies Record<string, HelpText>;
