//! Subject, text and HTML for outbound notifications. Styles are inline and the layout is a `<table>` because mail
//! clients handle stylesheets, `flex` and `grid` badly. The logo is `cid:{LOGO_CID}`, which `SmtpEmailSender` attaches.

pub struct EmailContent {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// Content id the mailer attaches the logo image under.
pub const LOGO_CID: &str = "ferrisgit-logo";

const PRIMARY: &str = "#0d1ed3";
const TEXT_PRIMARY: &str = "#1f2937";
const TEXT_SECONDARY: &str = "#6b7280";

/// Escapes text for an HTML body or a quoted attribute. Usernames and URLs are user-influenced.
fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn shell(preheader: &str, body_html: &str) -> String {
    let preheader = esc(preheader);
    format!(
        r#"<!doctype html>
<html lang="fr">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>FerrisGit</title>
  </head>
  <body style="margin:0; padding:0; background-color:#f4f5f7; font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;">
    <span style="display:none; font-size:1px; color:#f4f5f7; line-height:1px; max-height:0; max-width:0; opacity:0; overflow:hidden;">{preheader}</span>
    <table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background-color:#f4f5f7; padding:32px 16px;">
      <tr>
        <td align="center">
          <table role="presentation" width="100%" style="max-width:480px; background-color:#ffffff; border-radius:12px; overflow:hidden; box-shadow:0 1px 3px rgba(0,0,0,0.1);" cellpadding="0" cellspacing="0">
            <tr>
              <td style="padding:32px 32px 24px; text-align:center; border-bottom:1px solid #e5e7eb;">
                <img src="cid:{LOGO_CID}" alt="FerrisGit" width="220" style="display:block; width:220px; max-width:100%; height:auto; margin:0 auto;" />
              </td>
            </tr>
            <tr>
              <td style="padding:32px; font-size:14px; line-height:1.6; color:{TEXT_PRIMARY};">
                {body_html}
              </td>
            </tr>
            <tr>
              <td style="padding:20px 32px; background-color:#f9fafb; text-align:center;">
                <p style="margin:0; font-size:12px; color:{TEXT_SECONDARY};">Cet email a été envoyé automatiquement par votre instance FerrisGit.</p>
              </td>
            </tr>
          </table>
        </td>
      </tr>
    </table>
  </body>
</html>"#
    )
}

fn button(href: &str, label: &str) -> String {
    let href = esc(href);
    let label = esc(label);
    format!(
        r#"<table role="presentation" cellpadding="0" cellspacing="0" style="margin:24px 0;"><tr><td style="border-radius:8px; background-color:{PRIMARY};"><a href="{href}" style="display:inline-block; padding:12px 24px; font-size:14px; font-weight:600; color:#ffffff; text-decoration:none;">{label}</a></td></tr></table>"#
    )
}

fn greeting(username: &str) -> String {
    format!(
        r#"<p style="margin:0 0 16px;">Bonjour <strong>{}</strong>,</p>"#,
        esc(username)
    )
}

/// The amber box closing the security notifications. `html` is trusted markup, the caller escapes it.
fn warning(html: &str) -> String {
    format!(
        r#"<p style="margin:0; padding:12px 16px; background-color:#fef3c7; border-radius:8px; color:#92400e;">{html}</p>"#
    )
}

pub fn account_created(username: &str, activation_url: &str) -> EmailContent {
    let text = format!(
        "Bonjour {username},\n\n\
         Votre nom d'utilisateur : {username}\n\n\
         Un compte FerrisGit a été créé pour vous. Pour l'activer et choisir votre mot de passe, \
         cliquez sur le lien suivant dans les 24 heures :\n\n\
         {activation_url}\n\n\
         Passé ce délai, le lien expirera et vous devrez demander à un administrateur de vous \
         renvoyer une invitation."
    );
    let body_html = format!(
        r#"{greeting}
<p style="margin:0 0 16px;">Votre nom d'utilisateur : <strong>{username}</strong></p>
<p style="margin:0 0 16px;">Un compte FerrisGit a été créé pour vous. Pour l'activer et choisir votre mot de passe, cliquez sur le bouton ci-dessous.</p>
{button}
<p style="margin:16px 0 0; font-size:13px; color:{TEXT_SECONDARY};">Ce lien expire dans 24 heures. Passé ce délai, demandez à un administrateur de vous renvoyer une invitation.</p>"#,
        greeting = greeting(username),
        username = esc(username),
        button = button(activation_url, "Activer mon compte"),
    );
    EmailContent {
        subject: "Votre compte FerrisGit".to_string(),
        text,
        html: shell("Activez votre compte FerrisGit", &body_html),
    }
}

/// Sent when someone registers themselves: the link proves they own the address and lets them pick a password.
pub fn registration_confirmation(username: &str, activation_url: &str) -> EmailContent {
    let text = format!(
        "Bonjour {username},\n\n\
         Quelqu'un a demandé un compte FerrisGit avec cette adresse et le nom d'utilisateur : {username}\n\n\
         Pour confirmer votre adresse et choisir votre mot de passe, cliquez sur le lien suivant dans les 24 heures :\n\n\
         {activation_url}\n\n\
         Si vous n'êtes pas à l'origine de cette demande, ignorez ce message : aucun compte ne sera utilisable. \
         Passé ce délai, le lien expirera et vous pourrez vous inscrire de nouveau pour en recevoir un autre."
    );
    let body_html = format!(
        r#"{greeting}
<p style="margin:0 0 16px;">Quelqu'un a demandé un compte FerrisGit avec cette adresse. Votre nom d'utilisateur : <strong>{username}</strong></p>
<p style="margin:0 0 16px;">Pour confirmer votre adresse et choisir votre mot de passe, cliquez sur le bouton ci-dessous.</p>
{button}
<p style="margin:16px 0 0; font-size:13px; color:{TEXT_SECONDARY};">Ce lien expire dans 24 heures. Passé ce délai, vous pourrez vous inscrire de nouveau pour en recevoir un autre. Si vous n'êtes pas à l'origine de cette demande, ignorez ce message : aucun compte ne sera utilisable.</p>"#,
        greeting = greeting(username),
        username = esc(username),
        button = button(activation_url, "Confirmer mon inscription"),
    );
    EmailContent {
        subject: "Confirmez votre inscription à FerrisGit".to_string(),
        text,
        html: shell("Confirmez votre inscription à FerrisGit", &body_html),
    }
}

pub fn password_changed(username: &str) -> EmailContent {
    let text = format!(
        "Bonjour {username},\n\n\
         Le mot de passe de votre compte FerrisGit vient d'être modifié.\n\n\
         Si vous êtes à l'origine de ce changement, aucune action n'est nécessaire.\n\n\
         Si vous n'êtes pas à l'origine de ce changement, contactez immédiatement un \
         administrateur de votre instance FerrisGit."
    );
    let body_html = format!(
        r#"{greeting}
<p style="margin:0 0 16px;">Le mot de passe de votre compte FerrisGit vient d'être modifié.</p>
<p style="margin:0 0 16px;">Si vous êtes à l'origine de ce changement, aucune action n'est nécessaire.</p>
{warning}"#,
        greeting = greeting(username),
        warning = warning(
            "Si vous n'êtes <strong>pas</strong> à l'origine de ce changement, contactez immédiatement un administrateur de votre instance FerrisGit."
        ),
    );
    EmailContent {
        subject: "Votre mot de passe FerrisGit a été modifié".to_string(),
        text,
        html: shell("Votre mot de passe a été modifié", &body_html),
    }
}

pub fn mfa_enrolled(username: &str, method: &str) -> EmailContent {
    let text = format!(
        "Bonjour {username},\n\n\
         Une nouvelle méthode de double authentification vient d'être ajoutée à votre compte \
         FerrisGit : {method}.\n\n\
         Si vous êtes à l'origine de cet ajout, aucune action n'est nécessaire.\n\n\
         Si vous n'êtes pas à l'origine de cet ajout, contactez immédiatement un administrateur \
         de votre instance FerrisGit."
    );
    let body_html = format!(
        r#"{greeting}
<p style="margin:0 0 16px;">Une nouvelle méthode de double authentification vient d'être ajoutée à votre compte FerrisGit : <strong>{method}</strong>.</p>
<p style="margin:0 0 16px;">Si vous êtes à l'origine de cet ajout, aucune action n'est nécessaire.</p>
{warning}"#,
        greeting = greeting(username),
        method = esc(method),
        warning = warning(
            "Si vous n'êtes <strong>pas</strong> à l'origine de cet ajout, contactez immédiatement un administrateur de votre instance FerrisGit."
        ),
    );
    EmailContent {
        subject: "Nouvelle méthode de double authentification ajoutée".to_string(),
        text,
        html: shell("Nouvelle méthode de double authentification", &body_html),
    }
}

pub fn mfa_reset(username: &str) -> EmailContent {
    let text = format!(
        "Bonjour {username},\n\n\
         Un administrateur de votre instance FerrisGit a réinitialisé la double authentification de votre compte. \
         Vous devrez la configurer de nouveau à votre prochaine connexion.\n\n\
         Si vous ne vous attendiez pas à ce changement, contactez un administrateur."
    );
    let body_html = format!(
        r#"{greeting}
<p style="margin:0 0 16px;">Un administrateur de votre instance FerrisGit a réinitialisé la double authentification de votre compte. Vous devrez la configurer de nouveau à votre prochaine connexion.</p>
{warning}"#,
        greeting = greeting(username),
        warning =
            warning("Si vous ne vous attendiez pas à ce changement, contactez un administrateur."),
    );
    EmailContent {
        subject: "La double authentification de votre compte a été réinitialisée".to_string(),
        text,
        html: shell("Double authentification réinitialisée", &body_html),
    }
}

/// Sent when an admin resets someone's password: the old one no longer works, sessions are closed, and the link (valid
/// 1 hour) lets them pick a new one. Has the same warning box as the other security mails, since the user didn't ask.
pub fn password_reset(username: &str, reset_url: &str) -> EmailContent {
    let text = format!(
        "Bonjour {username},\n\n\
         Un administrateur de votre instance FerrisGit a réinitialisé le mot de passe de votre compte. \
         Votre mot de passe actuel ne fonctionne plus et vos sessions en cours ont été fermées.\n\n\
         Pour choisir un nouveau mot de passe, cliquez sur le lien suivant dans l'heure qui vient :\n\n\
         {reset_url}\n\n\
         Passé ce délai, le lien expirera et vous devrez demander à un administrateur de recommencer.\n\n\
         Si vous ne vous attendiez pas à ce changement, contactez un administrateur."
    );
    let body_html = format!(
        r#"{greeting}
<p style="margin:0 0 16px;">Un administrateur de votre instance FerrisGit a réinitialisé le mot de passe de votre compte. Votre mot de passe actuel ne fonctionne plus et vos sessions en cours ont été fermées.</p>
<p style="margin:0 0 16px;">Pour choisir un nouveau mot de passe, cliquez sur le bouton ci-dessous.</p>
{button}
<p style="margin:16px 0; font-size:13px; color:{TEXT_SECONDARY};">Ce lien expire dans 1 heure. Passé ce délai, demandez à un administrateur de recommencer.</p>
{warning}"#,
        greeting = greeting(username),
        warning =
            warning("Si vous ne vous attendiez pas à ce changement, contactez un administrateur."),
        button = button(reset_url, "Choisir un nouveau mot de passe"),
    );
    EmailContent {
        subject: "Réinitialisation de votre mot de passe FerrisGit".to_string(),
        text,
        html: shell("Choisissez un nouveau mot de passe", &body_html),
    }
}

pub fn smtp_test() -> EmailContent {
    let message = "Ceci est un e-mail de test envoyé depuis les réglages de votre instance FerrisGit. Si vous le lisez, la configuration SMTP fonctionne.";
    let body_html = format!(r#"<p style="margin:0;">{message}</p>"#);
    EmailContent {
        subject: "E-mail de test FerrisGit".to_string(),
        text: message.to_string(),
        html: shell("Test de la configuration SMTP", &body_html),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_created_includes_the_activation_link_in_both_bodies() {
        let c = account_created("florian", "https://git.example.com/activate?token=abc");
        assert!(
            c.text
                .contains("https://git.example.com/activate?token=abc")
        );
        assert!(
            c.html
                .contains("https://git.example.com/activate?token=abc")
        );
        assert!(c.html.contains("florian"));
        assert!(c.html.starts_with("<!doctype html>"));
        assert_eq!(c.subject, "Votre compte FerrisGit");
    }

    #[test]
    fn account_created_states_the_username_to_sign_in_with_in_both_bodies() {
        let c = account_created("Bob_1-x", "https://x/activate#token=abc");
        assert!(
            c.text.contains("Votre nom d'utilisateur : Bob_1-x"),
            "{}",
            c.text
        );
        assert!(
            c.html
                .contains("Votre nom d'utilisateur : <strong>Bob_1-x</strong>"),
            "{}",
            c.html
        );
        let hostile = account_created("<b>x</b>", "https://x/y");
        assert!(
            !hostile.html.contains("<b>x</b>"),
            "the username is escaped in the HTML body"
        );
        assert!(
            hostile.html.contains("Votre nom d") && hostile.html.contains("&lt;b&gt;x&lt;/b&gt;")
        );
    }

    #[test]
    fn registration_confirmation_includes_the_link_and_the_username_and_says_it_lasts_a_day() {
        let c = registration_confirmation("bob", "https://git.example.com/activate#token=abc");
        assert_eq!(c.subject, "Confirmez votre inscription à FerrisGit");
        assert!(
            c.text
                .contains("https://git.example.com/activate#token=abc")
        );
        assert!(
            c.html
                .contains("https://git.example.com/activate#token=abc")
        );
        assert!(c.text.contains("le nom d'utilisateur : bob"), "{}", c.text);
        assert!(c.html.contains("<strong>bob</strong>"), "{}", c.html);
        assert!(c.text.contains("24 heures") && c.html.contains("24 heures"));
    }

    #[test]
    fn registration_confirmation_tells_a_stranger_to_ignore_it_and_escapes_the_username() {
        let c = registration_confirmation("<b>x</b>", "https://x/y");
        assert!(c.text.contains("ignorez ce message"), "{}", c.text);
        assert!(c.html.contains("ignorez ce message"));
        assert!(!c.html.contains("<b>x</b>"));
        assert!(c.html.contains("&lt;b&gt;x&lt;/b&gt;"));
    }

    #[test]
    fn every_html_body_shares_the_shell_with_the_inline_logo_and_the_footer() {
        for c in [
            account_created("a", "https://x/y"),
            registration_confirmation("a", "https://x/y"),
            password_changed("a"),
            mfa_enrolled("a", "TOTP"),
            mfa_reset("a"),
            password_reset("a", "https://x/y"),
            smtp_test(),
        ] {
            assert!(c.html.contains("cid:ferrisgit-logo"), "{}", c.subject);
            assert!(
                c.html.contains(
                    "Cet email a été envoyé automatiquement par votre instance FerrisGit."
                ),
                "{}",
                c.subject
            );
            assert!(c.html.contains("#f4f5f7"));
            assert!(!c.text.is_empty());
            assert!(!c.html.contains("ArtiFerris"), "{}", c.subject);
        }
    }

    #[test]
    fn user_influenced_values_are_html_escaped() {
        let c = password_changed("<script>alert(1)</script>");
        assert!(!c.html.contains("<script>"));
        assert!(c.html.contains("&lt;script&gt;"));
        let link = account_created("a", "https://x/a?b=1&c=\"2\"");
        assert!(link.html.contains("b=1&amp;c=&quot;2&quot;"));
        assert!(
            link.text.contains("b=1&c=\"2\""),
            "the plain-text body is not escaped"
        );
    }

    #[test]
    fn password_changed_and_mfa_reset_carry_the_warning_callout() {
        assert!(password_changed("f").html.contains("#fef3c7"));
        assert!(mfa_reset("f").html.contains("#fef3c7"));
        assert_eq!(
            mfa_reset("f").subject,
            "La double authentification de votre compte a été réinitialisée"
        );
    }

    #[test]
    fn password_reset_includes_the_reset_link_in_both_bodies_and_says_it_lasts_one_hour() {
        let c = password_reset(
            "florian",
            "https://git.example.com/reset-password#token=abc",
        );
        assert!(
            c.text
                .contains("https://git.example.com/reset-password#token=abc")
        );
        assert!(
            c.html
                .contains("https://git.example.com/reset-password#token=abc")
        );
        assert!(c.html.contains("Choisir un nouveau mot de passe"));
        assert!(c.text.contains("dans l'heure qui vient"), "{}", c.text);
        assert!(
            c.html.contains("Ce lien expire dans 1 heure."),
            "{}",
            c.html
        );
        assert!(c.html.contains("<strong>florian</strong>"));
        assert_eq!(
            c.subject,
            "Réinitialisation de votre mot de passe FerrisGit"
        );
        assert!(
            c.text
                .contains("Votre mot de passe actuel ne fonctionne plus"),
            "{}",
            c.text
        );
        assert!(
            c.html
                .contains("Votre mot de passe actuel ne fonctionne plus"),
            "{}",
            c.html
        );
    }

    #[test]
    fn password_reset_carries_the_warning_callout_and_escapes_its_values() {
        let c = password_reset("<b>x</b>", "https://x/a?b=1&c=\"2\"");
        assert!(c.html.contains("#fef3c7"));
        assert!(c.html.contains(
            "Si vous ne vous attendiez pas à ce changement, contactez un administrateur."
        ));
        assert!(!c.html.contains("<b>x</b>"));
        assert!(c.html.contains("&lt;b&gt;x&lt;/b&gt;"));
        assert!(c.html.contains("b=1&amp;c=&quot;2&quot;"));
    }

    #[test]
    fn mfa_enrolled_mentions_the_method_in_both_bodies() {
        let c = mfa_enrolled("f", "une clé d'accès (passkey)");
        assert!(c.text.contains("une clé d'accès (passkey)"));
        assert!(c.html.contains("une clé d&#39;accès (passkey)"));
        assert_eq!(
            c.subject,
            "Nouvelle méthode de double authentification ajoutée"
        );
    }
}
