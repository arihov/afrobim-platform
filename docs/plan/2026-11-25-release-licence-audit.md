# AfroBIM: release licence audit for the public installer (2026-11-25)

Scope: Gate 1 of `2026-11-25-afrobim-launch-and-product-roadmap.md` ("licence audit of everything inside the installer, NOTICE inside the app, project names removed from defaults"). Branch audited: `claude/satellite-basemap` at `7f4448c`. This is a desk audit by reading the repository, the packaging scripts and the dependency metadata. It is not legal advice. Where a licence is not known the document says so, and the owner or a lawyer has to settle it.

## 1. Summary

| Area | Result |
|---|---|
| Rust dependencies (474 locked packages) | Pass. `cargo deny` 0.22.1 ran here: "bans ok, licenses ok, sources ok". Nothing is GPL or AGPL. One MPL-2.0 library (acadrust), already in NOTICE. |
| AfroBIM's own code | No licence file. The 37 workspace crates have no `license` field. A public download needs an end-user licence agreement (EULA) of the owner's choosing. |
| Transport fonts (`fonts/tranh___.TTF`, `tranm___.TTF`) | **Fail: licence unknown.** The files say "Copyright Keysoft Solutions, based on fonts supplied by cbrd.co.uk". They are not the Department for Transport's own release. No licence text is in the repository. |
| Rule packs (`packs/`) | **Unknown.** Values transcribed from government manuals (UK DfT, Uganda MoWHC) and pictograms extracted from TSRGD 2016. Facts and figures are likely fine; the drawings and the Uganda manual's terms are unconfirmed. The README already says the Open Government Licence text "has not been confirmed". |
| Manuals (`Manuals/`) | Not shipped by any installer. They must stay out of every public download and ideally out of any public repository. Several are third-party commercial or Crown-copyright documents. |
| NOTICE inside the app | **Fail.** `NOTICE` is not copied into the Windows, macOS or Linux packages, and the app does not show it. |
| Bundled UI fonts and icons | Pass. Inter (SIL OFL 1.1) and Phosphor (MIT). Both licence texts are in the repository, but neither is in the installers or NOTICE. |
| Project names in defaults | Pass for product defaults, with a few items to tidy (section 6). |
| Basemap default provider | Not an installer item, but a risk: the default is Esri World Imagery, whose terms for commercial products need checking (section 7). |

## 2. What the installers contain

Read from `.github/workflows/installers.yml`, `afrobim/packaging/stage.sh`, `layout.env`, `macos/build-dmg.sh`, `linux/build-archive.sh`, `windows/build-msi.ps1`, `windows/main.wxs` and `afrobim/docs/installing.md`.

All three platforms build one staged tree with `stage.sh`, then wrap it:

