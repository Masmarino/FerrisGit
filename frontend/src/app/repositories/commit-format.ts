/** The first line of a commit message. */
export function commitTitle(message: string): string {
  return message.split('\n', 1)[0].trim();
}

/** The seven-character form of a commit hash. */
export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}
