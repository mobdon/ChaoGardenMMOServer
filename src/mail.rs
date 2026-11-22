//! Password-reset mail through Postmark's HTTPS API (cloud hosts often block SMTP).
//!
//! Configured by `POSTMARK_SERVER_TOKEN`, `CHAO_MAIL_FROM` and `CHAO_PUBLIC_URL`.
//! With any of them unset no mail is sent and the reset link is never logged.

use serde_json::{Value, json};

pub const POSTMARK_URL: &str = "https://api.postmarkapp.com/email";

pub struct Mailer {
    token: String,
    from: String,
    public_url: String,
}

impl Mailer {
    pub fn from_env() -> Option<Self> {
        let var = |name| {
            std::env::var(name)
                .ok()
                .filter(|v: &String| !v.trim().is_empty())
        };
        Some(Self {
            token: var("POSTMARK_SERVER_TOKEN")?,
            from: var("CHAO_MAIL_FROM")?,
            public_url: var("CHAO_PUBLIC_URL")?.trim_end_matches('/').to_owned(),
        })
    }
    /// The reset link the mail carries.
    pub fn reset_link(&self, token: &str) -> String {
        format!("{}/?reset={token}", self.public_url)
    }
    /// Postmark's JSON body for a reset mail.
    pub fn reset_body(&self, to: &str, username: &str, token: &str) -> Value {
        let link = self.reset_link(token);
        json!({
            "From": self.from,
            "To": to,
            "Subject": "Reset your Chao Garden password",
            "TextBody": format!(
                "Hi {username},\n\nOpen this link within 30 minutes to choose a new password:\n{link}\n\n\
                 Your Chao stays safe. After the reset, sign in again on every device.\n\
                 If you didn't ask for this, ignore this mail."
            ),
            "MessageStream": "outbound",
        })
    }
    /// Sends the mail. Blocking: call it from `spawn_blocking`.
    pub fn send_reset(&self, to: &str, username: &str, token: &str) -> Result<(), String> {
        ureq::post(POSTMARK_URL)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("X-Postmark-Server-Token", &self.token)
            .send(self.reset_body(to, username, token).to_string())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_mail_body_carries_the_link_and_addresses() {
        let mailer = Mailer {
            token: "server-token".into(),
            from: "garden@example.com".into(),
            public_url: "https://chao.example.com".into(),
        };
        let body = mailer.reset_body("player@example.com", "tester", "abc123");
        assert_eq!(body["From"], "garden@example.com");
        assert_eq!(body["To"], "player@example.com");
        assert_eq!(body["MessageStream"], "outbound");
        let text = body["TextBody"].as_str().unwrap();
        assert!(text.contains("https://chao.example.com/?reset=abc123"));
        assert!(text.contains("tester"));
        assert!(!body.to_string().contains("server-token"));
    }

    #[test]
    fn unconfigured_mail_is_off() {
        // SAFETY: test-only; no other test reads this variable.
        unsafe { std::env::remove_var("POSTMARK_SERVER_TOKEN") };
        assert!(Mailer::from_env().is_none());
    }
}
