/* mtx-ondemand.c: install missing files on demand (MennoTeX).

   Included at the end of tex-make.c by the MennoTeX kpathsea patch, so the
   TeX Live build system needs no changes. Public domain.

   When a lookup finds nothing, kpathsea_find_file_generic calls
   kpathsea_ondemand_find. It looks the name up in the mtx file index
   ($MTX_INDEX, default $TEXMFROOT/tlpkg/mtx/files.idx): a memory-mapped,
   sorted table from file basenames to the TeX Live packages that ship
   them (layout: crates/mtx-core/src/index.rs in MennoTeX). For hits inside
   this format's search path it runs

     $SELFAUTOLOC/mtx ensure --package PKG --path REL --siblings -- NAME

   which installs the package and prints the file's path, followed by every
   other file the installation added. All printed paths go into the
   in-memory ls-R database, so later lookups of sibling files (a package's
   .lua next to its .sty, its dependencies) succeed without another run.

   Unlike kpathsea_make_tex this runs whatever must_exist says: \openin,
   Lua's kpse.find_file and all font, map and encoding lookups pass false.
   A miss costs one binary search in the index, so doing it always is cheap.

   Set MTX_AUTOINSTALL=0 (environment or texmf.cnf, also per program as
   MTX_AUTOINSTALL.kpsewhich) to only return files that are already
   installed. mtx sets it for the tools it runs itself, which prevents
   recursion while it holds its install lock.

   When a package that has the file is not installed (declined, the install
   failed, or MTX_AUTOINSTALL is off), a one-line warning is queued for the
   engine to print in TeX's log (kpathsea_ondemand_problem): mtx's own
   message goes to stderr only, and editors read the log. Engines register
   a printer (kpathsea_ondemand_set_printer) that prints it right away: a
   missing package makes LaTeX stop before it opens another file.  */

#include <kpathsea/concatn.h>
#include <kpathsea/pathsearch.h>
#include <kpathsea/str-list.h>
#include <kpathsea/tex-file.h>

#include <errno.h>
#include <stdint.h>
#include <strings.h>
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define MTX_MAGIC "MTXIDX\0\1"
#define MTX_VERSION 1
#define MTX_HEADER_LEN 64
#define MTX_ENTRY_LEN 12
#define MTX_PKG_LEN 16
#define MTX_FLAG_IS_DEV 8u

static struct {
  int state;                    /* 0 not loaded, 1 ready, -1 unavailable */
  const unsigned char *base;
  size_t len;
  size_t n_entries, n_dirs, n_pkgs;
  size_t off_hashes, off_entries, off_dirs, off_pkgs, off_strings, len_strings;
  string root;                  /* TEXMFROOT; index paths are relative to it */
  str_list_type failed;         /* names we could not install in this run */
  str_list_type problems;       /* warnings not yet printed by the engine */
} mtx;

/* mtx ensure's exit status when it did not install (crates/mtx-core/src/
   ensure.rs, NotInstalled); 1 means no package has the file, 2 an error. */
#define MTX_EXIT_DECLINED 3
#define MTX_EXIT_FAILED 4

static void (*mtx_printer) (void);

void
kpathsea_ondemand_set_printer (void (*printer) (void))
{
  mtx_printer = printer;
}

/* Queue the warning that WHAT ("package foo (for foo.sty)", freed here)
   was not installed. STATUS is mtx's exit status, or -1 when
   MTX_AUTOINSTALL kept mtx from running. The format makes editors (LaTeX
   Workshop, latexmk's summary) list it as a package warning. */
static void
mtx_problem (string what, int status)
{
  const_string why;
  string msg;
  if (status == 1) {
    free (what);
    return;                     /* nothing provides it after all */
  }
  if (status == -1)
    why = "was not installed: MTX_AUTOINSTALL is off";
  else if (status == MTX_EXIT_DECLINED)
    why = "was not installed: declined; see `mtx log'";
  else
    why = "could not be installed; see `mtx log'";
  msg = concatn ("Package mtx Warning: ", what, " ", why, ".", NULL);
  str_list_add (&mtx.problems, msg);
  free (what);
  if (mtx_printer)
    mtx_printer ();
}

