/* Unit tests for mtx-ondemand.c, against the index fixture.idx (written by
   mtx's Rust code from fixture.tlpdb; crates/mtx-core/src/index.rs checks
   that it is current) and a fake `mtx` that records how it is called.
   Built and run by tests/run_c_tests.sh against a texlive-source build:

     tests/run_c_tests.sh <texlive-source checkout, built>

   The resolver is #included, so its static functions and state are
   reachable. libkpathsea has its own copy of the public functions; they
   are renamed here so the two do not clash.  Public domain.  */

#define kpathsea_ondemand_find test_ondemand_find
#define kpathsea_ondemand_generation test_ondemand_generation
#define kpathsea_ondemand_problem test_ondemand_problem
#define kpathsea_ondemand_set_printer test_ondemand_set_printer
#define kpathsea_ondemand_font_map test_ondemand_font_map

/* The headers tex-make.c includes before it includes the resolver. */
#include <kpathsea/config.h>
#include <kpathsea/absolute.h>
#include <kpathsea/c-fopen.h>
#include <kpathsea/c-pathch.h>
#include <kpathsea/db.h>
#include <kpathsea/fn.h>
#include <kpathsea/magstep.h>
#include <kpathsea/readable.h>
#include <kpathsea/tex-make.h>
#include <kpathsea/variable.h>
#include <sys/wait.h>

#include "../mtx-ondemand.c"

#include <stdio.h>
#include <stdlib.h>

static int printed;               /* calls of the printer hook */

static void
count_printer (void)
{
  printed++;
}

static int failures, checks;
static const char *root, *calls, *argv0;

#define CHECK(cond, ...)                                        \
  do {                                                          \
    checks++;                                                   \
    if (!(cond)) {                                              \
      failures++;                                               \
      fprintf (stderr, "FAIL %s:%d: ", __FILE__, __LINE__);      \
      fprintf (stderr, __VA_ARGS__);                            \
      fputc ('\n', stderr);                                     \
    }                                                           \
  } while (0)

/* The next queued warning for TeX's log is WANT (NULL: none). */
#define CHECK_PROBLEM(want)                                     \
  do {                                                          \
    const char *want_ = (want);                                 \
    string got_ = test_ondemand_problem ();                     \
    if (want_)                                                  \
      CHECK (got_ && STREQ (got_, want_), "warning %s, want %s", \
             got_ ? got_ : "(none)", want_);                    \
    else                                                        \
      CHECK (!got_, "unexpected warning %s", got_);             \
    free (got_);                                                \
  } while (0)

static kpathsea
instance (const char *progname)
{
  kpathsea kpse = kpathsea_new ();
  kpathsea_set_program_name (kpse, argv0, progname);
  /* Where the fake mtx is: set_program_name has just pointed SELFAUTOLOC
     at this program's directory. */
  setenv ("SELFAUTOLOC", getenv ("MTX_TEST_BIN"), 1);
  kpathsea_init_format (kpse, kpse_tex_format);
  kpathsea_init_format (kpse, kpse_tfm_format);
  return kpse;
}

/* The index entry the resolver would pick, as a root-relative path. */
static char *
pick (kpathsea kpse, kpse_file_format_type format, const char *name)
{
  mtx_hit hit = mtx_lookup (kpse, format, name, NULL);
  free (hit.path);
  return hit.rel;
}

static int
lines (const char *file)
{
  FILE *f = fopen (file, "r");
  int n = 0, c;
  if (!f)
    return 0;
  while ((c = getc (f)) != EOF)
    n += c == '\n';
  fclose (f);
  return n;
}

static int
exists (const char *rel)
{
  char *p = concat3 (root, "/", rel);
  int ok = access (p, F_OK) == 0;
  free (p);
  return ok;
}

static void
expect_pick (kpathsea kpse, kpse_file_format_type format, const char *name, const char *want)
{
  char *got = pick (kpse, format, name);
  if (want)
    CHECK (got && STREQ (got, want), "%s: picked %s, want %s", name, got ? got : "(none)", want);
  else
    CHECK (!got, "%s: picked %s, want none", name, got);
  free (got);
}

