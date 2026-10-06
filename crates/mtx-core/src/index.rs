//! `files.idx`: a memory-mappable map from file basename to the packages
//! that provide it.
//!
//! The layout is fixed little-endian with offsets instead of pointers, so a
//! C reader inside kpathsea (Phase 1) can use it after a single `mmap`:
//!
//! ```text
//! header   64 bytes, see `Header`
//! hashes   n_entries × u64   FNV-1a of the ASCII-lowercased basename, sorted
//! entries  n_entries × {u32 basename_off, u32 dir_idx, u32 pkg_idx}  (parallel to hashes)
//! dirs     n_dirs    × u32   string offset of a root-relative directory
//! pkgs     n_pkgs    × {u32 name_off, u32 revision, u32 container_size, u32 flags}  (sorted by name)
//! strings  NUL-terminated strings; offsets are relative to this section
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use memmap2::Mmap;

use crate::tlpdb::Tlpdb;

pub const MAGIC: &[u8; 8] = b"MTXIDX\0\x01";
pub const VERSION: u32 = 1;
const HEADER_LEN: usize = 64;
const ENTRY_LEN: usize = 12;
const PKG_LEN: usize = 16;

pub mod flags {
    pub const HAS_MAPS: u32 = 1 << 0;
    pub const HAS_FORMATS: u32 = 1 << 1;
    pub const HAS_HYPHEN: u32 = 1 << 2;
    /// Pre-release packages such as `latex-base-dev`; never the first choice.
    pub const IS_DEV: u32 = 1 << 3;
}

/// FNV-1a over the ASCII-lowercased name; the C side must match exactly.
pub fn name_hash(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b.to_ascii_lowercase());
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Serialize the index for every content package in `db`.
pub fn build(db: &Tlpdb) -> Vec<u8> {
    let mut strings: Vec<u8> = Vec::new();
    let mut interned: HashMap<String, u32> = HashMap::new();
    let mut intern = |s: &str, strings: &mut Vec<u8>| -> u32 {
        if let Some(&off) = interned.get(s) {
            return off;
        }
        let off = strings.len() as u32;
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        interned.insert(s.to_string(), off);
        off
    };

    let pkgs: Vec<_> = db.content_packages().collect(); // BTreeMap order = sorted by name
    let mut pkg_records = Vec::with_capacity(pkgs.len());
    let mut dirs: Vec<u32> = Vec::new();
    let mut dir_ids: HashMap<String, u32> = HashMap::new();
    let mut entries: Vec<(u64, u32, u32, u32)> = Vec::new();

    for (pkg_idx, p) in pkgs.iter().enumerate() {
        let mut f = 0;
        if ["addMap", "addMixedMap", "addKanjiMap"].iter().any(|k| p.executes_of(k).next().is_some()) {
            f |= flags::HAS_MAPS;
        }
        if p.executes_of("AddFormat").next().is_some() {
            f |= flags::HAS_FORMATS;
        }
        if p.executes_of("AddHyphen").next().is_some() {
            f |= flags::HAS_HYPHEN;
        }
        if p.name.ends_with("-dev") {
            f |= flags::IS_DEV;
        }
        let name_off = intern(&p.name, &mut strings);
        pkg_records.push((name_off, p.revision as u32, p.container_size.min(u64::from(u32::MAX)) as u32, f));

        for path in &p.runfiles {
            let (dir, base) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
            let dir_idx = match dir_ids.get(dir) {
                Some(&i) => i,
                None => {
                    let i = dirs.len() as u32;
                    dirs.push(intern(dir, &mut strings));
                    dir_ids.insert(dir.to_string(), i);
                    i
                }
            };
            let base_off = intern(base, &mut strings);
            entries.push((name_hash(base), base_off, dir_idx, pkg_idx as u32));
        }
    }
    entries.sort_unstable();

    let off_hashes = HEADER_LEN;
    let off_entries = off_hashes + entries.len() * 8;
    let off_dirs = off_entries + entries.len() * ENTRY_LEN;
    let off_pkgs = off_dirs + dirs.len() * 4;
    let off_strings = off_pkgs + pkg_records.len() * PKG_LEN;
    let total = off_strings + strings.len();

    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(MAGIC);
    for v in [VERSION, entries.len() as u32, dirs.len() as u32, pkg_records.len() as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&db.config.revision.to_le_bytes());
    for v in [off_hashes, off_entries, off_dirs, off_pkgs, off_strings, strings.len()] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    out.resize(HEADER_LEN, 0);
    for e in &entries {
        out.extend_from_slice(&e.0.to_le_bytes());
    }
    for e in &entries {
        for v in [e.1, e.2, e.3] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    for d in &dirs {
        out.extend_from_slice(&d.to_le_bytes());
    }
    for p in &pkg_records {
        for v in [p.0, p.1, p.2, p.3] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&strings);
    debug_assert_eq!(out.len(), total);
    out
}

/// Write `bytes` to `path` atomically, so readers that already mapped the
/// old file keep a consistent view.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("idx.tmp");
    fs::write(&tmp, bytes).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("renaming to {}", path.display()))?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct PkgInfo<'a> {
    pub name: &'a str,
    pub revision: u32,
    pub container_size: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Hit<'a> {
    pub dir: &'a str,
    pub basename: &'a str,
    pub pkg: u32,
}

