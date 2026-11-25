# AfroBIM: launch and product roadmap (2026-11-25)

Owner: K. Venture (PMP). Prepared with Claude. This plan joins the product roadmap (what the software does) to a launch project (how people get it, use it, pay for it and tell us about it), using the PMI process groups so it can be run and controlled like any other project.

## 1. Project charter

**Purpose.** Put AfroBIM in the hands of road and infrastructure engineers as a downloadable desktop application, learn from real use, and turn that into sales.

**Objectives (to be confirmed by the owner):**

| # | Objective | Measure | Target date |
|---|---|---|---|
| O1 | A signed installer anyone can download from afrobim.com | Windows and macOS installers open without security warnings | Gate 2 |
| O2 | Real users try it | 50 private alpha testers, then 500 public beta downloads | Gate 2, Gate 3 |
| O3 | We know how it is used | Downloads, weekly active users, crash-free sessions, feedback count, all on one dashboard | Gate 2 |
| O4 | First revenue | First 10 paying seats | Gate 4 |

**Success criteria.** Each gate below is passed only when its criteria are met, not on a date.

**Constraints.** Source code stays private. One owner with AI agents as the delivery team. Unsigned builds are blocked by macOS Gatekeeper and Windows SmartScreen, so signing is on the critical path.

**Assumptions.** afrobim.com (the live site, repository `arihov/afrobim`) can host a download page. The owner can open Apple Developer and Windows code-signing accounts.

**Stakeholders.** Owner (sponsor and product manager); testers (Ugandan consultants, contractors, students); clients and partners (MoWT, UNRA, KCCA, AIL); payment and hosting providers.

## 2. How the download works: private code, public downloads

GitHub cannot make one repository partly public. The standard answer is two repositories:

| Piece | What it holds | Visibility |
|---|---|---|
| `arihov/afrobim-platform` | All source code | Private (stays as it is) |
| `arihov/afrobim-releases` (new) | Only installers attached to GitHub Releases, a changelog, a public issue tracker and Discussions for feedback | Public |
| afrobim.com/download (new page on the existing site) | The download page users see: latest version, what is new, system needs, buttons per platform | Public |

Flow: CI in the private repository builds and signs the installers, then publishes them as a Release in `afrobim-releases`. The download page reads the latest release and links to its files, so the page never needs editing for a new version. The app checks the same feed to offer updates.

Why GitHub Releases for the files: free, fast worldwide, and every file carries a download count readable through the GitHub API. If AfroBIM later needs its own domain for files, Cloudflare R2 behind `download.afrobim.com` is the next step (low storage cost, no egress fees).

## 3. How we measure: downloads, usage and feedback

| What | How | Privacy |
|---|---|---|
| Downloads | GitHub Releases download count per file, pulled daily into the dashboard | No personal data |
| Page visits and where people come from | Privacy-friendly analytics on afrobim.com (for example Plausible or Vercel Web Analytics) | No cookies needed |
| Active users and which tools they use | In-app usage report, **off unless the user agrees** on first start: anonymous install id, version, OS, which commands were used (counts only, never drawings or file names) sent to an AfroBIM endpoint | Opt-in, stated in a privacy notice |
| Crashes | The recovery system already catches a crash; with consent, send the crash summary (no document content) | Opt-in |
| Feedback | "Send feedback" in the Help menu (text, optional screenshot, optional email), stored on the endpoint and forwarded to the owner; public bugs in `afrobim-releases` Issues; ideas in Discussions | User chooses what to send |
| Satisfaction | One-question in-app survey after 7 days of use (score 0 to 10 and a comment) | Opt-in |

