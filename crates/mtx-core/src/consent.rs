//! Whether an automatic install may go ahead (PLAN.md §5.9).
//!
//! The policy comes from `$MTX_AUTOINSTALL`, else the `autoinstall` setting
//! (`mtx config autoinstall yes|no|ask`), else `yes`. kpathsea's resolver
//! stops calling mtx only for values starting with `0`, `n` or `f`, so
//! `ask` is decided here, not in C.
//!
//! `ask` prompts on `/dev/tty` when there is a terminal, otherwise shows an
//! `osascript` dialog that gives up after 30 s, otherwise uses the
//! `ask_fallback` setting (default `yes`: MiKTeX's headless "ask" silently
//! means no, which confuses people). One first compile can need dozens of
//! packages, so an answer can cover the rest of the run: it is remembered
//! for the latexmk build the request comes from (one build runs several TeX
//! passes, biber and mtx's prefetch, each a different parent of mtx), else
//! for the parent process, which for kpathsea's calls is the TeX engine.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::process::Command;

use anyhow::Result;

use crate::ctx::Ctx;
use crate::db::now_secs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Yes,
    No,
    Ask,
}

impl Policy {
    /// Parse a setting. Anything not recognised means yes, as in kpathsea's
    /// resolver, which only checks the first character for `0`/`n`/`f`.
    pub fn parse(v: &str) -> Policy {
        let v = v.trim().to_ascii_lowercase();
        if v == "ask" {
            Policy::Ask
        } else if v.starts_with(['0', 'n', 'f']) {
            Policy::No
        } else {
            Policy::Yes
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Policy::Yes => "yes",
            Policy::No => "no",
            Policy::Ask => "ask",
        }
    }
}

pub fn policy(ctx: &Ctx) -> Result<Policy> {
    if let Ok(v) = std::env::var("MTX_AUTOINSTALL") {
        if !v.is_empty() {
            return Ok(Policy::parse(&v));
        }
    }
    Ok(ctx.db.get("autoinstall")?.map_or(Policy::Yes, |v| Policy::parse(&v)))
}

/// An automatic install was refused by the policy or the user.
#[derive(Debug, thiserror::Error)]
#[error("not installing {packages} for {trigger}: {why}")]
pub struct Declined {
    pub trigger: String,
    pub packages: String,
    pub why: String,
}

/// The user's answer to one prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Yes,
    /// Yes, and for the rest of this run.
    All,
    No,
    /// No, and for the rest of this run.
    None,
}

/// What is about to be installed, for the prompt.
pub struct Request<'a> {
    /// The file or program that needs it (`tcolorbox.sty`, `latexmk`).
    pub trigger: &'a str,
    /// Packages in the install plan, dependencies included.
    pub packages: &'a [String],
    pub bytes: u64,
}

impl Request<'_> {
    pub fn describe(&self) -> String {
        let first = self.packages.first().map(String::as_str).unwrap_or("?");
        let more = match self.packages.len() {
            0 | 1 => String::new(),
            n => format!(" and {} more", n - 1),
        };
        format!("{} needs package {first}{more} ({:.1} MiB)", self.trigger, self.bytes as f64 / 1048576.0)
    }
}

fn run_key() -> String {
    let parent = std::os::unix::process::parent_id();
    let processes = Command::new("/bin/ps")
        .args(["-A", "-o", "pid=,ppid=,command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    format!("ask_run:{}", latexmk_ancestor(&processes, parent).unwrap_or(parent))
}

/// The nearest process, starting at `pid` and going up, that is latexmk
/// (`latexmk …`, or `perl …/latexmk.pl …` as MennoTeX's `latexmk` runs it),
/// given `ps -o pid=,ppid=,command=` output.
fn latexmk_ancestor(ps: &str, mut pid: u32) -> Option<u32> {
    let procs: HashMap<u32, (u32, &str)> = ps
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let pid = w.next()?.parse().ok()?;
            let ppid = w.next()?.parse().ok()?;
            let cmd_start = l.find(w.next()?)?;
            Some((pid, (ppid, &l[cmd_start..])))
        })
        .collect();
    for _ in 0..32 {
        let (ppid, cmd) = procs.get(&pid)?;
        let is_latexmk =
            cmd.split_whitespace().take(2).any(|w| matches!(w.rsplit('/').next(), Some("latexmk" | "latexmk.pl")));
        if is_latexmk {
            return Some(pid);
        }
        if *ppid <= 1 {
            return None;
        }
        pid = *ppid;
    }
    None
}

/// How long an "all"/"none" answer for a parent process is trusted: process
/// ids are reused, and a compile does not take this long.
const RUN_ANSWER_SECS: u64 = 3600;