impl Hit<'_> {
    pub fn path(&self) -> String {
        if self.dir.is_empty() { self.basename.to_string() } else { format!("{}/{}", self.dir, self.basename) }
    }
}

pub struct Index {
    map: Mmap,
    n_entries: usize,
    n_dirs: usize,
    n_pkgs: usize,
    off_hashes: usize,
    off_entries: usize,
    off_dirs: usize,
    off_pkgs: usize,
    off_strings: usize,
    len_strings: usize,
    pub tlpdb_revision: u64,
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

impl Index {
    pub fn open(path: &Path) -> Result<Index> {
        let file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        // SAFETY: the file is only ever replaced by rename, never modified in place.
        let map = unsafe { Mmap::map(&file)? };
        Index::from_map(map).with_context(|| format!("reading {}", path.display()))
    }

    fn from_map(map: Mmap) -> Result<Index> {
        let b = &map[..];
        if b.len() < HEADER_LEN || &b[..8] != MAGIC {
            bail!("not an mtx index");
        }
        if u32_at(b, 8) != VERSION {
            bail!("unsupported index version {}", u32_at(b, 8));
        }
        let idx = Index {
            n_entries: u32_at(b, 12) as usize,
            n_dirs: u32_at(b, 16) as usize,
            n_pkgs: u32_at(b, 20) as usize,
            tlpdb_revision: u64_at(b, 24),
            off_hashes: u32_at(b, 32) as usize,
            off_entries: u32_at(b, 36) as usize,
            off_dirs: u32_at(b, 40) as usize,
            off_pkgs: u32_at(b, 44) as usize,
            off_strings: u32_at(b, 48) as usize,
            len_strings: u32_at(b, 52) as usize,
            map,
        };
        if idx.off_strings + idx.len_strings != idx.map.len()
            || idx.off_entries != idx.off_hashes + idx.n_entries * 8
            || idx.off_dirs != idx.off_entries + idx.n_entries * ENTRY_LEN
            || idx.off_pkgs != idx.off_dirs + idx.n_dirs * 4
            || idx.off_strings != idx.off_pkgs + idx.n_pkgs * PKG_LEN
        {
            bail!("corrupt index layout");
        }
        Ok(idx)
    }

    fn string(&self, off: u32) -> &str {
        let s = &self.map[self.off_strings + off as usize..self.off_strings + self.len_strings];
        let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        std::str::from_utf8(&s[..end]).unwrap_or("")
    }

    fn hash(&self, i: usize) -> u64 {
        u64_at(&self.map, self.off_hashes + i * 8)
    }

    fn entry(&self, i: usize) -> Hit<'_> {
        let o = self.off_entries + i * ENTRY_LEN;
        let dir_idx = u32_at(&self.map, o + 4) as usize;
        Hit {
            basename: self.string(u32_at(&self.map, o)),
            dir: self.string(u32_at(&self.map, self.off_dirs + dir_idx * 4)),
            pkg: u32_at(&self.map, o + 8),
        }
    }

