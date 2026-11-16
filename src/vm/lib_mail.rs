//! `use vyrn.mail`: sending email, as Python's smtplib does.
//!
//! ```vyrn
//! use vyrn.mail
//! use vyrn.os
//!
//! main:
//!     mail.send(
//!         to = "friend@example.com",
//!         subject = "Hello",
//!         body = "Sent from Vyrn.",
//!         sender = "me@example.com",
//!         server = "smtp.example.com",
//!         user = "me@example.com",
//!         password = os.env("MAIL_PASSWORD"),
//!     )
//! end
//! ```
//!
//! Port 587 with STARTTLS is the default; `security = "tls"` (port 465)
//! and `security = "none"` (for a local test server) are the others. Keep
//! passwords out of the code: read them from the environment.

use super::ops::{self, err, Ctx, VmResult};
use super::value::Value;

/// The parameters, in order; keyword arguments land in these places.
pub const PARAMS: &[&str] = &[
    "to", "subject", "body", "sender", "server", "port", "user", "password", "security", "html",
];

fn get(args: &[Value], i: usize) -> Option<&Value> {
    args.get(i)
        .filter(|v| !matches!(v, Value::Missing | Value::None))
}

fn text(ctx: &Ctx, args: &[Value], i: usize) -> Option<String> {
    get(args, i).map(|v| match v {
        Value::Str(s) => s.to_string(),
        other => ops::display(&ctx.program, other),
    })
}

/// `mail.send(to, subject, body, sender, server, port, user, password,
/// security, html)`
pub fn send(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let to: Vec<String> = match get(args, 0) {
        Some(Value::Str(s)) => vec![s.to_string()],
        Some(other) => ops::collect(other)?
            .iter()
            .map(|v| ops::display(&ctx.program, v))
            .collect(),
        None => return Err(err("CallError", "`mail.send` needs `to`")),
    };
    let subject = text(ctx, args, 1).unwrap_or_default();
    let body = text(ctx, args, 2).unwrap_or_default();
    let sender = text(ctx, args, 3)
        .or_else(|| text(ctx, args, 6))
        .ok_or_else(|| err("CallError", "`mail.send` needs `sender` (the From address)"))?;
    let server =
        text(ctx, args, 4).ok_or_else(|| err("CallError", "`mail.send` needs `server`"))?;
    let security = text(ctx, args, 8).unwrap_or_else(|| "starttls".into());
    let port = match get(args, 5) {
        Some(v) => super::builtins::integer(v, "the port")?,
        None => match security.as_str() {
            "tls" => 465,
            "none" => 25,
            _ => 587,
        },
    };
    let port =
        u16::try_from(port).map_err(|_| err("ValueError", format!("{port} is not a port")))?;
    let html = get(args, 9).is_some_and(Value::truthy);
    let user = text(ctx, args, 6);
    let password = text(ctx, args, 7);
    deliver(
        &to, &subject, &body, &sender, &server, port, user, password, &security, html,
    )?;
    Ok(Value::None)
}

#[cfg(feature = "mail")]
#[allow(clippy::too_many_arguments)]
fn deliver(
    to: &[String],
    subject: &str,
    body: &str,
    sender: &str,
    server: &str,
    port: u16,
    user: Option<String>,
    password: Option<String>,
    security: &str,
    html: bool,
) -> VmResult<()> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{Message, SmtpTransport, Transport};

    let address = |a: &str| {
        a.parse::<lettre::message::Mailbox>()
            .map_err(|e| err("ValueError", format!("{a:?} is not an email address: {e}")))
    };
    let mut b = Message::builder().from(address(sender)?).subject(subject);
    for t in to {
        b = b.to(address(t)?);
    }
    let message = b
        .header(if html {
            ContentType::TEXT_HTML
        } else {
            ContentType::TEXT_PLAIN
        })
        .body(body.to_string())
        .map_err(|e| err("MailError", e.to_string()))?;
    let fail = |e: lettre::transport::smtp::Error| {
        err(
            "MailError",
            format!("cannot send through {server}:{port}: {e}"),
        )
    };
    let mut builder = match security {
        "tls" => SmtpTransport::relay(server).map_err(fail)?.port(port),
        "starttls" => SmtpTransport::starttls_relay(server)
            .map_err(fail)?
            .port(port),
        "none" => SmtpTransport::builder_dangerous(server).port(port),
        other => {
            return Err(super::ops::type_error(format!(
                "unknown security {other:?}: use \"starttls\", \"tls\" or \"none\""
            )))
        }
    };
    if let Some(u) = user {
        builder = builder.credentials(Credentials::new(u, password.unwrap_or_default()));
    }
    builder
        .timeout(Some(std::time::Duration::from_secs(30)))
        .build()
        .send(&message)
        .map_err(fail)?;
    Ok(())
}

#[cfg(not(feature = "mail"))]
#[allow(clippy::too_many_arguments)]
fn deliver(
    _: &[String],
    _: &str,
    _: &str,
    _: &str,
    _: &str,
    _: u16,
    _: Option<String>,
    _: Option<String>,
    _: &str,
    _: bool,
) -> VmResult<()> {
    Err(err(
        "Unavailable",
        "`mail.send` needs a build with the `mail` feature",
    ))
}
