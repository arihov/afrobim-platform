# Installing AfroBIM

AfroBIM is built as an unsigned Windows installer (`.msi`), an unsigned macOS disk image (`.dmg`) and a
Linux portable archive (`.tar.gz`). A GitHub Actions workflow builds them on request.

## What has and has not been tested

Read this first.

- **The installer workflow has never run.** It was written while GitHub Actions could not start runners on
  this account (a billing or usage problem), so no job in `.github/workflows/installers.yml` has executed.
  Expect to fix small things on the first real run.
- **Nothing here was built or run on Windows or macOS.** The `.msi` and `.dmg` scripts are untested.
- **Checked on Linux only:** the staged file layout (the Linux archive is built with the same staging script
  the other two use, unpacked outside the repository and started headless with `--demo --screenshot`, and it
  found its rule packs and fonts); the workflow YAML (`actionlint`); the shell scripts (`bash -n`,
  `shellcheck`); that `main.wxs` is well-formed XML and lists exactly the files the staging script produces;
  that `Info.plist.in` parses as a property list.
- **Not checked:** `main.wxs` against the WiX schema or by compiling it, any macOS tool (`lipo`, `codesign`,
  `hdiutil`), the PowerShell script, the install wizard, the file association.

## What the app needs at run time

The app loads these from disk. Nothing is compiled in.

| File | Looked up as | Override |
|------|--------------|----------|
| `packs/UK-TSM-2019.json` | `packs/UK-TSM-2019.json` | `AFROBIM_PACK` (the JSON file) |
| `packs/UG-TSM-2004/*.xml` (17 files) | `packs/UG-TSM-2004/pack.xml` | none |
| `fonts/tranh___.TTF`, `fonts/tranm___.TTF` | `fonts/` | `AFROBIM_FONTS` (the folder) |