The endpoint: a small serverless API (for example on Vercel, where the owner's other sites already run) with a managed Postgres database (for example Supabase or Neon). The dashboard is one page reading that database plus the GitHub counts.

## 4. How we sell

- **Model.** Free public beta, then a free tier (viewing, exchange, learning) and paid tiers (design modules, 4D/5D, culvert and structures detailing at LOD 400). Pricing to be tested with beta users.
- **Licence keys and payments.** A merchant-of-record service (for example Lemon Squeezy or Paddle) issues licence keys, takes cards and handles VAT. Check which pays out to a Ugandan business and whether mobile money is needed (for example Flutterwave or Pesapal) before choosing.
- **Audience, in order.** Ugandan and East African road consultants and contractors; MoWT, UNRA and KCCA engineers; universities (free education licences build the next generation of users); then the wider African market and Gulf partners (AIL).
- **Channels.** LinkedIn posts with short videos of real tasks (the culvert from a sample line in two minutes); YouTube tutorials; webinars with training colleges (the MTTC Green BIM webinar format); case studies from AfroBIM's own projects (ACP, Kafunjo, Iganga-Bulopa-Kamuli); the afrobim.com Academy.

## 5. Stage gates (the launch project)

| Gate | Name | Entry criteria (all must hold) |
|---|---|---|
| G0 | Branch merged | Pull request opened, CI green on Linux, Windows and macOS, branch merged to main |
| G1 | Release-ready build | Licence audit of everything inside the installer (fonts, manuals, packs, NOTICE inside the app); hands-on smoke test of every new tool (MAPCAPTURE, MAPEXPORT, GREENBIM, GISIMPORT, DWG, recovery, culvert); manual up to date; project names removed from defaults |
| G2 | Private alpha (50 testers) | Signed Windows and macOS installers; `afrobim-releases` and the download page live; opt-in usage, crash and feedback reporting working; dashboard live; privacy notice and terms published |
| G3 | Public beta | Alpha feedback triaged; top crash causes fixed; tutorials for the five main workflows; announcement plan ready |
| G4 | Version 1.0 and sales | Licence keys and payments working; pricing chosen; support process (response times, where to ask) defined |

## 6. Work breakdown structure

1. **Project management**: charter sign-off, this plan, weekly status, risk register, decision log.
2. **Release engineering**: CI on all platforms; installers (MSI, DMG, Linux archive); code signing and notarisation; publishing to `afrobim-releases`; in-app update check.
3. **Legal and compliance**: licence choice for AfroBIM; third-party notices in the app; fonts and manuals decision; privacy notice; terms of use; tile-provider terms for a commercial product.
4. **Feedback and analytics**: endpoint and database; opt-in consent screen; usage, crash and feedback reporting; dashboard.
5. **Web and marketing**: download page; release notes; tutorials and videos; case studies; launch posts.
6. **Commercial**: pricing; licence keys; payments; education licences; support.
7. **Product** (section 7).

## 7. Product roadmap

### 7.1 Scope statement: what AfroBIM is and is not

AfroBIM is one application for road and infrastructure BIM: survey, terrain, alignments, corridors, drainage, structures such as culverts, signs and markings, coordination, 4D and 5D, and Green BIM, with drawings and fabrication detail from the same model.

**Out of scope:** copying Civil 3D, Revit and Navisworks feature for feature. Each of those is the work of hundreds of engineers over decades. Clients such as AIL also require those products' native files by contract (the Al Noud 2 Employer's Requirements name Revit, Civil 3D and Navisworks 2022). So AfroBIM wins by doing the road engineer's whole job in one place and exchanging cleanly with those tools: IFC 4.3 in and out, DWG and DXF in and out (built in now), BCF issues, LandXML, and federation of other people's models for clash and 4D.

**The Navisworks connection, honestly.** AfroBIM already federates IFC reference models, runs clash tests, manages BCF issues and plays 4D. That covers what a road engineer opens Navisworks for. Writing Navisworks' own NWC/NWD files is not possible without Autodesk's SDK; NWC export from AfroBIM is not planned. Navisworks users take AfroBIM models through IFC, and issues travel both ways through BCF.

### 7.2 Level of detail

LOD is a property of each element, not of the software. AfroBIM will publish an LOD matrix (BIMForum LOD Specification) per element type and carry what each level needs:

- **LOD 300**: correct size, shape, location and orientation (most AfroBIM elements today).
- **LOD 400**: fabrication and installation detail: reinforcement with bar schedules, joints, connections, sizes from manufacturers' catalogues. First targets: culverts (bar schedule derived from the Km 39+136 drawing, 85 of 85 cutting lengths reproduced), sign supports and foundations, drainage chambers, gantries.
- **LOD 500**: field-verified as-built. It cannot come from a desk. AfroBIM's part is the tooling: survey and point-cloud comparison against the design, an as-built status per element, and a verification report.
- A **model audit report** (elements, LOD reached, missing properties, clashes, file health) that doubles as a client deliverable, as the AIL plan proposes.

### 7.3 Tracks and horizons

| Track | Now (Dec 2026) | Next (Jan to Feb 2027) | Later (Mar to Jun 2027) |
|---|---|---|---|
| Launch | G0 and G1: PR, CI, licence audit, smoke tests, manual | G2: signing, releases repo, download page, telemetry, feedback, dashboard | G3 public beta, G4 sales |
| Culverts and structures (LOD 400) | Culvert engine (running) with the exact bar schedule rules | Culvert window: from a sample line, a drainage schedule or by hand; drawing sheets; DWG out | Box culverts detailing, 3D and IFC with reinforcement, other structures |
| 2D drafting core | Design note | LINE, PLINE, ARC, CIRCLE, RECTANGLE, HATCH, OFFSET, TRIM, EXTEND, FILLET, ROTATE, SCALE, MIRROR, ARRAY, BLOCK, MATCHPROP | Paper space layouts, external references |
| Interface | Design review against the AfroBIM design system | Modern ribbon: denser, smaller icons with labels, every tool visible per tab, better workspace switcher, light and dark themes | Customisable ribbon and tool palettes |
| Coordination (the Navisworks part) | Already built: federation, clash, BCF, 4D | Clash grouping and status workflow, viewpoints, model comparison | Timeliner-style 4D with resources |
| 5D | Already built: bill, rates, estimate, cash flow, certificates | Link culvert and drafting elements to the bill | Cost of design options with Green BIM side by side |
| Green BIM | Built, fixes merged | Lighting energy and grid factor in the sheet set | Building energy (9.16) |
| Parametric content and assets | Parametric culverts | Family-style parametric objects (headwalls, chambers, gantries) with a parameter editor | Online asset library: download parametric objects into a project |
| Visualisation | | Rendered cross-sections and 3D with materials | Photo-real renders and flythroughs |

