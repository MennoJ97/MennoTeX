# Decisions

* [0009: Crash recovery of installs](0009-crash-recovery.md) - A package stays journaled until its whole install (ls-R, configuration, font maps, shims) is done; the next lookup of its files, the next install of anything, or mtx repair finishes it; tests/run_crash.sh kills mtx at each step.
* [0008: Licensing](0008-licensing.md) - Own code is MIT OR Apache-2.0; patches keep the license of the TeX Live file they change; binaries and packages keep theirs; README follows TeX Live's redistribution guidelines.
* [0007: Asking before automatic installs](0007-install-consent.md) - An autoinstall setting (yes, no, ask) decided in mtx; ask prompts on the terminal, else a dialog, else a fallback; one answer can cover a whole compile; failures and refusals go to mtx.log, shown by mtx log and mtx doctor.
* [0006: Release transitions](0006-release-transitions.md) - When tlnet moves to the next TeX Live release, an installation stays on its own release via the frozen historic repository; moving up is a new MennoTeX build plus `mtx bootstrap --from`.
* [0001: Build on TeX Live, copy MiKTeX's idea](0001-texlive-base.md) - TeX Live engines and tlnet with a new Rust package manager.
* [0005: On-demand fonts selected by name](0005-fonts-by-name.md) - Font-name index, XeTeX CoreText registration, prefetch for LuaLaTeX.
* [0004: Embedded font-map index](0004-font-map-index.md) - Installing TFMs also installs the packages whose maps cover them (ec → cm-super).
* [0003: Shape of the kpathsea patch](0003-kpathsea-patch-shape.md) - One included C file, called on every miss, hands installation to mtx.
* [0002: Phase 0 uses stock hooks](0002-phase0-stock-hooks.md) - Develop against unmodified tlnet binaries via MKTEXTEX/mktextfm first.
