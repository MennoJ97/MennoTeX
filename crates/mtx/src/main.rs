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
        /// Also install the packages another installation has, e.g. the
        /// previous TeX Live release's root after a release upgrade.
        #[arg(long, value_name = "ROOT")]
        from: Option<PathBuf>,
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
        #[arg(required_unless_present = "github")]
        source: Option<PathBuf>,
        /// SHA256SUMS file to verify the archive against.
        #[arg(long)]
        sums: Option<PathBuf>,
        /// Fetch the binaries from GitHub with the `gh` CLI: by default the
        /// artifact of the newest successful build workflow run.
        #[arg(long, conflicts_with_all = ["source", "sums"])]
        github: bool,
        /// With --github: this workflow run's artifact.
        #[arg(long, requires = "github")]
        run: Option<u64>,
        /// With --github: this release's assets (a tag, or `latest`).
        #[arg(long, requires = "github", conflicts_with = "run")]
        release: Option<String>,
        /// With --github: install even if this build is installed already.
        #[arg(long, requires = "github")]
        force: bool,
    },
    /// Install packages (and their dependencies).
    Install {
        packages: Vec<String>,
        /// Install automatically on behalf of PROGRAM (command shims): the
        /// `autoinstall` setting applies, and the packages count as auto.
        #[arg(long, value_name = "PROGRAM")]
        r#for: Option<String>,
    },
    /// Show or change settings: `mtx config`, `mtx config KEY`,
    /// `mtx config KEY VALUE`, `mtx config --unset KEY`.
    Config {
        key: Option<String>,
        value: Option<String>,
        /// Remove the setting (back to its default).
        #[arg(long, conflicts_with = "value", requires = "key")]
        unset: bool,
    },
    /// Show recent activity from tlpkg/mtx/mtx.log: installs, and why an
    /// install failed or was declined (TeX's own log never shows this).
    Log {
        /// Number of entries.
        #[arg(short = 'n', long, default_value_t = 20)]
        lines: usize,
        /// Only failed and declined installs.
        #[arg(long)]
        problems: bool,
        /// Only entries from this Unix time on.
        #[arg(long, value_name = "SECS")]
        since: Option<u64>,
    },
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
    /// installs, hooks and overlay, generated files, ls-R and shims.
    Repair,
    /// Install what a .tex file statically needs, in one go.
    Prefetch {
        file: PathBuf,
        /// Run automatically (latexmk's MennoTeX rc): honours `autoinstall`
        /// and `auto_prefetch`, says nothing when there is nothing to do,
        /// and never fails the build.
        #[arg(long)]
        auto: bool,
    },
    /// Install the documentation of packages (`texdoc NAME` does this by
    /// itself for what it is asked about).
    Docs { packages: Vec<String> },
    /// Remove packages installed on demand that no document has used for
    /// DAYS days (by file access time), with dependencies nothing else needs.
    /// They are installed again when needed.
    Gc {
        #[arg(long, default_value_t = 90)]
        days: u64,
        /// Only list what would be removed.
        #[arg(long)]
        dry_run: bool,
    },
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
    if prog == "texdoc" {
        return texdoc();
    }
    if prog == "latexmk" {
        return latexmk();
    }
    let cli = Cli::parse();
    let explicit_root = cli.root.clone();
    let command: Vec<String> = std::env::args().skip(1).collect();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mtx: error: {e:#}");
            // Also into mtx.log: TeX's log never shows our stderr.
            if let Ok(root) = Root::discover(explicit_root.as_deref()) {
                if root.mtx_dir().is_dir() {
                    mtx_core::ctx::append_log(&root, &format!("error: mtx {}: {e:#}", command.join(" ")));
                }
            }
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let root = Root::discover(cli.root.as_deref())?;
    match cli.cmd {
        Cmd::Bootstrap { repository, from } => {
            eprintln!("mtx: bootstrapping {}", root.dir.display());
            let from = from.map(|p| Root::discover(Some(&p))).transpose()?;
            let r = bootstrap::bootstrap(&root, repository.as_deref(), from.as_ref())?;
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
        Cmd::InstallBinaries { source, sums, github, run, release, force } => {
            use mtx_core::github::Source;
            let mut ctx = open(&root)?;
            let n = match source {
                _ if github => {
                    let src = match (run, release) {
                        (Some(id), _) => Source::Run(id),
                        (_, Some(tag)) => Source::Release(tag),
                        _ => Source::LatestRun,
                    };
                    match binaries::install_binaries_github(&mut ctx, &src, force)? {
                        Some(n) => n,
                        None => return Ok(ExitCode::SUCCESS),
                    }
                }
                Some(dir) if dir.is_dir() => binaries::install_binaries(&mut ctx, &dir)?,
                Some(archive) => binaries::install_binaries_archive(&mut ctx, &archive, sums.as_deref())?,
                None => unreachable!("clap requires a source without --github"),
            };
            eprintln!("mtx: installed {n} binaries; on-demand installation now uses the kpathsea patch");
        }
        Cmd::Install { packages, r#for } => {
            let mut ctx = open(&root)?;
            ctx.refresh(false)?;
            let names: Vec<&str> = packages.iter().map(String::as_str).collect();
            let reason = if r#for.is_some() { Reason::Auto } else { Reason::Explicit };
            ctx.ask_for = r#for;
            let r = match install::install(&mut ctx, &names, reason) {
                Err(e) if e.downcast_ref::<mtx_core::consent::Declined>().is_some() => return Ok(ExitCode::from(1)),
                other => other?,
            };
            if r.installed.is_empty() {
                eprintln!("mtx: already installed and up to date");
            }
        }
        Cmd::Config { key, value, unset } => {
            let mut ctx = open(&root)?;
            match (key, value) {
                (None, _) => {
                    for (k, values, help) in mtx_core::config::KEYS {
                        let v = ctx.db.get(k)?.unwrap_or_else(|| "(default)".into());
                        println!("{k} = {v}\n    {values}: {help}");
                    }
                    if let Ok(env) = std::env::var("MTX_AUTOINSTALL") {
                        println!("$MTX_AUTOINSTALL = {env} (overrides autoinstall)");
                    }
                }
                (Some(k), None) if unset => {
                    mtx_core::config::unset(&mut ctx, &k)?;
                    eprintln!("mtx: {k} reset to its default");
                }
                (Some(k), None) => {
                    if !mtx_core::config::KEYS.iter().any(|(name, _, _)| *name == k) {
                        bail!("unknown setting `{k}`");
                    }
                    println!("{}", ctx.db.get(&k)?.unwrap_or_else(|| "(default)".into()));
                }
                (Some(k), Some(v)) => {
                    let v = mtx_core::config::set(&mut ctx, &k, &v)?;
                    eprintln!("mtx: {k} = {v}");
                }
            }
        }
        Cmd::Log { lines, problems, since } => {
            let now = mtx_core::db::now_secs();
            let entries = mtx_core::logview::tail(&root);
            let shown: Vec<_> =
                entries.iter().filter(|e| (!problems || e.is_problem()) && e.at >= since.unwrap_or(0)).collect();
            if since.is_some() && !shown.is_empty() {
                eprintln!("mtx: why files may be missing (see `mtx log`):");
            }
            for e in &shown[shown.len().saturating_sub(lines)..] {
                println!("{:>12}  [{}] {}", mtx_core::logview::age(now, e.at), e.pid, e.msg);
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
                let (docs, pkgs): (Vec<&str>, Vec<&str>) =
                    outdated.iter().map(|(n, _, _)| n.as_str()).partition(|n| mtx_core::docs::base(n).is_some());
                let r = install::install(&mut ctx, &pkgs, Reason::Upgrade)?;
                let bases: Vec<&str> = docs.iter().filter_map(|d| mtx_core::docs::base(d)).collect();
                let d = mtx_core::docs::install(&mut ctx, &bases, Reason::Upgrade)?;
                eprintln!("mtx: upgraded {} package(s) and the documentation of {}", r.installed.len(), d.len());
            }
        }
        Cmd::Docs { packages } => {
            let mut ctx = open(&root)?;
            ctx.refresh(false)?;
            let names: Vec<&str> = packages.iter().map(String::as_str).collect();
            if mtx_core::docs::install(&mut ctx, &names, Reason::Explicit)?.is_empty() {
                eprintln!("mtx: documentation already installed and up to date");
            }
        }
        Cmd::Repair => {
            let mut ctx = open(&root)?;
            let fixed = install::repair(&mut ctx)?;
            if fixed.is_empty() {
                eprintln!("mtx: regenerated hooks, configuration, ls-R and shims; no packages needed fixing");
            } else {
                eprintln!("mtx: installed {}; regenerated hooks, configuration, ls-R and shims", fixed.join(", "));
            }
        }
        Cmd::Prefetch { file, auto } if auto => {
            let mut ctx = open(&root)?;
            let off = ctx.db.get("auto_prefetch")?.is_some_and(|v| v == "no");
            if off || mtx_core::consent::policy(&ctx)? == mtx_core::consent::Policy::No || ctx.offline()? {
                return Ok(ExitCode::SUCCESS);
            }
            let name = file.file_name().map_or_else(|| file.display().to_string(), |n| n.to_string_lossy().into_owned());
            ctx.ask_for = Some(name);
            let result = ctx.refresh(false).and_then(|_| mtx_core::prefetch::prefetch(&mut ctx, &file));
            match result {
                Ok(r) if !r.installed.is_empty() => eprintln!(
                    "mtx: prefetched {} package(s), {:.1} MiB downloaded",
                    r.installed.len(),
                    r.bytes_downloaded as f64 / 1048576.0
                ),
                Ok(_) => {}
                Err(e) if e.downcast_ref::<mtx_core::consent::Declined>().is_some() => {}
                Err(e) => ctx.log(format!("error: prefetch {}: {e:#}", file.display())),
            }
        }
        Cmd::Prefetch { file, .. } => {
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
        Cmd::Gc { days, dry_run } => {
            let mut ctx = open(&root)?;
            let tlpdb = ctx.tlpdb()?;
            let installed = ctx.db.installed()?;
            let now = mtx_core::db::now_secs();
            let mut used = std::collections::HashMap::new();
            for (name, i) in &installed {
                used.insert(name.clone(), mtx_core::gc::last_used(&ctx, name, i.installed_at)?);
            }
            let cutoff = now.saturating_sub(days * 86400);
            let plan = mtx_core::gc::plan(&tlpdb, &installed, &|n| used.get(n).copied().unwrap_or(now), cutoff)?;
            if plan.is_empty() {
                eprintln!("mtx: nothing to remove (no package installed on demand is unused for {days} days)");
                return Ok(ExitCode::SUCCESS);
            }
            let size: u64 = plan.iter().filter_map(|n| tlpdb.get(n)).map(|p| p.container_size).sum();
            for n in &plan {
                println!("{n}\tlast used {}", mtx_core::logview::age(now, used[n]));
            }
            if dry_run {
                eprintln!("mtx: would remove {} package(s), about {:.1} MiB packed", plan.len(), size as f64 / 1048576.0);
            } else {
                let names: Vec<&str> = plan.iter().map(String::as_str).collect();
                let removed = install::remove(&mut ctx, &names, false)?;
                eprintln!("mtx: removed {} package(s), about {:.1} MiB packed", removed.len(), size as f64 / 1048576.0);
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
            if let Ok(root) = Root::discover(None) {
                mtx_core::ctx::append_log(&root, &format!("error: {prog} {name}: {e:#}"));
            }
            ExitCode::from(1)
        }
    }
}

/// `latexmk ARGS`: TeX Live's latexmk with MennoTeX's system rc
/// (`$LATEXMKRCSYS`, unless set already), installing latexmk first if needed.
fn latexmk() -> ExitCode {
    use std::os::unix::process::CommandExt;
    let root = match Root::discover(None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("mtx: latexmk: {e:#}");
            return ExitCode::from(2);
        }
    };
    let script = root.texmf_dist().join("scripts/latexmk/latexmk.pl");
    if !script.exists() {
        let installed = (|| -> Result<()> {
            let mut ctx = open(&root)?;
            ctx.ask_for = Some("latexmk".into());
            ctx.refresh(false)?;
            install::install(&mut ctx, &["latexmk"], Reason::Auto)?;
            Ok(())
        })();
        if let Err(e) = installed {
            eprintln!("mtx: cannot install latexmk: {e:#}");
            return ExitCode::from(127);
        }
    }
    let mut cmd = std::process::Command::new("/usr/bin/perl");
    cmd.arg(&script).args(std::env::args_os().skip(1));
    cmd.env("PATH", format!("{}:{}", root.bin_dir().display(), std::env::var("PATH").unwrap_or_default()));
    if std::env::var_os("LATEXMKRCSYS").is_none() {
        cmd.env("LATEXMKRCSYS", root.texmf_overlay().join("latexmk/LatexMk"));
    }
    let err = cmd.exec();
    eprintln!("mtx: cannot run latexmk ({}): {err}", script.display());
    ExitCode::from(2)
}

/// `texdoc ARGS`: install the documentation the arguments name, then run
/// TeX Live's texdoc (installing the texdoc package first if needed).
fn texdoc() -> ExitCode {
    use std::os::unix::process::CommandExt;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = match Root::discover(None) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("mtx: texdoc: {e:#}");
            return ExitCode::from(2);
        }
    };
    // Names are the arguments that are not options (`-c NAME=VALUE` and
    // `-d` take a value in the next argument).
    let mut names = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-c" || a == "-d" {
            it.next();
        } else if !a.starts_with('-') {
            names.push(a.clone());
        }
    }
    let script = root.texmf_dist().join("scripts/texdoc/texdoc.tlu");
    let prepared = (|| -> Result<()> {
        let mut ctx = open(&root)?;
        if mtx_core::consent::policy(&ctx)? == mtx_core::consent::Policy::No {
            return Ok(());
        }
        if let Err(e) = ctx.refresh(false) {
            ctx.log(format!("error: texdoc: {e:#}"));
        }
        let tlpdb = ctx.tlpdb()?;
        if !script.exists() {
            install::install(&mut ctx, &["texdoc"], Reason::Auto)?;
        }
        for name in &names {
            let pkgs = mtx_core::docs::packages_for(&tlpdb, name);
            if pkgs.len() > 5 {
                ctx.log(format!("texdoc {name}: {} packages have documentation by that name; not installing them all", pkgs.len()));
                continue;
            }
            let pkgs: Vec<&str> = pkgs.iter().map(String::as_str).collect();
            if let Err(e) = mtx_core::docs::install(&mut ctx, &pkgs, Reason::Auto) {
                ctx.log(format!("error: documentation for {name}: {e:#}"));
            }
        }
        Ok(())
    })();
    if let Err(e) = prepared {
        eprintln!("mtx: texdoc: {e:#}");
    }
    let path = format!("{}:{}", root.bin_dir().display(), std::env::var("PATH").unwrap_or_default());
    let err = std::process::Command::new(root.bin_dir().join("texlua")).arg(&script).args(&args).env("PATH", path).exec();
    eprintln!("mtx: cannot run texdoc ({}): {err}", script.display());
    ExitCode::from(2)
}

fn open(root: &Root) -> Result<Ctx> {
    if !root.is_bootstrapped() {
        bail!("no MennoTeX installation at {} (run `mtx bootstrap`)", root.dir.display());
    }
    Ctx::open(root.clone())
}
