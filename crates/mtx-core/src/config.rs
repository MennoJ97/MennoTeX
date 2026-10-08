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
    ("binaries_repo", "OWNER/REPO", "GitHub repository for `mtx install-binaries --github` (default MennoJ97/MennoTeX)"),
];

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
    }
}