int
main (int argc, char **argv)
{
  kpathsea latex, xelatex;
  char *p;
  unsigned gen;
  int n;

  (void) argc;
  argv0 = argv[0];
  root = getenv ("TEXMFROOT");
  calls = getenv ("MTX_TEST_CALLS");
  if (!root || !calls) {
    fprintf (stderr, "run through tests/run_c_tests.sh\n");
    return 2;
  }
  latex = instance ("pdflatex");
  xelatex = instance ("xelatex");

  /* --- The index: Rust writer, C reader. */
  CHECK (mtx_load (latex), "fixture.idx does not load");
  CHECK (mtx.n_entries == 13 && mtx.n_pkgs == 10, "entries %zu, packages %zu", mtx.n_entries, mtx.n_pkgs);
  expect_pick (latex, kpse_tex_format, "alpha.sty", "texmf-dist/tex/latex/alpha/alpha.sty");
  expect_pick (latex, kpse_tex_format, "nosuch.sty", NULL);
  /* Exact case wins; otherwise case-insensitive, like texmf_casefold_search. */
  expect_pick (latex, kpse_tex_format, "MixedCase.cfg", "texmf-dist/tex/latex/alpha/MixedCase.cfg");
  expect_pick (latex, kpse_tex_format, "mixedcase.cfg", "texmf-dist/tex/latex/mixedlower/mixedcase.cfg");
  expect_pick (latex, kpse_tex_format, "MIXEDCASE.CFG", "texmf-dist/tex/latex/alpha/MixedCase.cfg");
  /* Search order: XeLaTeX's tex/xelatex comes first; pdfLaTeX has no such
     directory, so only the generic copy is in its path. */
  expect_pick (xelatex, kpse_tex_format, "pstricks.con", "texmf-dist/tex/xelatex/xetex-pstricks/pstricks.con");
  expect_pick (latex, kpse_tex_format, "pstricks.con", "texmf-dist/tex/generic/pstricks/pstricks.con");
  /* Same rank: a -dev package loses, then the smaller package wins. */
  expect_pick (latex, kpse_tex_format, "same.sty", "texmf-dist/tex/latex/small/same.sty");
  expect_pick (latex, kpse_tex_format, "twin.sty", "texmf-dist/tex/latex/twin/twin.sty");
  /* Outside the format's path: a TFM is not a TeX input. */
  expect_pick (latex, kpse_tex_format, "fonty10.tfm", NULL);
  expect_pick (latex, kpse_tfm_format, "fonty10.tfm", "texmf-dist/fonts/tfm/public/fonty/fonty10.tfm");
  /* A directory part must match the end of the entry's directory. */
  {
    mtx_hit h = mtx_lookup (latex, kpse_tex_format, "alpha.sty", "alpha");
    CHECK (h.rel && STREQ (h.rel, "texmf-dist/tex/latex/alpha/alpha.sty"), "alpha/alpha.sty");
    free (h.path); free (h.rel);
    h = mtx_lookup (latex, kpse_tex_format, "alpha.sty", "beta");
    CHECK (!h.rel, "beta/alpha.sty matched %s", h.rel);
  }

  /* --- Names and formats that are never passed on. */
  CHECK (mtx_name_ok ("foo-bar_1.sty") && mtx_name_ok ("latex/foo.sty") && mtx_name_ok ("a+b.tex"), "plain names");
  CHECK (!mtx_name_ok ("../x.sty") && !mtx_name_ok ("-x") && !mtx_name_ok ("/etc/passwd")
         && !mtx_name_ok ("a b.sty") && !mtx_name_ok ("a;b") && !mtx_name_ok ("$(x)") && !mtx_name_ok (""),
         "unsafe names accepted");
  CHECK (mtx_fontname_ok ("TeX Gyre Pagella") && !mtx_fontname_ok ("a;b") && !mtx_fontname_ok ("-x"), "font names");
  CHECK (mtx_format_allowed (kpse_tex_format) && mtx_format_allowed (kpse_tfm_format), "tex, tfm allowed");
  CHECK (!mtx_format_allowed (kpse_pk_format) && !mtx_format_allowed (kpse_fmt_format)
         && !mtx_format_allowed (kpse_cnf_format) && !mtx_format_allowed (kpse_db_format), "pk, fmt, cnf, db refused");

  /* --- Installing: the fake mtx creates the file (and a sibling for
     alpha), prints the paths, and logs its arguments. */
  gen = test_ondemand_generation ();
  p = test_ondemand_find (latex, kpse_tex_format, "alpha.sty");
  CHECK (p && exists ("texmf-dist/tex/latex/alpha/alpha.sty"), "alpha.sty not installed (%s)", p ? p : "NULL");
  CHECK (lines (calls) == 1, "%d mtx calls, want 1", lines (calls));
  CHECK (exists ("texmf-dist/tex/latex/alpha/alpha.lua"), "sibling not made");
  CHECK (test_ondemand_generation () == gen + 1, "generation %u, want %u", test_ondemand_generation (), gen + 1);
  free (p);
  {
    FILE *f = fopen (calls, "r");
    char line[512] = "";
    if (f) { if (!fgets (line, sizeof line, f)) line[0] = 0; fclose (f); }
    CHECK (STREQ (line, "ensure --package alpha --path texmf-dist/tex/latex/alpha/alpha.sty --siblings -- alpha.sty\n"),
           "mtx called with: %s", line);
  }

  /* A readable file of a healthy package: no call. */
  n = lines (calls);
  p = test_ondemand_find (latex, kpse_tex_format, "alpha.sty");
  CHECK (p && lines (calls) == n, "installed file asked mtx again");
  free (p);

  /* A readable file of a journaled package: mtx finishes the install. */
  p = concat3 (root, "/tlpkg/mtx/journal/", "alpha");
  fclose (fopen (p, "w"));
  free (p);
  n = lines (calls);
  gen = test_ondemand_generation ();
  p = test_ondemand_find (latex, kpse_tex_format, "alpha.sty");
  CHECK (p && lines (calls) == n + 1, "journaled package not finished (%d calls)", lines (calls) - n);
  CHECK (test_ondemand_generation () == gen + 1, "generation not bumped after finishing");
  free (p);

  /* A failing mtx: NULL, and asked once per run, with one warning for
     TeX's log, announced to the engine's printer. With the file present
     and journaled, the complete file is still returned. */
  test_ondemand_set_printer (count_printer);
  CHECK_PROBLEM (NULL);
  n = lines (calls);
  p = test_ondemand_find (latex, kpse_tex_format, "broken.sty");
  CHECK (!p, "broken.sty returned %s", p);
  p = test_ondemand_find (latex, kpse_tex_format, "broken.sty");
  CHECK (!p && lines (calls) == n + 1, "failed name asked %d times", lines (calls) - n);
  CHECK (printed == 1, "printer called %d times, want 1", printed);
  CHECK_PROBLEM ("Package mtx Warning: package broken (for broken.sty) could not be installed; see `mtx log'.");
  CHECK_PROBLEM (NULL);
  /* Declined (exit 3), and "no package has it after all" (exit 1). */
  setenv ("MTX_TEST_EXIT", "3", 1);
  mtx.failed.length = 0;
  test_ondemand_find (latex, kpse_tex_format, "broken.sty");
  CHECK_PROBLEM ("Package mtx Warning: package broken (for broken.sty) was not installed: declined; see `mtx log'.");
  setenv ("MTX_TEST_EXIT", "1", 1);
  mtx.failed.length = 0;
  test_ondemand_find (latex, kpse_tex_format, "broken.sty");
  CHECK_PROBLEM (NULL);
  unsetenv ("MTX_TEST_EXIT");
  mtx.failed.length = 0;
  test_ondemand_find (latex, kpse_tex_format, "broken.sty");
  CHECK_PROBLEM ("Package mtx Warning: package broken (for broken.sty) could not be installed; see `mtx log'.");
  /* Warnings come out in order. */
  mtx_problem (xstrdup ("first"), 4);
  mtx_problem (xstrdup ("second"), 3);
  CHECK_PROBLEM ("Package mtx Warning: first could not be installed; see `mtx log'.");
  CHECK_PROBLEM ("Package mtx Warning: second was not installed: declined; see `mtx log'.");
  CHECK_PROBLEM (NULL);
  p = concat3 (root, "/texmf-dist/tex/latex/broken/", "");
  mkdir (p, 0777);
  free (p);
  p = concat3 (root, "/texmf-dist/tex/latex/broken/broken.sty", "");
  fclose (fopen (p, "w"));
  free (p);
  p = concat3 (root, "/tlpkg/mtx/journal/", "broken");
  fclose (fopen (p, "w"));
  free (p);
  {
    kpathsea fresh = instance ("pdflatex"); /* mtx.failed is per process; reset it */
    mtx.failed.length = 0;
    n = lines (calls);
    p = test_ondemand_find (fresh, kpse_tex_format, "broken.sty");
    CHECK (p && STREQ (p + strlen (p) - 10, "broken.sty") && lines (calls) == n + 1,
           "complete file of a journaled package not returned after mtx failed");
    free (p);
  }

  /* MTX_AUTOINSTALL=0: nothing is run. */
  setenv ("MTX_AUTOINSTALL", "0", 1);
  n = lines (calls);
  p = test_ondemand_find (latex, kpse_tfm_format, "fonty10.tfm");
  CHECK (!p && lines (calls) == n, "MTX_AUTOINSTALL=0 still ran mtx");
  CHECK_PROBLEM ("Package mtx Warning: package fonty (for fonty10.tfm) was not installed: MTX_AUTOINSTALL is off.");
  unsetenv ("MTX_AUTOINSTALL");
  /* The miss is remembered for the rest of the process (a TeX run cannot
     change the setting midway); a new process asks again. */
  p = test_ondemand_find (latex, kpse_tfm_format, "fonty10.tfm");
  CHECK (!p && lines (calls) == n, "a remembered miss ran mtx");
  mtx.failed.length = 0;
  p = test_ondemand_find (latex, kpse_tfm_format, "fonty10.tfm");
  CHECK (p && lines (calls) == n + 1, "fonty10.tfm not installed (%s, %d calls)", p ? p : "NULL", lines (calls) - n);
  free (p);

  /* Font maps: the table is binary-searched (first, last, a name that is
     a prefix of another); mtx runs only for fonts in it, once per run. */
  CHECK (mtx_maps_has ("a-first") && mtx_maps_has ("zz-last") && mtx_maps_has ("fonty10")
         && mtx_maps_has ("fonty10x") && mtx_maps_has ("nothing-to-do"), "table entries not found");
  CHECK (!mtx_maps_has ("fonty1") && !mtx_maps_has ("fonty100") && !mtx_maps_has ("a")
         && !mtx_maps_has ("zzz") && !mtx_maps_has (""), "names not in the table found");
  n = lines (calls);
  gen = test_ondemand_generation ();
  CHECK (!test_ondemand_font_map (latex, "cmr10") && lines (calls) == n, "a font in no map ran mtx");
  CHECK (test_ondemand_font_map (latex, "fonty10"), "fonty10's map package not installed");
  CHECK (lines (calls) == n + 1 && test_ondemand_generation () == gen + 1, "font map: %d calls, generation +%u",
         lines (calls) - n, test_ondemand_generation () - gen);
  {
    FILE *f = fopen (calls, "r");
    char line[512] = "", last[512] = "";
    while (fgets (line, sizeof line, f))
      strcpy (last, line);
    fclose (f);
    CHECK (STREQ (last, "ensure --font-map --siblings fonty10\n"), "font map call: %s", last);
  }
  CHECK (!test_ondemand_font_map (latex, "fonty10") && lines (calls) == n + 1, "font map asked twice");
  CHECK (!test_ondemand_font_map (latex, "nothing-to-do") && lines (calls) == n + 2, "nothing-to-do not asked");
  CHECK_PROBLEM (NULL); /* exit 1: nothing to install, no warning */

  /* Names refused before the index is consulted. */
  n = lines (calls);
  CHECK (!test_ondemand_find (latex, kpse_tex_format, "../alpha.sty"), "../alpha.sty");
  CHECK (!test_ondemand_find (latex, kpse_pk_format, "fonty10.600pk"), "pk lookup");
  CHECK (lines (calls) == n, "refused names ran mtx");

  printf ("%d checks, %d failed\n", checks, failures);
  return failures ? 1 : 0;
}
