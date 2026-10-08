//! `mtx config`: the user-facing settings stored in the installed database.

use anyhow::{Result, bail};

use crate::consent::Policy;
use crate::ctx::Ctx;

/// (key, values, meaning)
pub const KEYS: &[(&str, &str, &str)] = &[
    ("autoinstall", "yes|no|ask", "install missing packages automatically (default yes; $MTX_AUTOINSTALL overrides)"),
    ("ask_fallback", "yes|no", "answer for `ask` when there is no terminal and no dialog (default yes)"),
    ("ask_dialog", "yes|no", "under `ask`, show a dialog when there is no terminal (default yes)"),
    ("repository", "URL|path", "TeX Live repository (default mirror.ctan.org's tlnet)"),
    ("historic_mirrors", "URL ...", "mirrors of TeX Live's historic archive, tried in order"),
    ("auto_prefetch", "yes|no", "before each latexmk build, install what the document visibly needs in one batch (default yes)"),
    ("prefetch_depth", "document|requires", "what prefetch installs: the packages the document names, or also what those load in turn (default requires)"),
    ("freshness_ttl", "DURATION", "how long a check of the package database stays valid, e.g. 30m, 1h, 1d, 0 to always check (default 1h)"),
    ("docs", "never|on-texdoc|always", "documentation: never, when `texdoc` asks for it, or with every install outside a compile (default on-texdoc)"),
    ("binaries_repo", "OWNER/REPO", "GitHub repository for `mtx install-binaries --github` (default MennoJ97/MennoTeX)"),
];

/// Seconds in `30m`, `1h`, `2d`, `90s`, `90` or `0`.
pub fn parse_duration(value: &str) -> Option<u64> {
    let v = value.trim().to_ascii_lowercase();
    let (num, unit) = v.split_at(v.find(|c: char| !c.is_ascii_digit()).unwrap_or(v.len()));
    let n: u64 = num.parse().ok()?;
    let mult = match unit.trim() {
        "" | "s" => 1,
        "m" | "min" => 60,
        "h" => 3600,
        "d" => 86400,
        _ => return None,
    };
    n.checked_mul(mult)
}

/// `secs` in the largest unit that divides it exactly.
fn format_duration(secs: u64) -> String {
    match secs {
        0 => "0".into(),
        s if s % 86400 == 0 => format!("{}d", s / 86400),
        s if s % 3600 == 0 => format!("{}h", s / 3600),
        s if s % 60 == 0 => format!("{}m", s / 60),
        s => format!("{s}s"),
    }
}

/// The `docs` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Docs {
    Never,
    OnTexdoc,
    Always,
}

pub fn docs(ctx: &Ctx) -> Result<Docs> {
    Ok(match ctx.db.get("docs")?.as_deref() {
        Some("never") => Docs::Never,
        Some("always") => Docs::Always,
        _ => Docs::OnTexdoc,
    })
}

/// Whether `mtx prefetch` follows `\RequirePackage` into installed files.
pub fn prefetch_follows_requires(ctx: &Ctx) -> Result<bool> {
    Ok(ctx.db.get("prefetch_depth")?.as_deref() != Some("document"))
}

/// Check and normalize `value` for `key`.
pub fn normalize(key: &str, value: &str) -> Result<String> {
    let yes_no = |v: &str| -> Result<String> {
        match v.trim().to_ascii_lowercase().as_str() {
            "yes" | "y" | "1" | "true" | "on" => Ok("yes".into()),
            "no" | "n" | "0" | "false" | "off" => Ok("no".into()),
            _ => bail!("{key} takes yes or no, not `{v}`"),
        }
    };
    match key {
        "autoinstall" => match value.trim().to_ascii_lowercase().as_str() {
            "ask" => Ok(Policy::Ask.as_str().into()),
            v => yes_no(v),
        },
        "ask_fallback" | "ask_dialog" | "auto_prefetch" => yes_no(value),
        "repository" | "historic_mirrors" if !value.trim().is_empty() => Ok(value.trim().to_string()),
        "repository" | "historic_mirrors" => bail!("{key} cannot be empty; use --unset"),
        "prefetch_depth" => match value.trim().to_ascii_lowercase().as_str() {
            v @ ("document" | "requires") => Ok(v.into()),
            _ => bail!("prefetch_depth takes document or requires, not `{value}`"),
        },
        "freshness_ttl" => match parse_duration(value) {
            Some(secs) => Ok(format_duration(secs)),
            None => bail!("freshness_ttl takes a duration such as 30m, 1h or 1d, not `{value}`"),
        },
        "docs" => match value.trim().to_ascii_lowercase().as_str() {
            v @ ("never" | "on-texdoc" | "always") => Ok(v.into()),
            _ => bail!("docs takes never, on-texdoc or always, not `{value}`"),
        },
        "binaries_repo" => match value.trim().split_once('/') {
            Some((o, r)) if !o.is_empty() && !r.is_empty() && !r.contains('/') => Ok(value.trim().to_string()),
            _ => bail!("binaries_repo takes OWNER/REPO, not `{value}`"),
        },
        _ => bail!("unknown setting `{key}`; known: {}", KEYS.iter().map(|k| k.0).collect::<Vec<_>>().join(", ")),
    }
}

pub fn set(ctx: &mut Ctx, key: &str, value: &str) -> Result<String> {
    let value = normalize(key, value)?;
    ctx.db.set(key, &value)?;
    if key == "repository" || key == "historic_mirrors" {
        ctx.unpin_mirror()?;
    }
    Ok(value)
}

pub fn unset(ctx: &mut Ctx, key: &str) -> Result<()> {
    if !KEYS.iter().any(|k| k.0 == key) {
        bail!("unknown setting `{key}`");
    }
    ctx.db.unset(key)?;
    if key == "repository" || key == "historic_mirrors" {
        ctx.unpin_mirror()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_checked_and_normalized() {
        assert_eq!(normalize("autoinstall", "ASK").unwrap(), "ask");
        assert_eq!(normalize("autoinstall", "0").unwrap(), "no");
        assert_eq!(normalize("ask_dialog", "off").unwrap(), "no");
        assert!(normalize("autoinstall", "sometimes").is_err());
        assert!(normalize("colour", "blue").is_err());
        assert!(normalize("repository", " ").is_err());
        assert_eq!(normalize("freshness_ttl", "90min").unwrap(), "90m");
        assert_eq!(normalize("freshness_ttl", "7200").unwrap(), "2h");
        assert_eq!(normalize("freshness_ttl", "0").unwrap(), "0");
        assert!(normalize("freshness_ttl", "soon").is_err());
        assert!(normalize("freshness_ttl", "-1h").is_err());
        assert_eq!(parse_duration("1d"), Some(86400));
        assert_eq!(normalize("docs", "Always").unwrap(), "always");
        assert!(normalize("docs", "sometimes").is_err());
        assert_eq!(normalize("prefetch_depth", "DOCUMENT").unwrap(), "document");
        assert!(normalize("prefetch_depth", "none").is_err());
    }
}
