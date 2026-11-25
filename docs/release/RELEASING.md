# Releasing AfroBIM

Source stays in the private repository `arihov/afrobim-platform`. Installers are published as GitHub Releases in the public repository `arihov/afrobim-releases` by `.github/workflows/release.yml`.

## One-off set-up (owner)

Add these under **Settings, Secrets and variables, Actions** in `arihov/afrobim-platform`.

### Required

| Secret | What it is |
|---|---|
| `RELEASES_TOKEN` | A fine-grained personal access token. Resource owner: `arihov`. Repository access: **only** `arihov/afrobim-releases`. Permission: **Contents: Read and write**. Set an expiry and diarise the renewal. |

Without it the build jobs still run, and the publish job stops with a clear error.

### Optional: macOS signing and notarisation (Apple Developer Program, US$99 a year)

All five must exist for a notarised build. If any is missing, the disk images are ad hoc signed only.

| Secret | What it is |
|---|---|
| `APPLE_CERT_P12` | The "Developer ID Application" certificate with its private key, exported from Keychain Access as `.p12`, then base64 encoded: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERT_PASSWORD` | The password chosen when exporting the `.p12` |
| `APPLE_ID` | The Apple ID e-mail of the developer account |
| `APPLE_TEAM_ID` | The 10-character team ID (Apple Developer, Membership details) |
| `APPLE_APP_PASSWORD` | An app-specific password created at appleid.apple.com for notarisation |

### Optional: Windows signing

Pick one route. With neither, the `.msi` is unsigned. The script is `afrobim/packaging/windows/sign.ps1`.

| Secret or variable | What it is |
|---|---|
| `WINDOWS_SIGN_COMMAND` (secret) | A command line with `{file}` standing for the file to sign. Use it for a cloud HSM or Azure Artifact Signing, for example the vendor's command-line signer. This is the route for certificates whose key cannot be exported (all new public certificates). |
| `WINDOWS_CERT_PFX_BASE64` and `WINDOWS_CERT_PASSWORD` (secrets) | A base64-encoded `.pfx` and its password, signed with `signtool`. Only for a certificate whose key may be exported, such as an internal test certificate. |
| `WINDOWS_TIMESTAMP_URL` (variable, optional) | RFC 3161 timestamp server. Default `http://timestamp.digicert.com`. |

If the signing command needs its own credentials (for example Azure tenant, client and secret), add them as further secrets and reference them in the command; they are passed through the step environment only if the workflow is edited to expose them. Which provider to buy is still an owner decision (roadmap 10.1).

### Optional variable

| Variable | Effect |
|---|---|
| `RELEASE_BUNDLE_FONTS` | Set to `true` to include the Transport fonts in public installers. Left unset they are excluded, because the licence decision is open (roadmap 10.2). |

## Cutting a release

1. Merge the work to `main` and check CI is green on Linux, Windows and macOS (run CI by hand to include all three).
2. Raise the version in `[workspace.package]` in `afrobim/Cargo.toml` (for example `0.2.0`) and commit it.
3. Optional: write `docs/release/notes/v0.2.0.md` with the user-facing notes. If the file is absent the release notes list commit subjects since the previous tag. Those subjects become public, so write the file for anything sensitive.
4. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`. The tag must equal `v` plus the workspace version, or the run stops at its first job.
5. Watch the run in the Actions tab. When it is green, check the release at `https://github.com/arihov/afrobim-releases/releases`. It contains the `.msi`, two `.dmg` files (arm64 and x86_64), the Linux `.tar.gz` and `SHA256SUMS`.
6. The download page and the in-app update check read `releases/latest`, so they show the new version at once.

A trial run: Actions, Release, **Run workflow**, leaving "publish" unticked. It builds everything and keeps the files as workflow artifacts for 90 days, and publishes nothing.

To withdraw a bad release, delete it in `afrobim-releases` (and its tag), then fix and re-tag with a new patch version. A version already downloaded cannot be recalled.

## Reading download counts

```
gh api repos/arihov/afrobim-releases/releases --jq '.[].assets[] | {name, download_count}'
```

Counts are per file and include the checksum file. Add `--paginate` once there are more than 30 releases. Roadmap section 3 describes pulling these into the dashboard daily.

## Known limits

- The workflow has not yet run end to end. Expect small fixes on the first real run, as the earlier `installers.yml` notes.
- `installers.yml` (tags `afrobim-v*`) still exists and makes a draft release in the private repository. Retire it once this workflow has worked.
- Linux and Windows are x86_64 only. macOS Intel is cross-compiled on an Apple silicon runner and is untested on Intel hardware.
- Unsigned installers show SmartScreen and Gatekeeper warnings (see `afrobim/docs/installing.md`).
