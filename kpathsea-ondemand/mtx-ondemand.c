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
   recursion while it holds its install lock.  */

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
} mtx;

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

/* Run `mtx ensure` for HIT and NAME. Every line it prints is a file it
   made available; insert them all into the db. Return the first, which is
   the requested file, if it is readable. */
static string
mtx_run (kpathsea kpse, const mtx_hit *hit, const_string name)
{
  string loc = kpathsea_var_value (kpse, "SELFAUTOLOC");
  string prog = loc ? concat (loc, "/mtx") : NULL;
  string out = NULL, ret = NULL;
  size_t out_len = 0;
  int pipefd[2], devnull;
  pid_t pid;
  char *argv[10];

  free (loc);
  if (!prog || pipe (pipefd) != 0) {
    free (prog);
    return NULL;
  }
  argv[0] = prog;
  argv[1] = (char *) "ensure";
  argv[2] = (char *) "--package";
  argv[3] = (char *) hit->pkg;
  argv[4] = (char *) "--path";
  argv[5] = hit->rel;
  argv[6] = (char *) "--siblings";
  argv[7] = (char *) "--";
  argv[8] = (char *) name;
  argv[9] = NULL;

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
    if (out && WIFEXITED (status) && WEXITSTATUS (status) == 0) {
      string line = out, nl;
      out[out_len] = 0;
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
    }
  }
  close (pipefd[0]);
  free (out);
  free (prog);
  return ret;
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

  if (!mtx_format_allowed (format) || !name || !mtx_name_ok (name))
    return NULL;
  for (i = 0; i < STR_LIST_LENGTH (mtx.failed); i++)
    if (STREQ (STR_LIST_ELT (mtx.failed, i), name))
      return NULL;
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
    if (kpathsea_readable_file (kpse, best.path)) {
      /* Installed already (by another process, or as a sibling): just not
         in this process's ls-R view yet. */
      kpathsea_db_insert (kpse, best.path);
      ret = xstrdup (best.path);
    } else {
      setting = kpathsea_var_value (kpse, "MTX_AUTOINSTALL");
      if (!setting || (*setting != '0' && *setting != 'n' && *setting != 'f'))
        ret = mtx_run (kpse, &best, name);
      free (setting);
      if (!ret)
        str_list_add (&mtx.failed, xstrdup (name));
    }
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
