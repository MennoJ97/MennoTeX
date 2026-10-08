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

impl Answer {
    pub fn as_str(self) -> &'static str {
        match self {
            Answer::Yes => "yes",
            Answer::All => "all",
            Answer::No => "no",
            Answer::None => "none",
        }
    }
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

/// Running processes from `ps -o pid=,ppid=,command=`: pid → (ppid, command).
fn parse_ps(ps: &str) -> HashMap<u32, (u32, &str)> {
    ps.lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            let pid = w.next()?.parse().ok()?;
            let ppid = w.next()?.parse().ok()?;
            let cmd_start = l.find(w.next()?)?;
            Some((pid, (ppid, &l[cmd_start..])))
        })
        .collect()
}

/// Where a prompt comes from: the key its run's answers are stored under,
/// and the chain of processes above mtx, for the log.
fn run_context() -> (String, String) {
    let parent = std::os::unix::process::parent_id();
    let ps = Command::new("/bin/ps")
        .args(["-A", "-o", "pid=,ppid=,command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let procs = parse_ps(&ps);
    let key = format!("ask_run:{}", latexmk_ancestor(&procs, parent).unwrap_or(parent));
    (key, ancestry(&procs, parent))
}

/// The nearest process, starting at `pid` and going up, that is latexmk
/// (`latexmk …`, or `perl …/latexmk.pl …` as MennoTeX's `latexmk` runs it).
fn latexmk_ancestor(procs: &HashMap<u32, (u32, &str)>, mut pid: u32) -> Option<u32> {
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

/// Short names of `pid` and its ancestors below launchd, nearest first
/// (`pdflatex < latexmk.pl < zsh < Visual Studio Code`), so the log shows
/// what a build was started from.
fn ancestry(procs: &HashMap<u32, (u32, &str)>, start: u32) -> String {
    let mut pid = start;
    let mut names = Vec::new();
    let mut seen = 0;
    while let Some((ppid, cmd)) = procs.get(&pid) {
        seen += 1;
        if pid <= 1 || seen > 16 || names.len() == 8 {
            break;
        }
        let name = process_name(cmd);
        if names.last() != Some(&name) {
            names.push(name); // an app's helpers repeat its name
        }
        pid = *ppid;
    }
    if names.is_empty() {
        format!("process {}", start)
    } else {
        names.join(" < ")
    }
}

/// A process's name for [`ancestry`]: an app bundle's name, a script's name
/// for interpreters and `sh -c`, else the program's file name.
fn process_name(cmd: &str) -> String {
    if let Some(i) = cmd.find(".app/") {
        return cmd[..i].rsplit('/').next().unwrap_or("?").to_string();
    }
    let base = |w: &str| w.rsplit('/').next().unwrap_or(w).to_string();
    let mut words = cmd.split_whitespace();
    let program = base(words.next().unwrap_or("?"));
    let program = program.trim_start_matches('-'); // login shells: `-zsh`
    if matches!(program, "perl" | "sh" | "bash" | "zsh" | "env" | "python3" | "texlua") {
        if let Some(script) = words.find(|w| !w.starts_with('-')) {
            return base(script);
        }
    }
    program.to_string()
}

/// How long an "all"/"none" answer for a parent process is trusted: process
/// ids are reused, and a compile does not take this long.
const RUN_ANSWER_SECS: u64 = 3600;

/// The log line for an install the fallback allowed: `UNASKED`, the
/// request, ` (<why>; from <processes>)`, `UNASKED_END`.
const UNASKED: &str = "could not ask about ";
const UNASKED_END: &str = "; ask_fallback is yes";

/// For a log message about a prompt that could not be shown, why not and
/// where the request came from (`mtx doctor` reports the last one).
pub fn unasked_reason(msg: &str) -> Option<&str> {
    let rest = msg.strip_prefix(UNASKED)?.strip_suffix(UNASKED_END)?.strip_suffix(')')?;
    // The request ends with its size, `(… MiB)`.
    rest.split_once(" MiB) (").map(|(_, why)| why)
}

/// How a prompt went: the answer and how it was given (`terminal`,
/// `dialog`), or why nobody could be asked.
pub type Asked = std::result::Result<(Answer, &'static str), String>;

/// Decide whether the install in `req` may go ahead, asking through
/// `prompt` under the `ask` policy. When `prompt` could not ask (no
/// terminal, no dialog, timeout) the fallback applies. Every outcome is
/// logged with the processes above mtx, to tell where a build came from.
pub fn decide(ctx: &Ctx, req: &Request, prompt: &mut dyn FnMut(&Ctx, &str) -> Asked) -> Result<bool> {
    let decline = |why: &str| -> Result<bool> {
        ctx.log(format!("declined: {} ({why})", req.describe()));
        Ok(false)
    };
    match policy(ctx)? {
        Policy::Yes => return Ok(true),
        Policy::No => return decline("autoinstall is off"),
        Policy::Ask => {}
    }
    let (key, from) = run_context();
    if let Some(v) = ctx.db.get(&key)? {
        if let Some((answer, at)) = v.split_once(' ') {
            if now_secs().saturating_sub(at.parse().unwrap_or(0)) < RUN_ANSWER_SECS {
                return if answer == "all" { Ok(true) } else { decline("answered none for this run") };
            }
        }
    }
    let answer = match prompt(ctx, &req.describe()) {
        Ok((answer, via)) => {
            ctx.log(format!("asked about {} by {via}: {} (from {from})", req.describe(), answer.as_str()));
            answer
        }
        Err(why) => {
            if ctx.db.get("ask_fallback")?.is_some_and(|v| Policy::parse(&v) == Policy::No) {
                return decline(&format!("could not ask: {why}; from {from}; ask_fallback is no"));
            }
            ctx.log(format!("{UNASKED}{} ({why}; from {from}){UNASKED_END}", req.describe()));
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
/// off or this is an SSH session). The error says why neither worked.
pub fn ask_user(ctx: &Ctx, question: &str) -> Asked {
    let tty = match ask_tty(question) {
        Ok(a) => return Ok((a, "terminal")),
        Err(why) => why,
    };
    let dialog_off = ctx.db.get("ask_dialog").ok().flatten().is_some_and(|v| Policy::parse(&v) == Policy::No);
    if dialog_off {
        return Err(format!("{tty}; ask_dialog is no"));
    }
    if std::env::var_os("SSH_CONNECTION").is_some() {
        return Err(format!("{tty}; no dialog in an SSH session"));
    }
    match ask_dialog(question) {
        Ok(a) => Ok((a, "dialog")),
        Err(why) => Err(format!("{tty}; dialog: {why}")),
    }
}

fn ask_tty(question: &str) -> std::result::Result<Answer, String> {
    let tty = OpenOptions::new().read(true).write(true).open("/dev/tty").map_err(|_| "no terminal".to_string())?;
    let io_err = |e: std::io::Error| format!("terminal: {e}");
    let mut out = tty.try_clone().map_err(io_err)?;
    let mut input = BufReader::new(tty);
    for _ in 0..3 {
        write!(out, "mtx: {question}. Install? [Y]es, [a]ll for this run, [n]o, n[o]ne for this run: ").map_err(io_err)?;
        out.flush().map_err(io_err)?;
        let mut line = String::new();
        if input.read_line(&mut line).map_err(io_err)? == 0 {
            return Err("terminal closed".into());
        }
        match line.trim().to_ascii_lowercase().as_str() {
            "" | "y" | "yes" => return Ok(Answer::Yes),
            "a" | "all" => return Ok(Answer::All),
            "n" | "no" => return Ok(Answer::No),
            "o" | "none" => return Ok(Answer::None),
            _ => {}
        }
    }
    Err("no valid answer on the terminal".into())
}

/// The AppleScript for the dialog; the question is passed as an argument,
/// so it needs no quoting. The answer goes into a variable: AppleScript's
/// `result` is the value of the last statement, so after the `if` line it
/// no longer holds the dialog's record, and every click failed with "The
/// variable result is not defined. (-2753)" (seen 2026-10-08 in builds from
/// VS Code; mtx then fell back to `ask_fallback`).
const DIALOG: &[&str] = &[
    "on run argv",
    "set answer to display dialog (item 1 of argv) with title \"MennoTeX\" \
     buttons {\"Don't Install\", \"Install All\", \"Install\"} \
     default button \"Install\" cancel button \"Don't Install\" giving up after 30",
    "if gave up of answer then return \"timeout\"",
    "return button returned of answer",
    "end run",
];

fn ask_dialog(question: &str) -> std::result::Result<Answer, String> {
    let mut cmd = Command::new("/usr/bin/osascript");
    for line in DIALOG {
        cmd.args(["-e", line]);
    }
    let started = std::time::Instant::now();
    let out = cmd
        .arg(format!("{question}.\n\n\"Install All\" also installs what the rest of this run needs."))
        .output()
        .map_err(|e| format!("cannot run osascript: {e}"))?;
    let secs = started.elapsed().as_secs_f64();
    parse_dialog(out.status.code(), &String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr))
        .map_err(|why| format!("{why} after {secs:.1} s"))
}

/// Read `osascript`'s result: its exit code (`None` if killed by a
/// signal), stdout and stderr. It exits with error -128 when the cancel
/// button is pressed; any other failure is reported with its message.
fn parse_dialog(code: Option<i32>, stdout: &str, stderr: &str) -> std::result::Result<Answer, String> {
    if code != Some(0) {
        if stderr.contains("(-128)") {
            return Ok(Answer::No);
        }
        let mut msg = stderr.split_whitespace().collect::<Vec<_>>().join(" ");
        if msg.len() > 300 {
            let cut = (0..=300).rev().find(|&i| msg.is_char_boundary(i)).unwrap_or(0);
            msg.truncate(cut);
            msg.push('…');
        }
        let status = code.map_or("killed by a signal".to_string(), |c| format!("exit {c}"));
        return Err(if msg.is_empty() { format!("osascript {status}") } else { format!("osascript {status}: {msg}") });
    }
    match stdout.trim() {
        "Install" => Ok(Answer::Yes),
        "Install All" => Ok(Answer::All),
        "timeout" => Err("no answer within 30 s".into()),
        other => Err(format!("osascript returned {other:?}")),
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

    const PS: &str = "    1     0 /sbin/launchd\n\
                      500     1 /Applications/Visual Studio Code.app/Contents/MacOS/Code\n\
                      510   500 /Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin) --type=utility\n\
                      520   510 -zsh -l\n\
                      600   520 /usr/bin/perl /r/texmf-dist/scripts/latexmk/latexmk.pl -pdf main\n\
                      610   600 /r/bin/universal-darwin/mtx prefetch --auto main.tex\n\
                      620   600 sh -c pdflatex -recorder main.tex\n\
                      621   620 pdflatex -recorder main.tex\n\
                      700     1 pdflatex plain.tex\n";

    #[test]
    fn answers_cover_the_whole_latexmk_build() {
        let procs = parse_ps(PS);
        // Prefetch and every TeX pass of the build share latexmk's id.
        assert_eq!(latexmk_ancestor(&procs, 600), Some(600));
        assert_eq!(latexmk_ancestor(&procs, 621), Some(600));
        // TeX run on its own: no latexmk above it.
        assert_eq!(latexmk_ancestor(&procs, 700), None);
        assert_eq!(latexmk_ancestor(&procs, 999), None);
    }

    #[test]
    fn ancestry_names_where_a_build_came_from() {
        let procs = parse_ps(PS);
        assert_eq!(
            ancestry(&procs, 621),
            "pdflatex < latexmk.pl < zsh < Visual Studio Code"
        );
        assert_eq!(ancestry(&procs, 700), "pdflatex");
        assert_eq!(ancestry(&procs, 999), "process 999");
    }

    #[test]
    fn unasked_reasons_are_read_back_from_the_log() {
        let msg = "could not ask about x.sty needs package x (0.1 MiB) (no terminal; dialog: osascript exit 1: \
                   No user interaction allowed. (-1713) after 2.1 s; from pdflatex < latexmk.pl); ask_fallback is yes";
        assert_eq!(
            unasked_reason(msg),
            Some("no terminal; dialog: osascript exit 1: No user interaction allowed. (-1713) after 2.1 s; from pdflatex < latexmk.pl")
        );
        assert_eq!(unasked_reason("installing 1 package(s), 0.0 MiB: x"), None);
    }

    /// The script after the dialog, run by osascript with a record in place
    /// of `display dialog` (no window): a click and a timeout both come back.
    #[test]
    fn dialog_script_returns_the_answer() {
        let run = |record: &str| {
            let mut cmd = Command::new("/usr/bin/osascript");
            for (i, line) in DIALOG.iter().enumerate() {
                let line = if i == 1 { format!("set answer to {record}") } else { line.to_string() };
                cmd.args(["-e", &line]);
            }
            let out = cmd.arg("question").output().unwrap();
            parse_dialog(out.status.code(), &String::from_utf8_lossy(&out.stdout), &String::from_utf8_lossy(&out.stderr))
        };
        if !std::path::Path::new("/usr/bin/osascript").exists() {
            return;
        }
        assert!(DIALOG[1].starts_with("set answer to display dialog"));
        assert_eq!(run(r#"{button returned:"Install All", gave up:false}"#), Ok(Answer::All));
        assert_eq!(run(r#"{button returned:"Install", gave up:false}"#), Ok(Answer::Yes));
        assert_eq!(run(r#"{button returned:"", gave up:true}"#), Err("no answer within 30 s".into()));
    }

    #[test]
    fn dialog_results() {
        assert_eq!(parse_dialog(Some(0), "Install\n", ""), Ok(Answer::Yes));
        assert_eq!(parse_dialog(Some(0), "Install All\n", ""), Ok(Answer::All));
        assert_eq!(parse_dialog(Some(0), "timeout\n", ""), Err("no answer within 30 s".into()));
        assert_eq!(parse_dialog(Some(1), "", "execution error: User canceled. (-128)"), Ok(Answer::No));
        assert_eq!(
            parse_dialog(Some(1), "", "0:12: execution error:\n No user interaction allowed. (-1713)\n"),
            Err("osascript exit 1: 0:12: execution error: No user interaction allowed. (-1713)".into())
        );
        assert_eq!(parse_dialog(None, "", ""), Err("osascript killed by a signal".into()));
    }
}
