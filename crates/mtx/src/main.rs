//! `mtx`: the MennoTeX package manager.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mtx_core::configfiles::Regen;
use mtx_core::ctx::{Ctx, Freshness};
use mtx_core::db::Reason;
use mtx_core::index::Index;
use mtx_core::root::Root;
use mtx_core::{binaries, bootstrap, ensure, install};

#[derive(Parser)]
#[command(name = "mtx", version, about = "MennoTeX package manager: TeX Live packages, installed on demand")]
struct Cli {
    /// Installation root (default: $MTX_ROOT, the root containing this
    /// executable, or ~/Library/MennoTeX/<release>).
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create (or repair) a minimal installation.
    Bootstrap {
        /// Repository URL or directory (default: mirror.ctan.org).
        #[arg(long)]
        repository: Option<String>,
    },
    /// Install the package providing a missing file and print its path
    /// (the kpathsea hook protocol: path on stdout, exit 1 if not found).
    Ensure {
        /// kpathsea format name, e.g. `tex`, `tfm`, `type1 fonts`
        /// (Phase 0 hooks: mtx chooses the file itself).
        #[arg(long, required_unless_present_any = ["package", "font_name"])]
        format: Option<String>,
        /// Package kpathsea chose (Phase 1 patch); requires --path.
        #[arg(long, requires = "path")]
        package: Option<String>,
        /// Root-relative path of the file kpathsea chose.
        #[arg(long)]
        path: Option<String>,
        /// Also print every other file this call installed.
        #[arg(long)]
        siblings: bool,
        /// NAME is a font name ("TeX Gyre Pagella"), not a file name.
        #[arg(long, conflicts_with_all = ["format", "package"])]
        font_name: bool,
        name: String,
    },
    /// Install MennoTeX-built (kpathsea-patched) binaries from a directory
    /// or a release archive (mennotex-bin-*.tar.xz) and switch on-demand
    /// installation to the kpathsea patch.
    InstallBinaries {
        /// Directory of programs, or a .tar.xz release archive.
        source: PathBuf,
        /// SHA256SUMS file to verify the archive against.
        #[arg(long)]
        sums: Option<PathBuf>,
    },
    /// Install packages (and their dependencies).
    Install { packages: Vec<String> },
    /// Remove packages.
    Remove {
        packages: Vec<String>,
        /// Remove even if other installed packages depend on them.
        #[arg(long)]
        force: bool,
    },
    /// Show which package provides a file.
    Which {
        name: String,
        #[arg(long, default_value = "tex")]
        format: String,
    },
    /// Show details about a package.
    Info { package: String },
    /// List installed packages.
    List,
    /// Check the mirror for a newer package database.
    Refresh,
    /// Upgrade all installed packages that have newer revisions.
    Update {
        /// Only list what would be upgraded.
        #[arg(long)]
        dry_run: bool,
    },
    /// Regenerate fmtutil.cnf, updmap.cfg, language.* and font maps.
    Regen,
    /// Check the installation and the environment for problems.
    Doctor,
    /// Fix what `mtx doctor` reports: missing font-map packages, interrupted
    /// installs, generated files, ls-R and shims.
    Repair,
    /// Install what a .tex file statically needs, in one go.
    Prefetch { file: PathBuf },
}

