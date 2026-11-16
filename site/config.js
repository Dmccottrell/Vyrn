// Where accounts are kept: a Supabase project (see site/supabase/schema.sql).
// The publishable key is meant to be public; the database's row-level
// security decides what each signed-in learner may read and write.
// Leave both empty to run the site without accounts (progress stays in the
// browser).
window.LEARN_VYRN_CONFIG = {
  supabaseUrl: "https://xxerfafjazgpkrrdsedr.supabase.co",
  supabaseKey: "sb_publishable_AgjbDHSQQgDFkO09Lmnx5A_VEtZ6i6R",
};