/* The oldest queued warning (the caller frees it), or NULL. */
string
kpathsea_ondemand_problem (void)
{
  string msg;
  unsigned i;
  if (STR_LIST_LENGTH (mtx.problems) == 0)
    return NULL;
  msg = STR_LIST_ELT (mtx.problems, 0);
  for (i = 1; i < STR_LIST_LENGTH (mtx.problems); i++)
    STR_LIST_ELT (mtx.problems, i - 1) = STR_LIST_ELT (mtx.problems, i);
  STR_LIST_LENGTH (mtx.problems)--;
  if (STR_LIST_LENGTH (mtx.problems) == 0)
    str_list_free (&mtx.problems);
  return msg;
}

static uint32_t
mtx_u32 (size_t off)
{
  const unsigned char *p = mtx.base + off;
  return (uint32_t) p[0] | (uint32_t) p[1] << 8 | (uint32_t) p[2] << 16
         | (uint32_t) p[3] << 24;
}

static uint64_t
mtx_u64 (size_t off)
{
  return (uint64_t) mtx_u32 (off) | (uint64_t) mtx_u32 (off + 4) << 32;
}

static const char *
mtx_str (uint32_t off)
{
  /* Strings are NUL-terminated; the section ends with a NUL. */
  return off < mtx.len_strings ? (const char *) mtx.base + mtx.off_strings + off : "";
}

/* FNV-1a over the ASCII-lowercased name; must match index.rs name_hash. */
static uint64_t
mtx_hash (const char *s)
{
  uint64_t h = 0xcbf29ce484222325ULL;
  for (; *s; s++) {
    unsigned char c = (unsigned char) *s;
    if (c >= 'A' && c <= 'Z')
      c += 'a' - 'A';
    h ^= c;
    h *= 0x00000100000001b3ULL;
  }
  return h;
}

static boolean
mtx_load (kpathsea kpse)
{
  string path;
  int fd;
  struct stat st;
  void *m;

  if (mtx.state)
    return mtx.state > 0;
  mtx.state = -1;

  mtx.root = kpathsea_var_value (kpse, "TEXMFROOT");
  if (!mtx.root || !*mtx.root)
    return false;
  path = kpathsea_var_value (kpse, "MTX_INDEX");
  if (!path || !*path) {
    free (path);
    path = concat (mtx.root, "/tlpkg/mtx/files.idx");
  }
  fd = open (path, O_RDONLY);
  free (path);
  if (fd < 0)
    return false;
  if (fstat (fd, &st) != 0 || st.st_size < MTX_HEADER_LEN) {
    close (fd);
    return false;
  }
  /* mtx replaces the index by rename, so this mapping stays consistent. */
  m = mmap (NULL, (size_t) st.st_size, PROT_READ, MAP_SHARED, fd, 0);
  close (fd);
  if (m == MAP_FAILED)
    return false;
  mtx.base = m;
  mtx.len = (size_t) st.st_size;

  if (memcmp (mtx.base, MTX_MAGIC, 8) != 0 || mtx_u32 (8) != MTX_VERSION)
    goto bad;
  mtx.n_entries = mtx_u32 (12);
  mtx.n_dirs = mtx_u32 (16);
  mtx.n_pkgs = mtx_u32 (20);
  mtx.off_hashes = mtx_u32 (32);
  mtx.off_entries = mtx_u32 (36);
  mtx.off_dirs = mtx_u32 (40);
  mtx.off_pkgs = mtx_u32 (44);
  mtx.off_strings = mtx_u32 (48);
  mtx.len_strings = mtx_u32 (52);
  if (mtx.off_hashes != MTX_HEADER_LEN
      || mtx.off_entries != mtx.off_hashes + mtx.n_entries * 8
      || mtx.off_dirs != mtx.off_entries + mtx.n_entries * MTX_ENTRY_LEN
      || mtx.off_pkgs != mtx.off_dirs + mtx.n_dirs * 4
      || mtx.off_strings != mtx.off_pkgs + mtx.n_pkgs * MTX_PKG_LEN
      || mtx.off_strings + mtx.len_strings != mtx.len
      || mtx.len_strings == 0
      || mtx.base[mtx.len - 1] != 0)
    goto bad;
  mtx.state = 1;
  return true;

 bad:
  munmap ((void *) mtx.base, mtx.len);
  mtx.base = NULL;
  return false;
}

