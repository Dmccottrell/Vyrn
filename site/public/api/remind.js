// GET /api/remind, run every evening by Vercel Cron (vercel.json): emails
// learners who turned reminders on, have a streak going (a lesson
// yesterday) and haven't done a lesson today.
//
// Needs, in the Vercel project's environment variables:
//   SUPABASE_SERVICE_ROLE_KEY  to read every learner's activity
//   RESEND_API_KEY             to send email (resend.com)
//   REMINDER_FROM              the sender, e.g. "Learn Vyrn <hello@your-domain.com>"
//   CRON_SECRET                any long random text; Vercel sends it with each run
// Without them it does nothing.
import { SUPABASE_URL } from "./_config.js";

const SITE = process.env.SITE_URL || "https://learn-vyrn.vercel.app";

export default async function handler(req, res) {
  const { CRON_SECRET, SUPABASE_SERVICE_ROLE_KEY: key, RESEND_API_KEY, REMINDER_FROM } = process.env;
  if (!CRON_SECRET || req.headers.authorization !== `Bearer ${CRON_SECRET}`) {
    return res.status(401).json({ error: "Only the scheduler runs this." });
  }
  if (!key || !RESEND_API_KEY || !REMINDER_FROM) {
    return res.status(503).json({ error: "Reminders aren't set up: see site/README.md." });
  }
  const db = (path, init = {}) =>
    fetch(`${SUPABASE_URL}/rest/v1/${path}`, {
      ...init,
      headers: { apikey: key, authorization: `Bearer ${key}`, "content-type": "application/json", ...init.headers },
    });

  const day = (offset) => new Date(Date.now() + offset * 86400000).toISOString().slice(0, 10);
  const today = day(0), yesterday = day(-1);
  const people = await (await db("profiles?select=id,email,name,last_reminded&reminders=eq.true")).json();
  if (!Array.isArray(people) || !people.length) return res.status(200).json({ sent: 0 });
  const recent = await (await db(`activity?select=user_id,day,lessons&day=gte.${yesterday}&lessons=gt.0`)).json();
  const did = (id, d) => recent.some((a) => a.user_id === id && a.day === d);

  let sent = 0;
  for (const p of people) {
    if (p.last_reminded === today || !did(p.id, yesterday) || did(p.id, today)) continue;
    const name = p.name ? ` ${p.name}` : "";
    const mail = await fetch("https://api.resend.com/emails", {
      method: "POST",
      headers: { authorization: `Bearer ${RESEND_API_KEY}`, "content-type": "application/json" },
      body: JSON.stringify({
        from: REMINDER_FROM,
        to: p.email,
        subject: "🔥 Keep your Vyrn streak going",
        html: `<p>Hi${name},</p><p>You learned yesterday, so your streak is still alive. One lesson today keeps it going; most take 10 minutes.</p>
          <p><a href="${SITE}/#/">Continue learning →</a></p>
          <p style="color:#777;font-size:12px">You get this because reminders are on. Turn them off on <a href="${SITE}/#/me">Your progress</a>.</p>`,
      }),
    });
    if (mail.ok) {
      sent++;
      await db(`profiles?id=eq.${p.id}`, { method: "PATCH", body: JSON.stringify({ last_reminded: today }) });
    }
  }
  return res.status(200).json({ sent });
}