fn main() -> ExitCode {
    // Multi-call: kpathsea runs `mktextex NAME` / `mktextfm NAME`, which are
    // symlinks to mtx. Dispatching here avoids a shell wrapper, which
    // cost more than mtx itself (about 10 ms per call).
    let argv0 = std::env::args_os().next().unwrap_or_default();
    let prog = std::path::Path::new(&argv0).file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
    if prog == "mktextex" || prog == "mktextfm" {
        return hook(&prog);
    }
    if prog == "mktexfmt" {
        return mktexfmt();
    }
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mtx: error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let root = Root::discover(cli.root.as_deref())?;
    match cli.cmd {
        Cmd::Bootstrap { repository } => {
            eprintln!("mtx: bootstrapping {}", root.dir.display());
            let r = bootstrap::bootstrap(&root, repository.as_deref())?;
            eprintln!(
                "mtx: installed {} packages ({} files, {:.1} MiB downloaded)",
                r.installed.len(),
                r.files,
                r.bytes_downloaded as f64 / 1048576.0
            );
            eprintln!("mtx: add {} to your PATH", root.bin_dir().display());
        }
        Cmd::Ensure { format, package, path, siblings, font_name, name } => {
            let found = match (package, path, format) {
                _ if font_name => ensure::ensure_font_name(&root, &name, siblings)?,
                (Some(pkg), Some(rel), _) => ensure::ensure_path(&root, &pkg, &rel, siblings)?,
                (_, _, Some(format)) => {
                    let Some(kind) = ensure::kind(&format) else { return Ok(ExitCode::from(1)) };
                    ensure::ensure(&root, kind, &name)?.map(|p| vec![p])
                }
                _ => None,
            };
            return Ok(match found {
                Some(paths) => {
                    let mut out = std::io::stdout().lock();
                    for p in paths {
                        use std::io::Write;
                        writeln!(out, "{}", p.display())?;
                    }
                    ExitCode::SUCCESS
                }
                None => ExitCode::from(1),
            });
        }
        Cmd::InstallBinaries { source, sums } => {
            let mut ctx = open(&root)?;
            let n = if source.is_dir() {
                binaries::install_binaries(&mut ctx, &source)?
            } else {
                binaries::install_binaries_archive(&mut ctx, &source, sums.as_deref())?
            };
            eprintln!("mtx: installed {n} binaries; on-demand installation now uses the kpathsea patch");
        }
        Cmd::Install { packages } => {
            let mut ctx = open(&root)?;
            ctx.refresh(false)?;
            let names: Vec<&str> = packages.iter().map(String::as_str).collect();
            let r = install::install(&mut ctx, &names, Reason::Explicit)?;
            if r.installed.is_empty() {
                eprintln!("mtx: already installed and up to date");
            }
        }
        Cmd::Remove { packages, force } => {
            let mut ctx = open(&root)?;
            let names: Vec<&str> = packages.iter().map(String::as_str).collect();
            let removed = install::remove(&mut ctx, &names, force)?;
            if removed.is_empty() {
                eprintln!("mtx: none of these packages are installed");
            }
        }
        Cmd::Which { name, format } => {
            let kind = ensure::kind(&format).with_context(|| format!("unknown format `{format}`"))?;
            let idx = Index::open(&root.index_path())?;
            let Some(hit) = ensure::resolve(&idx, kind, &name) else {
                eprintln!("mtx: no package provides {name}");
                return Ok(ExitCode::from(1));
            };
            let installed = root.dir.join(hit.path()).exists();
            println!("{}\t{}\t{}", idx.package(hit.pkg).name, hit.path(), if installed { "installed" } else { "available" });
        }
        Cmd::Info { package } => {
            let ctx = open(&root)?;
            let tlpdb = ctx.tlpdb()?;
            let p = tlpdb.get(&package).with_context(|| format!("unknown package `{package}`"))?;
            let installed = ctx.db.installed()?;
            println!("name:      {}", p.name);
            println!("summary:   {}", p.shortdesc);
            println!("revision:  {}", p.revision);
            println!("category:  {}", p.category);
            println!("size:      {} bytes (runtime archive)", p.container_size);
            println!("files:     {}", p.runfiles.len());
            if !p.depends.is_empty() {
                println!("depends:   {}", p.depends.join(", "));
            }
            match installed.get(&package) {
                Some(i) => println!("installed: r{} ({})", i.revision, i.reason),
                None => println!("installed: no"),
            }
        }
        Cmd::List => {
            let ctx = open(&root)?;
            for (name, i) in ctx.db.installed()? {
                println!("{name}\tr{}\t{}", i.revision, i.reason);
            }
        }
        Cmd::Refresh => {
            let mut ctx = open(&root)?;
            match ctx.refresh(true)? {
                Freshness::Updated { from, to } => {
                    eprintln!("mtx: package database updated {} → r{to}", from.map_or("none".into(), |f| format!("r{f}")))
                }
                _ => eprintln!("mtx: package database is current"),
            }
        }
        Cmd::Update { dry_run } => {
            let mut ctx = open(&root)?;
            ctx.refresh(true)?;
            let tlpdb = ctx.tlpdb()?;
            let outdated = install::outdated(&ctx, &tlpdb)?;
            if outdated.is_empty() {
                eprintln!("mtx: all installed packages are up to date");
                return Ok(ExitCode::SUCCESS);
            }
            for (name, from, to) in &outdated {
                eprintln!("mtx: {name}: r{from} → r{to}");
            }
            if !dry_run {
                let names: Vec<&str> = outdated.iter().map(|(n, _, _)| n.as_str()).collect();
                let r = install::install(&mut ctx, &names, Reason::Upgrade)?;
                eprintln!("mtx: upgraded {} package(s)", r.installed.len());
            }
        }
        Cmd::Repair => {
            let mut ctx = open(&root)?;
            let fixed = install::repair(&mut ctx)?;
            if fixed.is_empty() {
                eprintln!("mtx: regenerated configuration, ls-R and shims; no packages needed fixing");
            } else {
                eprintln!("mtx: installed {}; regenerated configuration, ls-R and shims", fixed.join(", "));
            }
        }
        Cmd::Prefetch { file } => {
            let mut ctx = open(&root)?;
            ctx.refresh(false)?;
            let r = mtx_core::prefetch::prefetch(&mut ctx, &file)?;
            eprintln!(
                "mtx: prefetched {} package(s), {:.1} MiB downloaded",
                r.installed.len(),
                r.bytes_downloaded as f64 / 1048576.0
            );
        }
        Cmd::Doctor => {
            let ctx = open(&root)?;
            let path = std::env::var("PATH").unwrap_or_default();
            let findings = mtx_core::doctor::check(&ctx, &path)?;
            let worst = findings.iter().map(|f| f.severity).max();
            for f in &findings {
                let tag = match f.severity {
                    mtx_core::doctor::Severity::Ok => "ok",
                    mtx_core::doctor::Severity::Warning => "warning",
                    mtx_core::doctor::Severity::Problem => "PROBLEM",
                };
                println!("{tag:>8}  {}", f.message);
            }
            if worst == Some(mtx_core::doctor::Severity::Problem) {
                return Ok(ExitCode::from(1));
            }
        }
        Cmd::Regen => {
            let ctx = open(&root)?;
            let tlpdb = ctx.tlpdb()?;
            install::apply_regen(&ctx, &tlpdb, Regen::all(), &[])?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `mktexfmt NAME.fmt`: build the format (serialized, atomic) and print
/// its path; other requests (`.base`, `.mem`, options) go to TeX Live's.
fn mktexfmt() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = match Root::discover(None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("mtx: mktexfmt: {e:#}");
            return ExitCode::from(1);
        }
    };
    match args.as_slice() {
        [name] if !name.starts_with('-') && (name.ends_with(".fmt") || !name.contains('.')) => {
            match mtx_core::formats::mkfmt(&root, name) {
                Ok(path) => {
                    println!("{}", path.display());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("mtx: mktexfmt {name}: {e:#}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            use std::os::unix::process::CommandExt;
            let err = std::process::Command::new(root.texmf_dist().join("scripts/texlive/fmtutil.pl"))
                .arg0("mktexfmt")
                .args(&args)
                .exec();
            eprintln!("mtx: cannot run TeX Live's mktexfmt: {err}");
            ExitCode::from(1)
        }
    }
}

/// kpathsea hook mode: `mktextex NAME` or `mktextfm NAME` (the name is the
/// last argument). Prints the path and exits 0, or exits 1. For TFMs that no
/// package provides, falls back to TeX Live's METAFONT-based mktextfm.
fn hook(prog: &str) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.last() else { return ExitCode::from(1) };
    let (format, fallback) = if prog == "mktextex" { ("tex", None) } else { ("tfm", Some("mktextfm")) };
    let result = Root::discover(None).and_then(|root| {
        let kind = ensure::kind(format).expect("known format");
        Ok((ensure::ensure(&root, kind, name)?, root))
    });
    match result {
        Ok((Some(path), _)) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Ok((None, root)) => match fallback {
            Some(script) => {
                use std::os::unix::process::CommandExt;
                let err = std::process::Command::new(root.texmf_dist().join("scripts/texlive").join(script)).args(&args).exec();
                eprintln!("mtx: cannot run TeX Live's {script}: {err}");
                ExitCode::from(1)
            }
            None => ExitCode::from(1),
        },
        Err(e) => {
            eprintln!("mtx: {prog} {name}: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn open(root: &Root) -> Result<Ctx> {
    if !root.is_bootstrapped() {
        bail!("no MennoTeX installation at {} (run `mtx bootstrap`)", root.dir.display());
    }
    Ctx::open(root.clone())
}