/* Formats whose lookups must never install anything: configuration and
   databases (recursion during startup), formats (mktexfmt builds them),
   generated bitmap fonts, and pool files. */
/* Whether mtx's install of PKG was interrupted (a crash or kill -9): its
   journal entry stays until the whole install, including ls-R and font
   maps, is done. Its files may then be in place without the rest, so a
   lookup asks mtx to finish the install instead of just taking the file. */
static boolean
mtx_interrupted (const_string pkg)
{
  struct stat st;
  string path = concat3 (mtx.root, "/tlpkg/mtx/journal/", pkg);
  boolean ret = stat (path, &st) == 0;
  free (path);
  return ret;
}

static boolean
mtx_format_allowed (kpse_file_format_type format)
{
  switch (format) {
  case kpse_cnf_format: case kpse_db_format:
  case kpse_fmt_format: case kpse_base_format: case kpse_mem_format:
  case kpse_gf_format: case kpse_pk_format: case kpse_any_glyph_format:
  case kpse_texpool_format: case kpse_mfpool_format: case kpse_mppool_format:
    return false;
  default:
    return true;
  }
}

/* Same rules as kpathsea_make_tex: names that could be shell or path
   tricks are never passed on. */
static boolean
mtx_name_ok (const_string name)
{
  const_string p;
  if (!name[0] || name[0] == '-' || IS_DIR_SEP (name[0]) || strstr (name, ".."))
    return false;
  for (p = name; *p; p++)
    if (!ISALNUM (*p) && *p != '-' && *p != '+' && *p != '_' && *p != '.'
        && !IS_DIR_SEP (*p))
      return false;
  return true;
}

static boolean
mtx_has_suffix (const_string name, const_string *suffixes)
{
  size_t n = strlen (name);
  for (; suffixes && *suffixes; suffixes++) {
    size_t s = strlen (*suffixes);
    if (n >= s && FILESTRCASEEQ (name + n - s, *suffixes))
      return true;
  }
  return false;
}

/* Position of the first element of FORMAT's search path that FILE (an
   absolute path) falls under, or -1. Uses kpathsea's own // matching. */
static int
mtx_path_rank (kpathsea kpse, kpse_file_format_type format, const_string file)
{
  int rank = 0;
  string elt;
  for (elt = kpathsea_path_element (kpse, kpse->format_info[format].path);
       elt; elt = kpathsea_path_element (kpse, NULL), rank++) {
    if (elt[0] == '!' && elt[1] == '!')
      elt += 2;
    if (*elt && kpathsea_path_elt_match (file, elt))
      return rank;
  }
  return -1;
}

typedef struct {
  int rank, dev;
  uint32_t size;
  string path;                  /* absolute */
  string rel;                   /* root-relative */
  const char *pkg;
} mtx_hit;

/* Best index entry for one candidate basename (with optional directory
   suffix SUBDIR), or a hit with path == NULL. */
