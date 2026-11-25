# AfroBIM: in-app update check (design note, 2026-11-25)

Status: design only. No network code exists yet, and none should be written until the owner approves this note.

## Purpose

Tell a user, once the app is running, that a newer version has been published, and where to get it. The app never downloads or installs anything by itself in the first version.

## Source

The public releases feed:

`GET https://api.github.com/repos/arihov/afrobim-releases/releases/latest`

This returns the newest published release that is neither a draft nor marked as a pre-release. The release workflow therefore publishes without the pre-release flag, even for beta builds. The fields used are `tag_name` (for example `v0.2.0`), `html_url`, `published_at`, `name` and the first lines of `body`. Nothing else is read.

## Behaviour

1. **Opt-in.** The check is off until the user agrees, in the same first-start dialog that asks about usage and crash reports (roadmap section 3), as a separate switch. The choice is kept in the user settings file and can be changed under Help, then Settings. A manual "Check for updates" menu item always works, whatever the switch says, because the user asked for it.
2. **When.** If opted in: once at start-up, at most once every 24 hours, on a background thread, with a timeout of 5 seconds. A failure (no network, rate limit, bad JSON) is silent, and the next attempt waits for the next day.
3. **Comparison.** Strip the leading `v` from `tag_name` and compare as semantic versions with the running version (`CARGO_PKG_VERSION`). Notify only when the feed version is higher. A user who runs a newer build than the feed (for example a developer build) sees nothing.
4. **What the user sees.** A quiet notice in the status bar or Help menu: "AfroBIM 0.2.0 is available" with a button that opens `html_url` in the system browser, and a "Skip this version" choice that stores the skipped version.
5. **No automatic install.** Installers are replaced by the user, from the download page or the release. Automatic update needs signed builds and a signature check inside the app, which is a later decision.

## Privacy

- The request carries no identifier, no document data, no file names and no licence information.
- The only header set is `User-Agent: AfroBIM/<version>` (GitHub requires one) and `Accept: application/vnd.github+json`. Version and operating system are not sent in any other form.
- GitHub sees the user's IP address, as any website would. This is stated in the privacy notice.
- The check is separate from the opt-in usage report. Accepting one does not enable the other.
- The app does not call any AfroBIM server for updates, so there is nothing for AfroBIM to log.

## Limits and failure modes

- Unauthenticated GitHub API calls are limited to 60 per hour per IP address. One call per day per install is far below that, but a shared office network may see the limit. On HTTP 403 or 429 the app stops quietly and retries the next day.
- If the releases repository is renamed or made private the check fails silently. The URL is kept in one constant so it can be changed in a point release.
- Only HTTPS is used, with the system certificate store. A response larger than 256 KB is discarded.

## Implementation notes (for later)

- A small blocking HTTP client on a worker thread, with no new async runtime. Check the licence of any new dependency against `afrobim/deny.toml` and add it to the third-party notices.
- Parse with the existing JSON dependency into a struct holding the five fields above. Unknown fields are ignored.
- Unit tests for version comparison, tag parsing (`v1.2.3`, `v1.2.3-beta.1`, bad input) and the skip logic, with the HTTP call behind a trait so tests use a canned response and no test touches the network.
- The download page (`web/download/index.html`) reads the same feed, so a change to the feed shape must update both.

## Open questions for the owner

1. Is the opt-in default correct (off), or should the update check alone default to on, with a clear first-start notice?
2. Should pre-release builds be offered to a user who chooses a "beta channel" setting? This would use the releases list rather than `latest`.