### 7.4 Advice on 2D drafting

Yes, add a core drafting set. BIM does not remove 2D: every submission still ends as sheets, and engineers reach for a polyline, a hatch or a quick offset many times a day. Without them users leave for AutoCAD and may not come back. Keep drafting as a first-class element family that exports to DWG and DXF, stays out of IFC quantities unless tagged, and can be converted into model objects (a polyline becomes a feature line, a drain path or an alignment).

## 8. Risk register (top items)

| # | Risk | P | I | Response |
|---|---|---|---|---|
| R1 | Installers blocked by Gatekeeper and SmartScreen | High | High | Buy signing certificates before G2; budget about US$100 per year (Apple) and US$200 to 400 per year (Windows) |
| R2 | A licence problem in a bundled font, manual or library | Medium | High | Licence audit at G1; ship NOTICE in the app |
| R3 | Tile provider terms forbid commercial or saved use | Medium | Medium | Default to providers that allow it; keyed providers for the rest |
| R4 | Scope creep from "do everything Civil 3D, Revit and Navisworks do" | High | High | Hold the scope statement in 7.1; every new request goes through the change log |
| R5 | Untested on Windows | High | High | CI on Windows at G0; one Windows tester in the alpha |
| R6 | Personal data in telemetry | Low | High | Opt-in only, no content or file names, privacy notice, deletion on request |
| R7 | Payment provider does not support Uganda | Medium | Medium | Check before G4; mobile-money fallback |

## 9. Monitoring and control

A weekly status note (done, next, blocked, risks) and a dashboard with: downloads by platform and version; weekly active installs; crash-free session rate; feedback received and closed; tool usage top 20; open issues; gate progress. Change requests go into a decision log with impact on time, cost and scope before they are accepted.

## 10. Owner decisions (recorded 2026-11-25)

| # | Question | Decision |
|---|---|---|
| 1 | Public releases repository | Yes. `arihov/afrobim-releases` created (public, Issues and Discussions on). Source stays private. |
| 2 | Download page | Drafted as a static page in this repository (`web/download/`), to be copied into afrobim.com. |
| 3 | Tile providers in a commercial product | Safer route: defaults only from providers whose terms allow commercial use; non-commercial providers marked "Evaluation only"; MAPCAPTURE refused where saving is not allowed. |
| 4 | Design standards | Ugandan first: MoWT Road Design Manual (Geometric Design 2010 and companion volumes). Design vehicles DV-1 to DV-5 per the Geometric Design Manual. Other standards only where the Ugandan manual is silent. |
| 5 | Licence and protection | Proprietary licence for the source, EULA for the binaries, CC BY-ND 4.0 for public documents, trademark notice, full third-party notices in the app. Legal texts to be reviewed by a Ugandan advocate. |
| 6 | Carbon factor source | ICE database (Circular Ecology), A1 to A3, A4 where a haul is entered; EPDs and client figures override. Subject to the ICE redistribution terms. |
| 7 | Code signing | Costs below; owner to choose. |

### 10.1 Code signing costs (checked 2026-11-25)

| Platform | Option | Cost | Notes |
|---|---|---|---|
| macOS | Apple Developer Program | US$99 per year | Needed for Developer ID signing and notarisation. Individual or organisation (organisation needs a D-U-N-S number). |
| Windows | Azure Artifact Signing (formerly Trusted Signing), Basic | US$9.99 per month (about US$120 per year), 5,000 signatures | Public trust certificates only for organisations in the USA, Canada, EU and UK, and individuals in the USA and Canada. A Ugandan company is not eligible today. |
| Windows | OV code signing certificate (Sectigo, SSL.com and others) | About US$220 to 380 per year, plus cloud key storage (for example SSL.com eSigner, from about US$20 per month) | Keys must sit in a hardware token or cloud HSM since June 2023. Validity now capped at about 15 months. Available to a registered Ugandan business. |
| Windows | EV code signing certificate | About US$300 to 600 per year | Not needed any more for SmartScreen reputation; OV is enough. |

Suggested budget for Gate 2: about US$100 (Apple) plus US$450 to 600 (Windows OV with cloud signing), so roughly **US$550 to 700 per year**.

### 10.2 Still open

1. Code-signing purchase and who holds the accounts.
2. Bundle the Transport fonts in public installers or not.
3. Written permission from MoWT for the Uganda traffic signs rule pack.
4. Km 39+136 steel: 1.329 t on the drawing against 1,498 kg in the BBS.