/// Decide whether the install in `req` may go ahead, asking through
/// `prompt` under the `ask` policy. `prompt` returns `None` when it could
/// not ask (no terminal, no dialog, timeout); the fallback applies then.
pub fn decide(ctx: &Ctx, req: &Request, prompt: &mut dyn FnMut(&Ctx, &str) -> Option<Answer>) -> Result<bool> {
    let decline = |why: &str| -> Result<bool> {
        ctx.log(format!("declined: {} ({why})", req.describe()));
        Ok(false)
    };
    match policy(ctx)? {
        Policy::Yes => return Ok(true),
        Policy::No => return decline("autoinstall is off"),
        Policy::Ask => {}
    }
    let key = run_key();
    if let Some(v) = ctx.db.get(&key)? {
        if let Some((answer, at)) = v.split_once(' ') {
            if now_secs().saturating_sub(at.parse().unwrap_or(0)) < RUN_ANSWER_SECS {
                return if answer == "all" { Ok(true) } else { decline("answered none for this run") };
            }
        }
    }
    let answer = match prompt(ctx, &req.describe()) {
        Some(a) => a,
        None => {
            if ctx.db.get("ask_fallback")?.is_some_and(|v| Policy::parse(&v) == Policy::No) {
                return decline("no terminal or dialog to ask; ask_fallback is no");
            }
            ctx.log(format!("no terminal or dialog to ask about {}; ask_fallback is yes", req.describe()));
            Answer::Yes
        }
    };
    if let Answer::All | Answer::None = answer {
        // Forget answers for runs that are long over.
        for (k, v) in ctx.db.with_prefix("ask_run:")? {
            let at: u64 = v.split_once(' ').and_then(|(_, t)| t.parse().ok()).unwrap_or(0);
            if now_secs().saturating_sub(at) >= RUN_ANSWER_SECS {
                ctx.db.unset(&k)?;
            }
        }
        let word = if answer == Answer::All { "all" } else { "none" };
        ctx.db.set(&key, &format!("{word} {}", now_secs()))?;
    }
    match answer {
        Answer::Yes | Answer::All => Ok(true),
        Answer::No => decline("answered no"),
        Answer::None => decline("answered none for this run"),
    }
}

/// Ask the user: terminal first, then a dialog (unless `ask_dialog` is
/// off or this is an SSH session), else `None`.
pub fn ask_user(ctx: &Ctx, question: &str) -> Option<Answer> {
    if let Some(a) = ask_tty(question) {
        return Some(a);
    }
    let dialog_off = ctx.db.get("ask_dialog").ok().flatten().is_some_and(|v| Policy::parse(&v) == Policy::No);
    if dialog_off || std::env::var_os("SSH_CONNECTION").is_some() {
        return None;
    }
    ask_dialog(question)
}

fn ask_tty(question: &str) -> Option<Answer> {
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty").ok()?;
    let mut out = tty.try_clone().ok()?;
    let mut input = BufReader::new(tty);
    for _ in 0..3 {
        write!(out, "mtx: {question}. Install? [Y]es, [a]ll for this run, [n]o, n[o]ne for this run: ").ok()?;
        out.flush().ok()?;
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Some(Answer::Yes),
            "a" | "all" => return Some(Answer::All),
            "n" | "no" => return Some(Answer::No),
            "o" | "none" => return Some(Answer::None),
            _ => {}
        }
    }
    None
}

/// The AppleScript for the dialog; the question is passed as an argument,
/// so it needs no quoting.
const DIALOG: &[&str] = &[
    "on run argv",
    "display dialog (item 1 of argv) with title \"MennoTeX\" buttons {\"Don't Install\", \"Install All\", \"Install\"} \
     default button \"Install\" cancel button \"Don't Install\" giving up after 30",
    "if gave up of result then return \"timeout\"",
    "return button returned of result",
    "end run",
];

fn ask_dialog(question: &str) -> Option<Answer> {
    let mut cmd = Command::new("/usr/bin/osascript");
    for line in DIALOG {
        cmd.args(["-e", line]);
    }
    let out = cmd.arg(format!("{question}.\n\n\"Install All\" also installs what the rest of this run needs.")).output().ok()?;
    parse_dialog(out.status.success(), &String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr))
}

/// `osascript` exits with error -128 when the cancel button is pressed.
fn parse_dialog(ok: bool, stdout: &str, stderr: &str) -> Option<Answer> {
    if !ok {
        return stderr.contains("(-128)").then_some(Answer::No);
    }
    match stdout.trim() {
        "Install" => Some(Answer::Yes),
        "Install All" => Some(Answer::All),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_values_match_the_resolver() {
        assert_eq!(Policy::parse("ask"), Policy::Ask);
        for no in ["0", "no", "false", "never", "N"] {
            assert_eq!(Policy::parse(no), Policy::No, "{no}");
        }
        for yes in ["1", "yes", "true", "always", ""] {
            assert_eq!(Policy::parse(yes), Policy::Yes, "{yes}");
        }
    }

    #[test]
    fn answers_cover_the_whole_latexmk_build() {
        let ps = "    1     0 /sbin/launchd\n\
                  500     1 /Applications/Visual Studio Code.app/Contents/MacOS/Code\n\
                  600   500 /usr/bin/perl /r/texmf-dist/scripts/latexmk/latexmk.pl -pdf main\n\
                  610   600 /r/bin/universal-darwin/mtx prefetch --auto main.tex\n\
                  620   600 sh -c pdflatex -recorder main.tex\n\
                  621   620 pdflatex -recorder main.tex\n\
                  700     1 pdflatex plain.tex\n";
        // Prefetch and every TeX pass of the build share latexmk's id.
        assert_eq!(latexmk_ancestor(ps, 600), Some(600));
        assert_eq!(latexmk_ancestor(ps, 621), Some(600));
        // TeX run on its own: no latexmk above it.
        assert_eq!(latexmk_ancestor(ps, 700), None);
        assert_eq!(latexmk_ancestor(ps, 999), None);
    }

    #[test]
    fn dialog_results() {
        assert_eq!(parse_dialog(true, "Install\n", ""), Some(Answer::Yes));
        assert_eq!(parse_dialog(true, "Install All\n", ""), Some(Answer::All));
        assert_eq!(parse_dialog(true, "timeout\n", ""), None);
        assert_eq!(parse_dialog(false, "", "execution error: User canceled. (-128)"), Some(Answer::No));
        assert_eq!(parse_dialog(false, "", "execution error: No user interaction allowed. (-1713)"), None);
    }
}
