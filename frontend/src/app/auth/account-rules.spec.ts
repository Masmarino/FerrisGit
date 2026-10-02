import { USERNAME_ERROR, USERNAME_HINT, emailError, usernameError } from './account-rules';

describe('account rules (client mirror of the server)', () => {
  describe('usernameError', () => {
    it.each(['abc', 'Alice', 'a_b-c1', 'a'.repeat(32), 'x1_'])('accepts %s', (username) => {
      expect(usernameError(username)).toBeNull();
    });

    it("ignores the spaces around the name, as the server trims", () => {
      expect(usernameError('  alice  ')).toBeNull();
    });

    it('asks for a name when it is empty', () => {
      expect(usernameError('')).toBe("Saisissez un nom d'utilisateur");
      expect(usernameError('   ')).toBe("Saisissez un nom d'utilisateur");
    });

    it.each(['ab', 'a'.repeat(33), '1abc', '_abc', 'a b', 'a.git', 'é'.repeat(3), 'ali ce', 'a@b'])('refuses %s with the rule, not a vague message', (username) => {
      expect(usernameError(username)).toBe(USERNAME_ERROR);
    });
  });

  describe('emailError', () => {
    it.each(['a@example.com', 'first.last+tag@sub.example.co', ' padded@example.com '])('accepts %s', (email) => {
      expect(emailError(email)).toBeNull();
    });

    it('asks for an address when it is empty', () => {
      expect(emailError('')).toBe('Saisissez votre adresse e-mail');
    });

    it.each(['nope', 'admin@localhost', '@example.com', 'a@@example.com', 'a@b@example.com', 'a@.example.com', 'a@example.com.', 'a b@example.com', `${'a'.repeat(250)}@example.com`])(
      'refuses %s',
      (email) => {
        expect(emailError(email)).toBe('Saisissez une adresse e-mail valide, par exemple nom@exemple.fr');
      },
    );
  });

  it('announces the lower-casing in the hint under the field', () => {
    expect(USERNAME_HINT).toContain('minuscules');
  });
});
