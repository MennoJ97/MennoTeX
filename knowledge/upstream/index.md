# Upstream reference

* [kpathsea file lookup and mktex hooks](kpathsea.md) - Verified facts about how kpathsea finds files and when it runs mktex* scripts.
* [tlnet](tlnet.md) - Layout, signing chain, archive formats and mirror behaviour of the TeX Live network repository.
* [How tlmgr generates configuration files](tlmgr-config.md) - Exact rules for fmtutil.cnf, updmap.cfg and language.dat/.def/.dat.lua.
* [TeX Live scripts: mktexfmt, fmtutil, updmap, texdoc, latexmk](texlive-scripts.md) - Behaviour of the TeX Live scripts mtx relies on, including the TEXMFVAR constraint, how texdoc finds its database and documents, and latexmk's rc files and hooks.
* [Building texlive-source on macOS](texlive-build.md) - What it takes to build TeX Live natively on Apple Silicon, including the C23/libgd problem.
* [How fonts are found by name](fonts-by-name.md) - XeTeX/CoreText, luaotfload request order, kpathsea directory cache.
* [MiKTeX on-the-fly installation](miktex.md) - How MiKTeX installs packages on demand, what to copy, and what to avoid.
* [Licenses of TeX Live, its programs and MiKTeX](licenses.md) - Verified license terms of kpathsea, XeTeX, pdfTeX, LuaTeX, dvipdfmx and luaotfload, TeX Live's and MiKTeX's redistribution guidelines, and Knuth's renaming condition.
* [Package quirks found by the corpus](package-quirks.md) - Behaviour of individual LaTeX packages and tools, seen while growing the document corpus, that looks like an on-demand problem but is not (or is), with the cause.
