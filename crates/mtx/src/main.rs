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
use mtx_core::{bootstrap, ensure, install};

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
        /// kpathsea format name, e.g. `tex`, `tfm`, `type1 fonts`.
        #[arg(long)]
        format: String,
        name: String,
    },
    /// Install packages (and their dependencies).
    Install { packages: Vec<String> },
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
    /// Regenerate fmtutil.cnf, updmap.cfg, language.* and font maps.
    Regen,
}

fn main() -> ExitCode {
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
        Cmd::Ensure { format, name } => {
            let Some(kind) = ensure::kind(&format) else { return Ok(ExitCode::from(1)) };
            return Ok(match ensure::ensure(&root, kind, &name)? {
                Some(path) => {
                    println!("{}", path.display());
                    ExitCode::SUCCESS
                }
                None => ExitCode::from(1),
            });
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
        Cmd::Regen => {
            let ctx = open(&root)?;
            let tlpdb = ctx.tlpdb()?;
            install::apply_regen(&ctx, &tlpdb, Regen::all(), &[])?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn open(root: &Root) -> Result<Ctx> {
    if !root.is_bootstrapped() {
        bail!("no MennoTeX installation at {} (run `mtx bootstrap`)", root.dir.display());
    }
    Ctx::open(root.clone())
}
