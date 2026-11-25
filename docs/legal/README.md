# AfroBIM legal and protection set

> Summary prepared on 25 November 2026. Not legal advice. All texts are drafts for review by a Ugandan advocate before release.

| # | Protection | File or action | Status |
|---|---|---|---|
| 1 | **Private source repository.** The source (`arihov/afrobim-platform`) stays private. Only installers, docs and release notes go to the public repository `arihov/afrobim-releases`. | GitHub settings | In place; check that no `docs/plan`, `Manuals/` or source goes to the public repository. |
| 2 | **Proprietary licence** (all rights reserved) | `LICENSE`, `license = "LicenseRef-AfroBIM-Proprietary"` in Cargo, SPDX header in each AfroBIM `.rs` file | Done |
| 3 | **End-user licence** for the binaries | `EULA.md` (shipped in every installer and shown in About > Licences) | Draft; advocate to review; paid-tier terms still placeholders |
| 4 | **Public content licence** (CC BY-ND 4.0) | `docs/legal/PUBLIC-CONTENT-LICENCE.md` | Done; add legal code to the releases repo |
| 5 | **Trademarks** | `TRADEMARKS.md` | Advice only; **nothing filed**. File with URSB, then consider ARIPO and Madrid |
| 6 | **Third-party notices** | `NOTICE`, `THIRD-PARTY-NOTICES.md` (made by `scripts/gen-notices.sh`) | Done for Rust crates; fonts and rule packs unresolved (see below) |
| 7 | **Copyright registration (optional)** | Copyright is automatic in Uganda under the Copyright and Neighbouring Rights Act, 2006, with no registration needed. The Act allows a voluntary record of a work at URSB, which helps as evidence of ownership and date. | Not done. Consider registering the source and the manual at URSB |
| 8 | **Confidentiality for alpha testers** | `docs/legal/TESTER-NDA-TEMPLATE.md` | Draft template |
| 9 | **Code signing** as tamper protection | Authenticode (Windows) and Apple Developer ID plus notarisation (macOS); sign the Linux archive checksum | Not done; Gate 2 of the roadmap |

## Also worth doing

* Keep dated records of authorship (git history in the private repository is good evidence). Make sure every contributor and contractor has signed an assignment of copyright to AfroBIM, since in Uganda the author is first owner unless there is a written assignment or employment terms.
* Publish SHA-256 checksums beside each installer.
* Remove `build1.log` and `testbuild.log` from the repository.

## Needs the owner or a lawyer

See the end of the final report on the branch, and `docs/plan/2026-11-25-release-licence-audit.md`: Transport font licence, Uganda rule-pack permission, OGL confirmation for the UK pack, tile-provider terms (Esri), the EULA's governing law and liability limits, and the legal entity details.
