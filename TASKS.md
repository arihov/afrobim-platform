# Tasks

## Round 7, item 7.1: make saving safe (branch `claude/r7-atomic-save`)

- [x] Kernel `atomic` module: temporary file in the same folder, flush, fsync, rename over the target; temporary removed on failure
- [x] `Document::save` uses it (covers app Save and Save as, `--save-as`, the MCP tools and the Python library)
- [x] No `.bak`: the repository has no backup convention, so none was added
- [x] Autosave: preference `autosave_minutes` (default 5, 0 is off), dialog under Window and the Manage tab, `AUTOSAVE` command
- [x] Recovery file `<name>.afrobim.recover` beside the document, or in the settings folder for an unnamed one; written only when dirty
- [x] Recovery file removed after a normal save and on clean exit
- [x] Panic hook (installed in `main` only): one attempt to write the document copy to the recovery file, then the previous hook
- [x] Offer on start and on open: Recover (as unsaved changes) or Discard; closing the dialog keeps the file
- [x] Tests: atomic write, replace, failing writer, dirty-only autosave, newer-only recovery, panic path as a function, app wiring
- [x] User manual (saving section 3.15, troubleshooting) and README status row
- [ ] Not done: Windows run, atomic writes for the shortcuts manifest, survey database and CSV exports, a prompt on quit with unsaved changes (found, not in scope)
