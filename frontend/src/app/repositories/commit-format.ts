export function commitTitle(message: string): string {
  return message.split('\n', 1)[0].trim();
}

export function shortSha(sha: string): string {
  return sha.slice(0, 7);
}