static mtx_hit
mtx_lookup (kpathsea kpse, kpse_file_format_type format, const_string base,
            const_string subdir)
{
  mtx_hit best = { -1, 0, 0, NULL, NULL, NULL };
  uint64_t h = mtx_hash (base);
  size_t lo = 0, hi = mtx.n_entries, i;
  int pass;

  while (lo < hi) {
    size_t mid = lo + (hi - lo) / 2;
    if (mtx_u64 (mtx.off_hashes + mid * 8) < h) lo = mid + 1; else hi = mid;
  }
  /* Exact matches first; case-insensitive ones only if there are none
     (like texmf_casefold_search). */
  for (pass = 0; pass < 2 && !best.path; pass++) {
    for (i = lo; i < mtx.n_entries && mtx_u64 (mtx.off_hashes + i * 8) == h; i++) {
      size_t e = mtx.off_entries + i * MTX_ENTRY_LEN;
      const char *b = mtx_str (mtx_u32 (e));
      uint32_t dir_idx = mtx_u32 (e + 4), pkg_idx = mtx_u32 (e + 8);
      const char *dir;
      size_t p;
      mtx_hit hit;

      if (dir_idx >= mtx.n_dirs || pkg_idx >= mtx.n_pkgs)
        continue;
      if (pass == 0 ? strcmp (b, base) != 0 : strcasecmp (b, base) != 0)
        continue;
      dir = mtx_str (mtx_u32 (mtx.off_dirs + dir_idx * 4));
      if (subdir) {
        size_t dl = strlen (dir), sl = strlen (subdir);
        if (dl <= sl || dir[dl - sl - 1] != '/' || strcmp (dir + dl - sl, subdir) != 0)
          continue;
      }
      p = mtx.off_pkgs + pkg_idx * MTX_PKG_LEN;
      hit.rel = concat3 (dir, "/", b);
      hit.path = concat3 (mtx.root, "/", hit.rel);
      hit.rank = mtx_path_rank (kpse, format, hit.path);
      hit.pkg = mtx_str (mtx_u32 (p));
      hit.size = mtx_u32 (p + 8);
      hit.dev = (mtx_u32 (p + 12) & MTX_FLAG_IS_DEV) != 0;
      if (hit.rank >= 0
          && (!best.path || hit.rank < best.rank
              || (hit.rank == best.rank && (hit.dev < best.dev
                  || (hit.dev == best.dev && hit.size < best.size))))) {
        free (best.path);
        free (best.rel);
        best = hit;
      } else {
        free (hit.path);
        free (hit.rel);
      }
    }
  }
  return best;
}

/* Directories created by an install are invisible to kpathsea's cached
   `//` expansions (elt-dirs.c), which luaotfload, for example, uses to
   rescan its fonts. Forget them; they are rebuilt on demand. The cached
   lists are leaked on purpose, since a caller may still hold one. */
static void
mtx_flush_dir_cache (kpathsea kpse)
{
  unsigned i;
  for (i = 0; i < kpse->cache_length; i++)
    free ((string) kpse->the_cache[i].key);
  kpse->cache_length = 0;
}

/* Successful mtx runs in this process. An install can change the font
   maps (updmap runs inside it), and pdfTeX reads pdftex.map only once: its
   map lookup re-reads the map on a miss when this count has changed since
   (MennoTeX pdfTeX patch), so a font package installed after the first
   page still gets its map entries. */
static unsigned mtx_generation;

unsigned
kpathsea_ondemand_generation (void)
{
  return mtx_generation;
}

/* Run mtx with ARGV (ARGV[0] is replaced by $SELFAUTOLOC/mtx). Every line
   it prints is a file it made available; insert them all into the db.
   Return the first, if readable. Store mtx's exit status in *STATUS (2 if
   it could not run). */
