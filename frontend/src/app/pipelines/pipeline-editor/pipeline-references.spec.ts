import { isEnvName, looksLikeSecret, missingSecrets, referencedNames, secretUsage, unknownReferences, wantedSecretNames } from './pipeline-references';

const job = (script: string[], variables: { key: string; value: string }[] = []) => ({ script, variables });

describe('pipeline references', () => {
  it('reads the names of the variables the commands use, in both spellings, once each', () => {
    expect(referencedNames(job(['echo $A ${B}', 'curl "$A" ${C:-x}', 'price is $5 and $?']))).toEqual(['A', 'B', 'C']);
  });

  it('does not take a plain dollar for a variable', () => {
    expect(referencedNames(job(['echo "$"', 'echo $1 $@ $$']))).toEqual([]);
  });

  it('knows which names an environment variable can have', () => {
    expect(['A', '_a', 'npm_config_cache', 'A1'].every(isEnvName)).toBe(true);
    expect(['1A', 'A-B', 'A B', ''].some(isEnvName)).toBe(false);
  });

  it('spots names that look like credentials', () => {
    expect(['API_TOKEN', 'db_password', 'SECRET', 'AWS_ACCESS_KEY', 'SSH_PRIVATE', 'DEPLOY_KEY', 'apikey'].every(looksLikeSecret)).toBe(true);
    expect(['NODE_ENV', 'RUST_LOG', 'KEYBOARD', 'TARGET'].some(looksLikeSecret)).toBe(false);
  });

  describe('what nothing provides', () => {
    it('leaves out the job variables, the secrets, the shell, and what the commands assign themselves', () => {
      const j = job(['export OUT=build', 'DEST=$HOME/x', 'echo $OUT $DEST $PATH $MODE $TOKEN $LOST'], [{ key: 'MODE', value: 'fast' }]);

      expect(unknownReferences(j, ['TOKEN'])).toEqual(['LOST']);
    });

    it('treats a variable that is still being typed as missing, not as a crash', () => {
      expect(unknownReferences(job(['echo $A'], [{ key: '', value: '' }]), [])).toEqual(['A']);
    });
  });

  it('counts which jobs read each secret, including the secrets nobody reads', () => {
    const jobs = [
      { name: 'build', stage: 's', image: '', script: ['echo $A'], variables: [], needs: [], tags: [], cache: [] },
      { name: 'deploy', stage: 's', image: '', script: ['curl $A $B'], variables: [], needs: [], tags: [], cache: [] },
    ];

    expect([...secretUsage(jobs, ['A', 'B', 'C'])]).toEqual([['A', ['build', 'deploy']], ['B', ['deploy']], ['C', []]]);
  });

  describe('missing secrets', () => {
    it('lists the wanted ones that the job reads and the repository lacks', () => {
      expect(missingSecrets(job(['echo $DEPLOY_TOKEN $DEPLOY_URL']), ['DEPLOY_TOKEN', 'DEPLOY_URL', 'OTHER'], ['DEPLOY_URL'])).toEqual(['DEPLOY_TOKEN']);
    });

    it('does not ask for a secret the job defines itself', () => {
      expect(missingSecrets(job(['echo $DEPLOY_TOKEN'], [{ key: 'DEPLOY_TOKEN', value: 'x' }]), ['DEPLOY_TOKEN'], [])).toEqual([]);
    });
  });

  it('wants the names the catalog expects and the ones that look like credentials, not the rest', () => {
    expect(wantedSecretNames(job(['echo $DOCKER_HOST $MY_TOKEN $TARGET']))).toEqual(['DOCKER_HOST', 'MY_TOKEN']);
  });
});