The lookup (`find` in `crates/afrobim-app/src/signs_glue.rs`) tries the environment variable, then every
folder above the executable, then every folder above the working directory, joining the relative path.
So `packs/` and `fonts/` **next to the executable** are found today. This is the layout of the Windows
install (`C:\Program Files\AfroBIM\`) and the Linux archive.

**macOS needs one change in the app sources.** The bundle keeps data in `Contents/Resources`, which that
lookup does not search (it would find `Contents/packs`, which a signed bundle should not contain). Verified
on Linux by staging the same layout and starting the app: with the data in a sibling `Resources` folder the
app reports no traffic-signs module; with the working directory set to `Resources`, or with the
environment variables set, it loads normally.

The change, in `find` in `afrobim/crates/afrobim-app/src/signs_glue.rs`, is a lookup order:

1. the environment variable (as now);
2. next to the executable (`exe_dir/relative`, as now);
3. the platform resource directory: on macOS `exe_dir/../Resources/relative`;
4. the folders above the executable and the working directory (as now, for source checkouts).

Also give the Uganda pack its own override variable (today it has none; the code passes a variable name
that is never set). Until that change lands, the workflow builds the `.dmg` with `--launcher`: the app's
executable is a small script that starts the real binary with `Contents/Resources` as its working directory,
which the existing lookup accepts. When the lookup change is in, remove `--launcher` from the workflow.

Two other limits of the macOS image, both in the app and not the packaging:

- The bundle declares the `.afrobim` document type, but double-clicking a file in Finder delivers an
  open-document event, not a command-line argument. The app reads only arguments, so Finder would start
  AfroBIM without loading the file. (On Windows the association passes the file as an argument, which works.)
- The window title bar and Dock use the icon from the bundle, but the Windows `.exe` itself has no embedded
  icon (no `build.rs` was added). Shortcuts and the file type show the icon; the file in Explorer does not.

## Making the installers

1. On GitHub open the repository, then **Actions**, then **AfroBIM installers**, then **Run workflow**.
   Tick the platforms wanted. Cost: a Windows minute counts as 2 Linux minutes and a macOS minute as 10, so
   untick macOS unless a `.dmg` is needed.
2. Or push a tag named `afrobim-vX.Y.Z` where `X.Y.Z` equals the workspace version in `afrobim/Cargo.toml`
   (currently 0.1.0, so `afrobim-v0.1.0`). A tag builds all three and also creates a **draft** release.
3. Ordinary pushes and pull requests never start this workflow.

Where the files appear: on the run page, under **Artifacts** (`afrobim-windows`, `afrobim-macos`,
`afrobim-linux`), each holding the installer and a `SHA256SUMS-*.txt` file. For a tag, the draft release
(under **Releases**) holds all of them with one `SHA256SUMS.txt`. Artifacts are kept for GitHub's default
retention period. Check a download with `sha256sum -c SHA256SUMS.txt` (Linux), `shasum -a 256 -c` (macOS) or
`Get-FileHash` (Windows).

## Installing on Windows

1. Download `AfroBIM-X.Y.Z-windows-x64.msi`.
2. Double-click it. Windows asks for administrator permission (the install is for all users).
3. **Expected warning.** The installer is unsigned, so Windows SmartScreen may show "Windows protected
   your PC". Click **More info**, then **Run anyway**. (This wording is the commonly seen one; Microsoft's
   SmartScreen overview page, checked 2026-11-15, says unrecognised downloads get a warning but does not
   quote this screen, so the exact text may differ on your Windows version.)
4. Follow the wizard. The feature page lets you change the folder and tick **Desktop shortcut** (off by
   default). The Start menu shortcut is always created.

Installed to `C:\Program Files\AfroBIM\`. A newer `.msi` with a higher version replaces the old one.

Uninstall: **Settings, Apps, Installed apps, AfroBIM, Uninstall** (or `msiexec /x` on the `.msi`). Your own
`.afrobim` documents are not touched.

System requirements: 64-bit Windows. The exact minimum version is **not verified**: it depends on the Rust
standard library and the graphics stack the app uses, and nothing was run on Windows.

## Installing on macOS

1. Download `AfroBIM-X.Y.Z-macos-universal.dmg` and open it.
2. Drag **AfroBIM** onto the **Applications** shortcut in the window.
3. **Expected warning.** The app is signed only ad hoc, not with an Apple Developer ID and not notarised, so
   Gatekeeper will refuse the first launch. Apple's support page "Open a Mac app from an unknown developer"
   (checked 2026-11-15) gives these steps: try to open the app once, then open **System Settings**, choose
   **Privacy & Security**, scroll to the **Security** section, click **Open** next to the blocked app, click
   **Open Anyway**, enter your login password and click **OK**. Apple notes that the Open Anyway button is
   available for about an hour after the attempt. After that the app opens normally.
   Older macOS versions used right-click, **Open**; that is described from general knowledge, not checked.
4. Eject the disk image.

Uninstall: drag **AfroBIM** from Applications to the Bin. Nothing else is installed.

System requirements: the image is built with a deployment target of macOS 11.0 and declares it in
`Info.plist` (`LSMinimumSystemVersion`). Verified against the Rust platform-support page: Rust's own minimum is
macOS 10.12 on Intel and macOS 11.0 on Apple silicon, so 11.0 is the lowest value that covers both halves of
the universal binary. Not verified: that the app's windowing and graphics crates (eframe 0.31, winit 0.30)
work on every version from 11.0 up, and nothing was run on macOS.

Why "ad hoc": Apple silicon Macs are generally documented to require every arm64 program to carry some code
signature, and ad hoc signing (`codesign -s -`) supplies a signature with no identity. This statement was not
confirmed against an Apple page when this was written (the fetches returned no body text). It does not make
the app trusted by Gatekeeper; it only lets the binary run once the user approves it.

## Installing the Linux archive

Unpack the `.tar.gz` anywhere and run `./afrobim` from inside the folder (or `./afrobim --demo`). It needs a
desktop session with OpenGL (X11 or Wayland). Delete the folder to uninstall. Only tested here on the build
machine's own distribution, headless under Xvfb.

## Decisions for the owner

- **Signing.** Unsigned installers work, but every user sees the warnings above and some company
  computers block them outright. Removing the warnings needs: for Windows, a code-signing certificate from a
  certificate authority (paid, with identity checks; a new certificate also needs time to build SmartScreen
  reputation unless it is an extended-validation one); for macOS, an Apple Developer Program membership
  (paid yearly), a Developer ID Application certificate, then signing with it and notarising with Apple's
  `notarytool`. The workflow would need those credentials as repository secrets. This is the owner's decision
  and cost; nothing here assumes it.
- **Licence.** The repository has no `LICENSE` file, so the installers show no licence page and the
  `Cargo.toml` has no `license` field. Decide licensing (and end-user terms) before selling.
- **Transport fonts.** `fonts/tranh___.TTF` and `fonts/tranm___.TTF` are third-party files (the project README
  says not to redistribute them without a licence that allows it). The app cannot draw signs without them, so
  the workflow bundles them by default. Untick **bundle_fonts** for a build without them; the app then shows
  "Traffic signs unavailable" until `AFROBIM_FONTS` points at a folder holding them. Settle the font licence
  before any installer leaves the owner's hands. The UK rule pack also states it carries Open Government
  Licence material (see the repository README), which may require an attribution notice.
- **Icon.** `afrobim/packaging/icons/` holds a generated **placeholder** (a blue square with a triangle),
  made by `make_icons.py`. Replace the three files with the real logo.
- **Bundle identifier.** `com.afrobim.AfroBIM` is a placeholder. Choose a reverse-domain identifier for a
  domain the owner controls (pass `--bundle-id`, or edit the default in `build-dmg.sh`). Changing it later
  makes macOS treat the app as a different one.
- **Windows upgrade code.** `0DD46886-4D75-441A-B70C-A5445285E0D7` in `main.wxs` identifies the product for
  upgrades. Never change it once an installer has been given to anyone.
- **Version.** The installers take the workspace version in `afrobim/Cargo.toml`.

## For maintainers: the packaging files

```
afrobim/packaging/
  stage.sh                  stages binary + packs + fonts (all platforms use this)
  check_layout.py           checks main.wxs and Info.plist.in against a staged tree
  linux/build-archive.sh    the portable .tar.gz
  windows/main.wxs          hand-written WiX v3 source
  windows/build-msi.ps1     candle + light (cargo-wix is not used)
  macos/Info.plist.in       bundle metadata template
  macos/build-dmg.sh        .app, lipo, ad hoc codesign, hdiutil
  icons/make_icons.py       draws the placeholder icons (writes the .ico and .icns beside it)
```

The Windows build uses WiX Toolset v3. The `windows-latest` runner image (Windows Server 2025) lists "WiX
Toolset 3.14.1.8722" in its software list (`actions/runner-images`, `images/windows/Windows2025-Readme.md`,
checked 2026-11-15), and the workflow installs WiX 3.14 from Chocolatey if it is ever missing. When a file is
added to `packs/` or `fonts/`, add it to `stage.sh` (if it is a new kind of file) and to `main.wxs`;
`check_layout.py`, run by the Linux job, fails if the two disagree.