static string
mtx_run (kpathsea kpse, char **argv, int *status_out)
{
  string loc = kpathsea_var_value (kpse, "SELFAUTOLOC");
  string prog = loc ? concat (loc, "/mtx") : NULL;
  string out = NULL, ret = NULL;
  size_t out_len = 0;
  int pipefd[2], devnull;
  pid_t pid;

  free (loc);
  *status_out = 2;
  if (!prog || pipe (pipefd) != 0) {
    free (prog);
    return NULL;
  }
  argv[0] = prog;

  pid = fork ();
  if (pid == 0) {
    /* Child: stdin from /dev/null, stdout to the pipe, stderr shared so
       the user sees what is being installed. */
    devnull = open ("/dev/null", O_RDONLY);
    if (devnull >= 0) { dup2 (devnull, 0); close (devnull); }
    dup2 (pipefd[1], 1);
    close (pipefd[0]);
    close (pipefd[1]);
    execv (prog, argv);
    _exit (127);
  }
  close (pipefd[1]);
  if (pid > 0) {
    char buf[4096];
    ssize_t n;
    int status;
    for (;;) {
      n = read (pipefd[0], buf, sizeof buf);
      if (n < 0 && errno == EINTR)
        continue;
      if (n <= 0)
        break;
      out = (string) xrealloc (out, out_len + (size_t) n + 1);
      memcpy (out + out_len, buf, (size_t) n);
      out_len += (size_t) n;
    }
    while (waitpid (pid, &status, 0) < 0 && errno == EINTR)
      ;
    if (WIFEXITED (status))
      *status_out = WEXITSTATUS (status);
    if (out && WIFEXITED (status) && WEXITSTATUS (status) == 0) {
      string line = out, nl;
      out[out_len] = 0;
      mtx_generation++;
      for (; *line; line = nl + 1) {
        nl = strchr (line, '\n');
        if (!nl)
          nl = line + strlen (line);
        if (nl > line) {
          char save = *nl;
          *nl = 0;
          kpathsea_db_insert (kpse, line);
          if (!ret)
            ret = kpathsea_readable_file (kpse, line) ? xstrdup (line) : NULL;
          *nl = save;
        }
        if (!*nl)
          break;
      }
      mtx_flush_dir_cache (kpse);
    }
  }
  close (pipefd[0]);
  free (out);
  free (prog);
  return ret;
}

/* Formats whose lookups may carry a font *name* ("TeX Gyre Pagella")
   instead of a file name: XeTeX (MennoTeX patch) asks for OpenType and
   TrueType fonts by name, and luaotfload probes a name as a TFM before it
   rescans its font database. */
static boolean
mtx_font_format (kpse_file_format_type format)
{
  return format == kpse_opentype_format || format == kpse_truetype_format
         || format == kpse_tfm_format || format == kpse_ofm_format;
}

static boolean
mtx_fontname_ok (const_string name)
{
  const_string p;
  if (!name[0] || name[0] == '-' || strlen (name) > 200)
    return false;
  for (p = name; *p; p++)
    if (!ISALNUM (*p) && *p != ' ' && *p != '-' && *p != '+' && *p != '_' && *p != '.')
      return false;
  return true;
}

/* Install the package providing the font named NAME and return its file.
   For TFM/OFM probes the font is installed but NULL is returned, so that
   luaotfload goes on to rescan its font database and finds it there. */
static string
mtx_find_font_name (kpathsea kpse, kpse_file_format_type format, const_string name)
{
  char *argv[6];
  string found;
  int status;
  argv[0] = NULL;
  argv[1] = (char *) "ensure";
  argv[2] = (char *) "--font-name";
  argv[3] = (char *) "--siblings";
  argv[4] = (char *) name;
  argv[5] = NULL;
  found = mtx_run (kpse, argv, &status);
  if (!found)
    mtx_problem (concat3 ("the font `", name, "'"), status);
  if (found && (format == kpse_tfm_format || format == kpse_ofm_format)) {
    free (found);
    return NULL;
  }
  return found;
}

/* Font maps. pdfTeX and LuaTeX find a font's outlines through the map
   file (pdftex.map: ecrm1000 -> sfrm1000.pfb), not through a lookup that
   names the outlines' package (cm-super). mtx installs that package with
   the font's TFMs; for roots where that did not happen, the engines call
   kpathsea_ondemand_font_map on a map miss. mtx's font-map table
   ($TEXMFROOT/tlpkg/mtx/fontmaps.tsv, lines "FONT<TAB>PACKAGES" sorted
   bytewise) is binary-searched, so virtual and METAFONT fonts, which no map
   covers, cost no process. */