    pub fn package_count(&self) -> usize {
        self.n_pkgs
    }

    pub fn file_count(&self) -> usize {
        self.n_entries
    }

    pub fn package(&self, idx: u32) -> PkgInfo<'_> {
        let o = self.off_pkgs + idx as usize * PKG_LEN;
        PkgInfo {
            name: self.string(u32_at(&self.map, o)),
            revision: u32_at(&self.map, o + 4),
            container_size: u32_at(&self.map, o + 8),
            flags: u32_at(&self.map, o + 12),
        }
    }

    /// Binary search by package name.
    pub fn package_index(&self, name: &str) -> Option<u32> {
        let (mut lo, mut hi) = (0usize, self.n_pkgs);
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.package(mid as u32).name.cmp(name) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return Some(mid as u32),
            }
        }
        None
    }

    /// All entries named exactly `basename`; if there are none, entries
    /// matching case-insensitively (like kpathsea's `texmf_casefold_search`).
    pub fn lookup(&self, basename: &str) -> Vec<Hit<'_>> {
        let h = name_hash(basename);
        let (mut lo, mut hi) = (0usize, self.n_entries);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.hash(mid) < h { lo = mid + 1 } else { hi = mid }
        }
        let candidates: Vec<Hit<'_>> =
            (lo..self.n_entries).take_while(|&i| self.hash(i) == h).map(|i| self.entry(i)).collect();
        let exact: Vec<_> = candidates.iter().copied().filter(|e| e.basename == basename).collect();
        if !exact.is_empty() {
            return exact;
        }
        candidates.into_iter().filter(|e| e.basename.eq_ignore_ascii_case(basename)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_db() -> Tlpdb {
        Tlpdb::parse(
            "name amsmath\ncategory Package\nrevision 7\nrelocated 1\ncontainersize 100\nrunfiles size=1\n RELOC/tex/latex/amsmath/amsmath.sty\n RELOC/tex/latex/amsmath/amsbsy.sty\n\n\
             name latex-amsmath-dev\ncategory Package\nrevision 8\nrelocated 1\nrunfiles size=1\n RELOC/tex/latex-dev/amsmath/amsmath.sty\n\n\
             name lm\ncategory Package\nrevision 9\nexecute addMap lm.map\nrunfiles size=1\n texmf-dist/fonts/tfm/public/lm/Rm-LMRoman10.tfm\n\n\
             name scheme-x\ncategory Scheme\nrevision 1\ndepend lm\n",
        )
        .unwrap()
    }

    #[test]
    fn round_trip_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("files.idx");
        write_atomic(&path, &build(&sample_db())).unwrap();
        let idx = Index::open(&path).unwrap();
        assert_eq!(idx.package_count(), 3); // the scheme is not a content package
        assert_eq!(idx.file_count(), 4);

        let hits = idx.lookup("amsmath.sty");
        let mut got: Vec<_> = hits.iter().map(|h| (h.path(), idx.package(h.pkg).name)).collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                ("texmf-dist/tex/latex-dev/amsmath/amsmath.sty".to_string(), "latex-amsmath-dev"),
                ("texmf-dist/tex/latex/amsmath/amsmath.sty".to_string(), "amsmath"),
            ]
        );
        let dev = idx.package(idx.package_index("latex-amsmath-dev").unwrap());
        assert_ne!(dev.flags & flags::IS_DEV, 0);
        let lm = idx.package(idx.package_index("lm").unwrap());
        assert_ne!(lm.flags & flags::HAS_MAPS, 0);
        assert!(idx.lookup("nonexistent.sty").is_empty());
    }

    #[test]
    fn casefold_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("files.idx");
        write_atomic(&path, &build(&sample_db())).unwrap();
        let idx = Index::open(&path).unwrap();
        let hits = idx.lookup("rm-lmroman10.TFM");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].basename, "Rm-LMRoman10.tfm");
    }

    #[test]
    fn rejects_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.idx");
        fs::write(&path, b"not an index at all, definitely not one").unwrap();
        assert!(Index::open(&path).is_err());
    }
}
