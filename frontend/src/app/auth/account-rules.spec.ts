import { MIN_PASSWORD_LENGTH, USERNAME_ERROR, USERNAME_HINT, accountName, emailError, passwordError, usernameError } from './account-rules';

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

  describe('passwordError', () => {
    it('asks for a password when it is empty', () => {
      expect(passwordError('')).toBe('Saisissez un mot de passe');
    });

    it('refuses a password under the minimum length', () => {
      expect(MIN_PASSWORD_LENGTH).toBe(8);
      expect(passwordError('1234567')).toBe('Au moins 8 caractères');
    });

    it('accepts the minimum length, spaces included (a password is never trimmed)', () => {
      expect(passwordError('12345678')).toBeNull();
      expect(passwordError('       8')).toBeNull();
    });
  });

  describe('the name of the account', () => {
    it('is stored trimmed and lower-cased, so it is shown that way', () => {
      expect(accountName('  Florian_D ')).toBe('florian_d');
      expect(accountName('alice')).toBe('alice');
    });

    it('is announced in the hint under the field', () => {
      expect(USERNAME_HINT).toContain('minuscules');
    });
  });
});