static struct {
  int state;                    /* 0 not loaded, 1 ready, -1 unavailable */
  const char *base;
  size_t len;
  str_list_type tried;          /* fonts already asked about in this run */
} mtx_maps;

static boolean
mtx_maps_has (const_string font)
{
  size_t lo = 0, hi, n = strlen (font);
  const char *b;
  if (mtx_maps.state == 0) {
    string path = concat (mtx.root, "/tlpkg/mtx/fontmaps.tsv");
    int fd = open (path, O_RDONLY);
    struct stat st;
    void *m;
    free (path);
    mtx_maps.state = -1;
    if (fd >= 0) {
      if (fstat (fd, &st) == 0 && st.st_size > 0) {
        m = mmap (NULL, (size_t) st.st_size, PROT_READ, MAP_PRIVATE, fd, 0);
        if (m != MAP_FAILED) {
          mtx_maps.base = (const char *) m;
          mtx_maps.len = (size_t) st.st_size;
          mtx_maps.state = 1;
        }
      }
      close (fd);
    }
  }
  if (mtx_maps.state < 0)
    return false;
  b = mtx_maps.base;
  hi = mtx_maps.len;
  /* Lines starting in [lo, hi) may match; lo is always a line start. */
  while (lo < hi) {
    size_t start = lo + (hi - lo) / 2, end;
    int c;
    while (start > lo && b[start - 1] != '\n')
      start--;
    for (end = start; end < mtx_maps.len && b[end] != '\t' && b[end] != '\n'; end++)
      ;
    c = memcmp (b + start, font, end - start < n ? end - start : n);
    if (c == 0)
      c = (end - start > n) - (end - start < n);
    if (c == 0)
      return true;
    if (c < 0) {
      while (end < mtx_maps.len && b[end] != '\n')
        end++;
      lo = end + 1;
    } else
      hi = start;
  }
  return false;
}

/* FONT (a TFM name) has no entry in the font map. If a package's map
   covers it, have mtx install that package, which regenerates the maps;
   true if it did, and the engine should read its map again. Once per font
   per run. */
boolean
kpathsea_ondemand_font_map (kpathsea kpse, const_string font)
{
  string setting, found;
  char *argv[6];
  int status = -1;
  unsigned i;
  if (!font || !mtx_name_ok (font) || !mtx_load (kpse) || !mtx_maps_has (font))
    return false;
  for (i = 0; i < STR_LIST_LENGTH (mtx_maps.tried); i++)
    if (STREQ (STR_LIST_ELT (mtx_maps.tried, i), font))
      return false;
  str_list_add (&mtx_maps.tried, xstrdup (font));
  setting = kpathsea_var_value (kpse, "MTX_AUTOINSTALL");
  if (!setting || (*setting != '0' && *setting != 'n' && *setting != 'f')) {
    argv[0] = NULL;
    argv[1] = (char *) "ensure";
    argv[2] = (char *) "--font-map";
    argv[3] = (char *) "--siblings";
    argv[4] = (char *) font;
    argv[5] = NULL;
    found = mtx_run (kpse, argv, &status);
    free (found);
  }
  free (setting);
  if (status != 0)
    mtx_problem (concat ("the font map for ", font), status);
  return status == 0;
}

