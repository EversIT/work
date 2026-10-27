//! Outgoing mail. There is no HTTP endpoint that sends mail: messages are only produced by
//! server-side events. All interpolated values are HTML-escaped and links are built from the
//! configured frontend URL.

use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};

use crate::{config::SmtpConfig, state::AppState};

pub fn escape_html(s: &str) -> String {
    s.chars()
        .fold(String::with_capacity(s.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                _ => out.push(c),
            }
            out
        })
}

fn transport(
    cfg: &SmtpConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, lettre::transport::smtp::Error> {
    let builder = match cfg.tls.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host)?,
        "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host),
        _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)?,
    };
    let builder = builder.port(cfg.port);
    let builder = match (&cfg.user, &cfg.pass) {
        (Some(u), Some(p)) => builder.credentials(Credentials::new(u.clone(), p.clone())),
        _ => builder,
    };
    Ok(builder.build())
}

pub fn enabled(state: &AppState) -> bool {
    state.config.smtp.is_some()
}

/// Fire-and-forget: a mail failure never fails the request that caused it.
pub fn send(state: &AppState, to: String, subject: String, html: String) {
    let Some(cfg) = state.config.smtp.clone() else {
        return;
    };
    tokio::spawn(async move {
        let result = async {
            let from: Mailbox = cfg
                .from
                .parse()
                .map_err(|e| format!("invalid WORK_SMTP_FROM: {e}"))?;
            let to: Mailbox = to.parse().map_err(|e| format!("invalid recipient: {e}"))?;
            let msg = Message::builder()
                .from(from)
                .to(to)
                .subject(subject)
                .header(ContentType::TEXT_HTML)
                .body(html)
                .map_err(|e| e.to_string())?;
            transport(&cfg)
                .map_err(|e| e.to_string())?
                .send(msg)
                .await
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(())
        }
        .await;
        if let Err(e) = result {
            tracing::warn!(error = %e, "sending email failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html() {
        assert_eq!(
            escape_html("<script>alert('x')</script> & \"q\""),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt; &amp; &quot;q&quot;"
        );
    }
}