| Platform | Package | Where the files go |
|---|---|---|
| Windows | `.msi` (WiX v3) | `Program Files\AfroBIM\afrobim.exe`, `packs\`, `fonts\`; Start menu shortcut; `.afrobim` file association |
| macOS | `.dmg` holding `AfroBIM.app` (universal binary) | Binary in `Contents/MacOS`, `packs/` and `fonts/` in `Contents/Resources`, `AfroBIM.icns`, `Info.plist` |
| Linux | `.tar.gz` portable archive | `afrobim`, `packs/`, `fonts/` side by side |

The workflow input `bundle_fonts` defaults to **true**, and a tag push (`afrobim-v*`) always bundles the fonts. So today a tagged build puts the Transport fonts in a public download unless someone changes the workflow.

### 2.1 Item list

Sizes are from the working tree on this branch. The executable was not built for this audit, so its size is not stated.

| # | Item | Path in repo | Size | In installer? | Source | Licence | May ship in a public commercial download? |
|---|---|---|---|---|---|---|---|
| 1 | AfroBIM executable | built from `afrobim/crates/afrobim-app` | not measured | Yes, all three | The owner's code plus the dependencies in section 3 | Proprietary; no licence file exists yet | Yes once the owner issues an EULA. Dependency notices must travel with it (section 3). |
| 2 | UK rule pack | `packs/UK-TSM-2019.json` | 115,119 B | Yes | Header comment: values quoted from DfT Traffic Signs Manual Chapter 7 (2019) and TSRGD 2016 (SI 2016/362); pictograms read from the TSRGD PDF by `tools/TsrgdExtract/extract_tsrgd.py` | Crown copyright. The Open Government Licence v3.0 normally covers DfT publications and legislation.gov.uk text, but the repository does not record which licence applies and its README says the OGL text "has not been confirmed" | **Probably yes, not confirmed.** Needs the OGL attribution line in NOTICE and a check that the TSRGD pictograms fall under it. |
| 3 | Uganda rule pack | `packs/UG-TSM-2004/*.xml` (17 files) | 34,879 B in total (largest `sign-catalog.xml`, 18,190 B) | Yes | Header: "Values are transcribed from the July 2004 guidance appendix clause by clause" of the MoWHC Traffic Signs Manual Vol. 2 | Unknown. A Uganda government manual; no licence statement found | **Unknown.** Numbers and rules are facts and are low risk, but the owner should record who holds the copyright and ask the Ministry (or MoWT) for written permission. |
| 4 | Transport Medium font | `fonts/tranm___.TTF` | 83,552 B | Yes (`--no-fonts` leaves it out) | Font name table: "Key Transport Medium", version 1.003, "Copyright Keysoft Solutions, based on fonts suplied by cbrd.co.uk" | **Unknown.** Keysoft Solutions (the maker of the TrafixPLAN software) holds the copyright. No licence text is in the repository. The official DfT Transport font release has its own terms; this is a different build | **No, not until a licence is obtained.** Redistribution is not covered by anything on file. |
| 5 | Transport Heavy font | `fonts/tranh___.TTF` | 85,456 B | Yes | "Key Transport Heavy", same notice | Unknown, as item 4 | **No, not until a licence is obtained.** |
| 6 | Inter UI font | `afrobim/crates/afrobim-app/assets/fonts/Inter-Variable.ttf` (876,576 B), licence in `Inter-OFL.txt` | Compiled into the executable with `include_bytes!` | SIL Open Font Licence 1.1, copyright 2020 The Inter Project Authors | **Yes.** The OFL allows bundling and commercial sale. The copyright notice and licence text must accompany the software; they are not in the installer today. |
| 7 | Phosphor icon font | from crate `egui-phosphor` 0.9.0 (`ph::` constants); licence copy `egui-phosphor-LICENSE-MIT` | Compiled in | MIT (Phosphor Icons, copyright 2020) | **Yes**, with the MIT notice reproduced. |
| 8 | egui default fonts | crate `epaint_default_fonts` | Compiled in | OFL-1.1 and Ubuntu-font-1.0 (listed in `deny.toml`) | **Yes**, with notices. |
| 9 | Application icon | `afrobim/packaging/icons/afrobim.ico` (6,726 B), `.icns` (31,322 B), `afrobim-512.png` (6,475 B) | Yes (`.ico` in the MSI and the exe, `.icns` in the app bundle, `.png` compiled in) | Generated by `make_icons.py`, which says it is a placeholder, "NOT a brand mark" | Owner's work (created by the project), no third-party content | **Yes**, but it is a placeholder. Replace with the real logo before alpha. |
| 10 | Manuals | `Manuals/` (22 files, Git LFS; this checkout holds 130-byte pointer files only) | n/a | **No.** Neither `stage.sh` nor the WiX file copies it | UK DfT Traffic Signs Manual chapters, TSRGD 2016, "UK SIGNS MANUAL COMBINED", the TrafxPLAN Sign 2026 manual, help and "What's new" files (`from-qwen/`) | DfT items: Crown copyright (OGL likely). **TrafxPLAN files: commercial software documentation of another company, no right to redistribute.** | **Must not ship.** Also should not sit in a public repository. Keep them private. |
| 11 | Examples | `examples/` (CSV, XML and `out/*.dxf`; 44 KB) | **No** | Samples written for the project; they name Ugandan towns (Kasese, Masheruka, Kampala, Jinja, Entebbe, Mukono, Iganga is not in them) | Owner's work | Not shipped. If added later they are safe: place names are public facts, and no client project is named. |
| 12 | Schema files | `packs/Schema/*.xsd` | **No** (the app does not read them) | Owner's work | Not shipped. |
| 13 | NOTICE | `NOTICE` (repository root) | **No** | Lists acadrust (MPL-2.0), ACadSharp (MIT), BIMCraft (MIT) and ODA File Converter (not bundled) | n/a | **Must ship** (section 4). |
| 14 | User manual | `afrobim/docs/user-manual.md` | **No** | Owner's work | Not shipped. It does cover MAPCAPTURE, MAPEXPORT, GISIMPORT, GREENBIM, culverts and recovery. |
| 15 | Rust dependencies | `afrobim/Cargo.lock` | Compiled in | See section 3 | See section 3 |
| 16 | Python package | `afrobim/python/` | **No** | Owner's work | Not shipped. |

### 2.2 Housekeeping files

`build1.log` and `testbuild.log` are committed at the repository root. They are not shipped but should be removed from a repository that is ever made public.

## 3. Rust dependency licences

Command and result (run in `afrobim/`, cargo-deny 0.22.1 installed for this audit):

```
cargo deny --manifest-path Cargo.toml --config deny.toml check licenses bans sources
bans ok, licenses ok, sources ok
```

`cargo deny list` groups the 474 locked packages by licence. The counts overlap, because a crate with "MIT OR Apache-2.0" appears under both. Every crate is permissive or weak-copyleft with an alternative:

| Licence | Packages | Note |
|---|---|---|
| MIT | 341 | notice must be reproduced |
| Apache-2.0 | 266 | notice and NOTICE files reproduced |
| Apache-2.0 WITH LLVM-exception | 7 | |
| Zlib | 17 | |
| Unicode-3.0 | 20 | the `icu_*` family |
| Unlicense | 7 | public-domain style |
| BSD-3-Clause | 6, BSD-2-Clause 2 | |
| ISC | 6 | includes `ring`, `rustls-webpki` |
| BSL-1.0 | 3 | |
| CDLA-Permissive-2.0 | 2 | `webpki-roots` (Mozilla roots) |
| 0BSD | 1 | |
| OFL-1.1 and Ubuntu-font-1.0 | 1 | `epaint_default_fonts` |
| **MPL-2.0** | 1 | `acadrust` 0.6.8 (DWG reader and writer). File-level copyleft. Used unmodified, so only the notice and a pointer to its source are needed. Already in NOTICE. |
| LGPL-2.1-or-later | 2 | `r-efi` 5.3.0 and 6.0.0, offered as "MIT OR Apache-2.0 OR LGPL-2.1-or-later". AfroBIM takes it under MIT. It is a UEFI crate and not used on desktop targets. |
| "Unlicensed" | 37 | **AfroBIM's own crates**: no `license` field. Not a third-party problem, but fix it (section 8). |

Findings:

1. No GPL, AGPL or SSPL dependency, so closed-source distribution is allowed.
2. **MIT, Apache-2.0, BSD, ISC and OFL all require the copyright notice to accompany binary distribution.** NOTICE says "Other dependencies are listed with their licences in each crate's Cargo.toml and in Cargo.lock". That is not enough for an installer: users do not have the source tree. Generate a `THIRD-PARTY-LICENSES.txt` with `cargo about` or `cargo deny` output, and ship it with NOTICE.
3. `deny.toml` is consistent with the lock file (it already allows MPL-2.0, OFL-1.1 and Ubuntu-font-1.0).
4. The CI line `command: check --config afrobim/deny.toml licenses bans sources` (`.github/workflows/ci.yml:199`) places `--config` after `check`. With cargo-deny 0.22.1 that is rejected ("unexpected argument '--config' found"; the option must come before `check`). The pinned action (v2.1.4) may install a different version, so this may work in CI, but it has not been observed. Check the last CI log for the `deny` job; if it fails, move `--config` first or use the action's `arguments` input.
5. Advisories (known vulnerabilities) were not checked, by design of the CI job. Run `cargo deny check advisories` before alpha.
6. Windows-only dependencies (`windows-sys`, `winreg`, `embed-resource`, `vswhom`) are included in the check because `deny.toml` checks every target.

## 4. NOTICE inside the app bundles

Today:

* `stage.sh` copies only the binary, `packs/` and `fonts/`. It does not mention NOTICE or any licence file.
* `build-dmg.sh` (macOS) adds `Info.plist` and the icon to the stage output. It does not copy NOTICE. Its final check lists only the binary, `Info.plist`, the icon and two pack files.
* `build-archive.sh` (Linux) packs what `stage.sh` produced. Its README text points to the repository docs.
* `main.wxs` (Windows) lists each file by name. It has no NOTICE entry, and its own comment says "No licence dialog: the repository has no LICENSE file".
* The app has an About window (`Action::About`), but nothing in the code reads or shows NOTICE.

What to change, by file:

| File | Change |
|---|---|
| `afrobim/packaging/stage.sh` | After the packs step, `install -m 0644 "$repo/NOTICE" "$res/NOTICE"` and the same for `THIRD-PARTY-LICENSES.txt` and a `LICENSE.txt` (the EULA), plus `$res/fonts/Inter-OFL.txt` if the UI font is also placed on disk. On macOS `$res` is `Contents/Resources`, on Linux the archive root. Add `NOTICE` to `check_layout.py` so the Linux job fails if it is missing. |
| `afrobim/packaging/windows/main.wxs` | Add `File` entries for `NOTICE`, `THIRD-PARTY-LICENSES.txt` and `LICENSE.txt` beside `afrobim.exe`. Convert the EULA to RTF and pass it as `WixUILicenseRtf`, as the file's own comment describes, so the installer shows an accept screen. Add the files to the install-test list in `installers.yml`. |
| `afrobim/packaging/macos/build-dmg.sh` | No code beyond `stage.sh`, but add `Contents/Resources/NOTICE` to the final verification list. Optionally add `NOTICE` and `LICENSE.txt` next to the app in the disk image. |
| `afrobim/packaging/linux/build-archive.sh` | Covered by `stage.sh`. Name the files in the archive's README. |
| `afrobim/crates/afrobim-app` (About window, `app.rs` around `show_about`) | Add a "Licences" button that opens NOTICE, found by the same lookup as `packs/` (`signs_glue::find`). This is a small code change and is **outside this task**, which is documents only. |
| `NOTICE` | Add: Inter (OFL 1.1) and Phosphor (MIT) entries; the OGL v3.0 attribution line for the UK pack once confirmed; the Transport font notice once a licence exists; a line pointing to `THIRD-PARTY-LICENSES.txt`. |
| `.github/workflows/installers.yml` | Add a step that generates `THIRD-PARTY-LICENSES.txt` before staging, and a step that fails if the staged tree has no NOTICE. Default `bundle_fonts` to **false** until the font licence is settled. |

Also decide the licence of AfroBIM itself (roadmap decision 4). The file `LICENSE.txt` above is the EULA. Until it exists, "all rights reserved" applies (README), and no user has permission to run the software.

## 5. The Transport fonts: what is known and what to do

Known from the files themselves: both TTFs carry "Copyright Keysoft Solutions, based on fonts suplied by cbrd.co.uk", names "Key Transport Medium" and "Key Transport Heavy", version 1.003. The repository README and `installing.md` call them "third-party files: do not redistribute them without a licence that allows it". No licence, receipt or e-mail is on file. Git history shows they arrived with the merge of the SignCore work on 2026-11-14.

Not known: whether Keysoft allows redistribution, and whether the DfT's official free Transport release (which the DfT lets people use to make road signs) can replace them. The exact-metric sign tests and the sign engine use these files, so they are a functional dependency, not decoration.

Options, in the order the audit recommends:

1. **Ship without the fonts** (`bundle_fonts=false`). The app already reports "Traffic signs unavailable" and honours `AFROBIM_FONTS`. Signs would be disabled for public users, which hurts the main selling point.
2. **Ask Keysoft Solutions in writing** for permission to redistribute these two files with AfroBIM, including commercial use. Keep the reply in the repository.
3. **Replace them** with the official DfT Transport font, if its terms allow redistribution inside a commercial product (check the current DfT licence text), and re-run the exact-metric tests because glyph metrics may differ by version.
4. **Download on first use**, from a source the user accepts the terms of. This moves the licence question to the user but adds a network step.

## 6. Project names and real data in defaults

Searched all `.rs`, `.csv`, `.xml`, `.json`, `.toml`, `.py`, `.md` files under `afrobim/`, `packs/`, `examples/` for Km 39+136, Iganga, Bulopa, Kamuli, Kafunjo, Al Noud, ACP, MTTC and personal paths.

| Where | What | In an installer? | Action |
|---|---|---|---|
| `afrobim-drainage/src/lib.rs:74-78` | Culvert placeholders 1000 mm, 100 mm wall, 10 m long, flagged `placeholder` | Yes (compiled in) | Fine. Generic values, marked as placeholders. |
| Culvert bar schedule | `docs/reference/culvert/` holds the full "Km 39+136" schedule (CSV and README) | No (docs only) | Fine for now. The culvert bar-schedule engine is not on this branch. **When it merges, check that its defaults are not the Km 39+136 numbers** and that no drawing title, chainage or client name is a default. |
| `afrobim-signs/tests/golden/map_roundabout_ahead_right.json` | Destination "Iganga" | No (test fixture) | Harmless (a town name). |
| `afrobim-dwg/src/tests.rs:126` | Local macOS paths `/Users/dtmengineering/_Venture/afrobim/dwg-trials` and `.../kafunjo` | No (test only) | Remove: it names the owner's machine and a project. Replace with an environment variable. |
| `afrobim-app/src/map_capture.rs:236`, `basemap.rs:560`, `afrobim-raster` README and `georef.rs` comments | "acp-capture.png", "the ACP site", "world file ACP.pgw" | Partly: the string `acp-capture.png` is compiled in as the default capture file name | Rename the default to `afrobim-capture.png` and reword the comments. |
| `afrobim/README.md:145`, `afrobim/docs/user-manual.md:1494` | Mention the ACP project | No (docs) | Reword before the manual is published. |
| `afrobim-app/src/statusbar.rs` tests | "Kampala Northern Bypass" in test strings | No | Harmless. |
| Sign samples and tests | Kampala, Kasese, Masheruka, Jinja, Entebbe, Mukono | `examples/` not shipped; tests not shipped | Public place names. Fine. |
| Demo data (`civil_demo.rs`) | Declares itself demonstration data with example values and true-size UTM coordinates | Yes (`--demo civil`) | Fine. Confirm the demo names contain no client project (a quick read found none). |
| `docs/plan/*` | Names ACP, Kafunjo, Iganga-Bulopa-Kamuli, AIL, Al Noud 2 | No | Fine, but do not put `docs/plan` in a public repository. |

Result: **no real project name is a product default.** The tidy-ups above are small.

## 7. Related risks that are not in the installer

* **Basemap default.** `afrobim-raster/src/tiles.rs` makes "Esri World Imagery (satellite)" the first and default provider. Esri's terms for its free tile service are not the same as a licence to use the tiles in a sold product, and the app shows the attribution only. The roadmap already lists this as R3. Before public beta, read Esri's current terms; consider defaulting to OpenStreetMap-based tiles for commercial use or asking users to supply a key. The EOX Sentinel-2 2020 provider is already labelled non-commercial; keep it behind a visible warning.
* **ODA File Converter** is not bundled and NOTICE says so. Keep it that way.
* **acadrust MPL-2.0.** If the owner ever patches acadrust, those patched files must be published. Today it is unmodified.
* **Telemetry and privacy notice** are Gate 2 items, not Gate 1.

## 8. Gate 1 pass/fail list

The Gate 1 text in the roadmap has five parts. Items marked "not assessed" need a person at a screen and cannot be judged from the code.

| # | Gate 1 criterion | Result | What is needed |
|---|---|---|---|
| 1 | Rust dependency licences checked | **PASS** | `cargo deny` clean. Generate `THIRD-PARTY-LICENSES.txt` (item 4 below). Run an advisories check before alpha. |
| 2 | Fonts licensed for redistribution | **FAIL** | Transport font licence unknown. Get written permission from Keysoft Solutions, or replace the fonts, or set `bundle_fonts` to false (section 5). |
| 3 | Rule packs licensed | **FAIL (unconfirmed)** | Confirm OGL v3.0 for the UK pack and add its attribution to NOTICE. Get written confirmation for the Uganda pack. |
| 4 | Manuals kept out of installers | **PASS** | Confirmed: nothing under `Manuals/` is staged. Keep the folder out of any public repository. |
| 5 | NOTICE (and licences) shipped inside the app | **FAIL** | Section 4 table: edit `stage.sh`, `main.wxs`, the three verification lists, `installers.yml`; add a Licences button to About (code change). |
| 6 | AfroBIM's own licence / EULA | **FAIL** | None exists. Decide, write `LICENSE.txt`, add `license` or `license-file` to the workspace `Cargo.toml`, convert to RTF for the MSI. |
| 7 | Project names removed from defaults | **PASS (tidy-ups listed)** | Remove the personal paths in `afrobim-dwg/src/tests.rs`; rename `acp-capture.png`; reword two doc lines; re-check when the culvert engine merges. |
| 8 | Placeholder icon replaced | **FAIL (cosmetic)** | `make_icons.py` says its output is a placeholder. Not a legal blocker. |
| 9 | Manual up to date | **PARTLY** | `user-manual.md` covers MAPCAPTURE, MAPEXPORT, GISIMPORT, GREENBIM, DWG, recovery and culverts (last change 2026-11-25). Whether the text matches the current screens needs a read by a person. |
| 10 | Hands-on smoke test of MAPCAPTURE, MAPEXPORT, GREENBIM, GISIMPORT, DWG, recovery, culvert | **NOT ASSESSED** | Needs the app running on a desktop. The Windows `.msi` has never been run on a real Windows machine (`installing.md`). |

**Gate 1 as a whole: FAIL.** The blockers are items 2, 3, 5 and 6, and all four are decisions or small script edits, not engineering projects.

## 9. Exact changes, in order

1. **Owner decisions:** AfroBIM's licence and EULA text; whether to ask Keysoft for font permission, replace the fonts or ship without them; who asks the Ministry about the Uganda pack.
2. Set `bundle_fonts` default to `false` in `installers.yml` and make the tag-push line `BUNDLE_FONTS: ${{ github.event_name == 'push' || inputs.bundle_fonts }}` follow the decision (today a tag always bundles them).
3. Add `LICENSE.txt` (EULA) at the repository root. Add `license-file` (or `license = "LicenseRef-AfroBIM-EULA"`) to the workspace `Cargo.toml` and let each crate inherit it, so `cargo deny list` stops reporting 37 "Unlicensed" crates. Keep `[licenses.private] ignore = true` if the crates stay unpublished.
4. Generate `THIRD-PARTY-LICENSES.txt` in CI (for example `cargo about generate` with a template) and upload it with the build.
5. Edit `stage.sh` to install `NOTICE`, `LICENSE.txt` and `THIRD-PARTY-LICENSES.txt` into the resources folder; add them to `check_layout.py`, `main.wxs`, the macOS verification list and the Windows install test.
6. Update `NOTICE`: add Inter (OFL 1.1), Phosphor (MIT), the OGL attribution for the UK pack, and the font notice once settled.
7. Convert the EULA to RTF and wire `WixUILicenseRtf` so the MSI shows it.
8. Small code tasks for the next build session (not done here): About window "Licences" button; rename `acp-capture.png`; remove the personal paths from `afrobim-dwg/src/tests.rs`; remove `build1.log` and `testbuild.log`.
9. Replace the placeholder icon when the logo is ready.
10. Re-run this audit on the release commit, including the culvert engine defaults and a fresh `cargo deny check`.