string
kpathsea_ondemand_find (kpathsea kpse, kpse_file_format_type format,
                        const_string name)
{
  const kpse_format_info_type *info = &kpse->format_info[format];
  string setting, base, subdir = NULL, ret = NULL;
  const_string slash, *sfx;
  str_list_type candidates;
  unsigned i;
  mtx_hit best = { -1, 0, 0, NULL, NULL, NULL };

  if (!mtx_format_allowed (format) || !name)
    return NULL;
  for (i = 0; i < STR_LIST_LENGTH (mtx.failed); i++)
    if (STREQ (STR_LIST_ELT (mtx.failed, i), name))
      return NULL;
  if (!mtx_name_ok (name)) {
    /* Not a file name, but maybe a font name ("TeX Gyre Pagella"). */
    if (mtx_font_format (format) && mtx_fontname_ok (name)) {
      setting = kpathsea_var_value (kpse, "MTX_AUTOINSTALL");
      if (!setting || (*setting != '0' && *setting != 'n' && *setting != 'f'))
        ret = mtx_find_font_name (kpse, format, name);
      free (setting);
      str_list_add (&mtx.failed, xstrdup (name)); /* ask mtx once per run */
      return ret;
    }
    return NULL;
  }
  if (!mtx_load (kpse))
    return NULL;

  /* `latex/foo.sty' style names: look up the basename, require the dir. */
  slash = strrchr (name, '/');
  base = xstrdup (slash ? slash + 1 : name);
  if (slash) {
    size_t n = (size_t) (slash - name);
    subdir = (string) xmalloc (n + 1);
    memcpy (subdir, name, n);
    subdir[n] = 0;
  }

  /* Candidate names in kpathsea's order: as given if it already has one of
     the format's suffixes, otherwise with each standard suffix first. */
  candidates = str_list_init ();
  if (!mtx_has_suffix (base, info->suffix) && !mtx_has_suffix (base, info->alt_suffix))
    for (sfx = info->suffix; sfx && *sfx; sfx++)
      str_list_add (&candidates, concat (base, *sfx));
  str_list_add (&candidates, xstrdup (base));

  for (i = 0; i < STR_LIST_LENGTH (candidates) && !best.path; i++)
    best = mtx_lookup (kpse, format, STR_LIST_ELT (candidates, i), subdir);

  if (best.path) {
    boolean readable = kpathsea_readable_file (kpse, best.path) != NULL;
    if (readable && !mtx_interrupted (best.pkg)) {
      /* Installed already (by another process, or as a sibling): just not
         in this process's ls-R view yet. */
      kpathsea_db_insert (kpse, best.path);
      ret = xstrdup (best.path);
    } else {
      int status = -1;
      setting = kpathsea_var_value (kpse, "MTX_AUTOINSTALL");
      if (!setting || (*setting != '0' && *setting != 'n' && *setting != 'f')) {
        char *argv[10];
        argv[0] = NULL;
        argv[1] = (char *) "ensure";
        argv[2] = (char *) "--package";
        argv[3] = (char *) best.pkg;
        argv[4] = (char *) "--path";
        argv[5] = best.rel;
        argv[6] = (char *) "--siblings";
        argv[7] = (char *) "--";
        argv[8] = (char *) name;
        argv[9] = NULL;
        ret = mtx_run (kpse, argv, &status);
      }
      free (setting);
      if (!ret && readable) {
        /* mtx could not finish the interrupted install; the file itself
           is complete (mtx renames files into place whole). */
        kpathsea_db_insert (kpse, best.path);
        ret = xstrdup (best.path);
      }
      if (!ret) {
        str_list_add (&mtx.failed, xstrdup (name));
        mtx_problem (concatn ("package ", best.pkg, " (for ", name, ")", NULL), status);
      }
    }
  } else if (mtx_font_format (format) && !slash) {
    /* One-word font names ("Inconsolata") look like file names but are in
       no package's file list. */
    setting = kpathsea_var_value (kpse, "MTX_AUTOINSTALL");
    if (!setting || (*setting != '0' && *setting != 'n' && *setting != 'f'))
      ret = mtx_find_font_name (kpse, format, name);
    free (setting);
    if (!ret)
      str_list_add (&mtx.failed, xstrdup (name));
  }

  for (i = 0; i < STR_LIST_LENGTH (candidates); i++)
    free (STR_LIST_ELT (candidates, i));
  str_list_free (&candidates);
  free (best.path);
  free (best.rel);
  free (base);
  free (subdir);
  return ret;
}
