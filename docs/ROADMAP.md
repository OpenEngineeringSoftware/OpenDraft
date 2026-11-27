# Roadmap

How OpenDraft gets from "a capable early project" to "actually, that's quite an impressive project". This is the plan for trust, reach and depth, and the next steps of feature work. Everything known to be missing is in `docs/parking_lot.md`; this file decides what to do first and why.

Written 2026-10-31. Update it when something is done or the plan changes: tick the box, add the date and the PR.

## Who it is for, in order

1. **Open-source and Linux users.** Often leaving a closed tool or an ageing one. They try it, report bugs and spread it, so they get the first impression.
2. **Hobbyists and makers.** Laser cutting, CNC, 3D-print sketches, woodwork, floor plans. They need clean files going out as much as in.
3. **Small-scale professionals.** Drafters and small firms who need AutoCAD-compatible files and depth. This comes last; it needs the trust built by the first two.

## What sets it apart (keep these sharp)

- Reads **DWG** (AutoCAD 2010 to 2018) as well as DXF, free. The free 2D alternatives are weak or paid here.
- An **AutoCAD-style command line** with the usual names and aliases, and prompts with options.
- One **small native executable**; **scriptable by an AI assistant** through MCP with every change undoable.

## How to read this

- **Size:** S is up to a day, M is a few days, L is weeks.
- **Why** says which audience it serves and what it earns.
- Each phase has an **exit test**: how we know it is done, not just that the boxes are ticked.
- Phases are ordered by what earns trust soonest. Within a phase, items can go in any order unless stated. Phases 1 and 2 are the priority; later phases overlap with normal development.

---

## Phase 1: Linux as a first-class platform

*Why: the first audience runs Linux, and until now it had been run on one machine (Linux Mint).*

**Status (2026-10-31): done as far as it can be shown without real machines.** What is built, and what was found, is in `docs/linux.md`; the remainder needs people with the hardware.

- [x] **Test matrix (M), on virtual displays.** CI builds once on Ubuntu 22.04 and runs that program on Ubuntu 22.04 and 24.04, Debian 12, Fedora, Arch and openSUSE Tumbleweed (X11), on Ubuntu 24.04, Fedora and Arch under Wayland (a headless sway), and at scale 1.5 and 2 (`.github/workflows/linux.yml`). `docs/linux.md` records what it does and does not show.
- [ ] **Test on real machines (M, needs you).** Real GPUs, GNOME (mutter) and KDE (KWin) on X11 and Wayland, the clipboard, drag and drop, real file dialogs and input methods, and Wayland keyboard and mouse input (a headless compositor has no input devices). Reports from users are the way to cover this; say what was tried in `docs/linux.md`.
- [x] **GUI smoke test in CI (M).** `scripts/linux_smoke.py` starts the real program, opens a drawing, draws a line with the keyboard and mouse, undoes it, runs `opendraft-mcp --check` against it, quits with Ctrl+Q, and also starts without a drawing and without `XDG_RUNTIME_DIR`. It reads the screen and the program's state from outside, and leaves screenshots as artifacts.
- [x] **Release artifacts tested (S).** The release workflow already installed the .deb and ran the AppImage on a bare Ubuntu 22.04; the .deb now also passes the whole smoke test.
- [x] **Flatpak (M), built in CI.** `installer/linux/flatpak/` builds offline from `Cargo.lock`, has its files checked and starts under a virtual display, and `installer/linux/flatpak/flathub/` is the submission for release 0.1.4 (built in CI the way Flathub will). [ ] **Flathub (paused, 2026-10-31).** Flathub's rules (`AGENTS.md` and the submission template in `flathub/flathub`) do not allow AI-assisted packaging or submissions, and the manifest, scripts, metadata and CI here were written with AI help. Nothing has been submitted. Revisit by reading their generative AI policy, then either disclose honestly and submit by hand, or write the packaging again by hand. Their template also asks for a video and evidence of development history and real-world use, which a young project may not yet have.
- [x] **Arch package (S), built in CI.** `installer/linux/arch/PKGBUILD` builds, installs and links in an Arch container, with the checksum of the 0.1.4 source. [ ] **Publish it to the AUR (waiting).** New AUR registrations were closed on 2026-10-31 (a wave of automated sign-ups; no date given). Do not script retries; watch the Arch news feed and `aur-general`. Check the AUR's own policy on AI-assisted packages first. Debian/Ubuntu, Fedora and nixpkgs as people ask.
- [x] **Fix what the matrix finds.** Found and fixed: the program would not open at all where the graphics setup offered no multisampled configuration (Fedora, Arch, Ubuntu 24.04 and Tumbleweed under software rendering; Ubuntu 22.04 and Debian 12 were not affected); Open and Save As did nothing when there was no file dialog backend (the status bar now says what to install); the assistant connection needed `XDG_RUNTIME_DIR` (a private cache folder is used without it). Noticed and not changed: a drawing opened from DXF counts as unsaved at once, so closing it asks to save. That looks deliberate (it has no `.odraft` file yet) but is worth a look.

**Not covered, and why:** opening a *DWG* and *saving* were part of the original exit test. CI opens a DXF, because the DWG sample drawings are private and cannot be committed; and saving needs a file dialog or an automation grant that only the user can give, so a headless test cannot do it. A small hand-made DWG fixture would close the first; the second would need a test-only way to grant permission, which would weaken the rule that only the user grants write access, so it waits for a better idea.

**Exit test:** on each tested distribution the program starts and draws, takes keyboard and mouse input (X11), opens a drawing, answers the assistant connection and quits, and the CI smoke test guards that. Real-machine coverage is the open item above.

## Phase 2: Trust in files

*Why: "opens your drawings" has to be provable. All audiences.*

- [ ] **Fuzz the readers (M).** `cargo-fuzz` targets for the DWG, DXF and `.odraft` readers; a short run in CI and a longer run on a schedule. Fix what it finds.
- [ ] **Golden-image tests (M).** `opendraft-canvas::preview` renders a document to pixels without egui. Snapshot drawings that cover hatches, dimensions, dashed linetypes, text and blocks, so rendering regressions are caught by a test and not by eye.
- [ ] **Interoperability checks (M).** Open OpenDraft's DXF in LibreCAD, QCAD and FreeCAD, and in an independent DXF validator; open it in AutoCAD if it can be had (the dimension DXF has never been opened there). Keep the results and any sample files in the repo; fix what breaks.
- [ ] **Sample drawings pack (S).** Original or CC0 drawings for tests and for new users. (The AutoCAD sample drawings in `private_material/` must never be committed.)
- [ ] **Benchmarks in CI (M).** Run the existing benches against stored baselines so speed claims stay true.
- [ ] **Property tests for geometry (M).** Offset and back, trim and extend, fillet invariants, in `opendraft-core`.

**Exit test:** the readers survive a fixed amount of fuzzing, a documented set of drawings round-trips through other free CAD tools, and a rendering regression fails CI.

## Phase 3: What makers need

*Why: audience 2. Files going out matter as much as files coming in.*

**Correction (2026-10-31):** this phase first said PDF and SVG plotting were still to build. They are built (`PLOT` writes a layout to PDF or SVG at 1:1 in millimetres; `opendraft_core::plot` and `opendraft_files::plot`, checked in two PDF readers and a browser). What is missing is *proof on real fabrication and printing workflows*, and a few gaps listed in `docs/parking_lot.md` ("Layouts and plotting").

- [ ] **Prove the plots on real workflows (M, needs real software and a printer).** Open OpenDraft's SVG in Inkscape and in the laser and cutter software people use (LightBurn, Glowforge, Silhouette, Cricut and similar); print the PDF at 100% and measure it with a ruler. Write down what was tried in the repository, and fix what breaks.
- [x] **Plot gaps (M), done 03-11-2026 (PRs #105 and #108).** Margins, plot area (Layout, Limits, Extents, Window), plot scale, offset and centring, rotation, wide polylines (filled ribbons) and viewport borders are built and tested in `opendraft_core::plot`/`plot_plan`, and set in the Plot dialog and Page Setup. Still refused, with a clear message: Display and named View areas (no persistent view state). Paper output through the Windows and CUPS backends is built; see the next item for what is still unproven.
- [ ] **Layers to cut settings (S to M).** Laser software reads colour or layer as "cut", "engrave" and "score". Check that SVG and DXF carry layer names and colours in a way those tools use, and document the recipe.
- [ ] **Clean DXF for fabrication (M).** Correct units, closed polylines, splines handled sensibly. OVERKILL and JOIN already help; add a checklist test of a typical laser-cutting file.
- [ ] **DXF gaps (M).** Anonymous blocks on import are built (24-11-2026, not yet checked with a real AutoCAD file; see "Checks still owed"); a DXF's dynamic blocks come in as dynamic blocks for linear parameters with stretch and move actions (24-11-2026, MI3); the rest is named in an import note and kept as a picture. TEXT import and block export are built (checked 21-11-2026).
- [ ] **Templates and a first-hour tutorial (S).** A metric sheet, A4 and A3, a laser-cutting template, and a ten-command walkthrough.
- [ ] **Hatch patterns from `.pat` files, and linetypes from `.lin` files (M).** Check what loads today (there is a stock linetype library) before scoping.

**Exit test:** a drawing made in OpenDraft cuts correctly on a laser cutter or prints at 1:1 from the PDF, with no hand-fixing, and the repository says which software was tried.

## Phase 4: Community and project health

*Why: the main risk to a one-maintainer project is that it stays one maintainer.*

- [ ] **Contributor path (S).** Issue templates already exist; add "good first issue" labels and the "how to add a command" recipe (now only in `CLAUDE.md`) as a section in `CONTRIBUTING.md`.
- [ ] **A short demo video and a plain project page (S).** Include the MCP assistant drawing, with every change undoable and reviewable.
- [ ] **Readable architecture docs (M).** Split `ARCHITECTURE.md` into short pages by topic; its paragraphs are accurate but very long.
- [ ] **Dependency advisories (S).** `cargo deny check advisories` fails on `paste`, `rustybuzz` and `tokio`, all transitive. Fix them or record why each is accepted in `deny.toml`.
- [ ] **A place to talk (S).** GitHub Discussions is enough.
- [ ] **A regular release rhythm and a public statement of what is not supported yet (S).** An honest "not yet" list builds trust.
- [ ] **Windows code signing (M).** Unsigned downloads put off exactly the people we want; also winget.
- [ ] **A second maintainer (ongoing).** Not a task; the aim of the items above.

**Exit test:** someone who is not the author has made a merged contribution, and a new user can install, learn and report a bug without help.

## Phase 5: Declare `.odraft` stable

*Why: people will not keep real work in a format that "changes between builds".*

- [ ] Follow "Declaring .odraft stable" in `CONTRIBUTING.md`, set `CURRENT_VERSION` to 1 as a deliberate release step, and from then on keep a conversion for every older version. Never as part of a feature.
- [ ] Publish the schema (there is already `docs/odraft-schema-audit.md`) and a round-trip test corpus.

**Exit test:** a file saved by the first stable release opens in every later one.

## Phase 6: Depth for small professionals

*Why: audience 3. Only after phases 1 to 5, and in this rough order.*

- [ ] Text: TEXT/MTEXT in all import and export paths (`ByBlock` is done, 31-10-2026).
- [x] Block attributes, done 01-11-2026 (PRs #100 and #101). What is left (multiline attributes, `ATTDISP`, BATTMAN, more data fields) is in `docs/parking_lot.md`.
- [ ] DWG export (L).
- [x] Associative arrays as a record beside ordinary objects, with ARRAYEDIT, done 26-11-2026 (PR #275). The array as one object (grips, a contextual tab) is in `docs/parking_lot.md`.
- [x] Spline editing: trim, break and join (26-11-2026); extend, join with other kinds, offset, fillet, Lengthen for splines and PEDIT's vertex Tangent (27-11-2026, in the pull request that took the modify-command items off the parking lot).
- [ ] Dimension depth: other arrowheads, styles with colours and linetypes, `DIMBREAK`, jogged radius and `ARC_DIMENSION` as real DXF, annotative scales.
- [ ] Multileader depth (spline leaders, block content, MLEADEREDIT).
- [ ] A plugin system for third-party commands; an AutoLISP-compatible subset is a stretch idea, not a promise.
- [ ] External references (xrefs).

Dynamic blocks come last of these, and are built in stages that show a user nothing until the baseline is whole (see "Next steps").

## Next steps (feature work)

What to build next, roughly in order, beside the phases above. The details of each are in `docs/parking_lot.md`; take an item off this list when it is done.

1. The custom coordinate system (in progress, 22-11-2026): the system is built; next, turning the view (PLAN and a gesture). Plan in `docs/parking_lot.md`.
2. The commands still dimmed on the Home tab (`NOT_BUILT` in `opendraft-app/src/ribbon_def/home.rs`): Groups, Table, Radius, Angle and ID Point, Isolate, Layer States and the rest.
3. Associative arrays as one object, with grips.
4. Annotation: the MTEXT toolbar's font menu and symbols, then its ruler (waits for AutoCAD paragraph samples); the dimensions that are left, starting with DWG dimension import; leaders and multileaders.
5. OD Roman: the PDF hidden text layer, the DXF round trip, and the glyphs still to draw.
6. Layouts: non-rectangular viewport clips (planned in `docs/parking_lot.md`); open the R2000 export in AutoCAD.
7. Properties panel parity with AutoCAD (`private_material/properties_gap.md`, cheap rows first).
8. Dynamic blocks, in stages (plan in `private_material/dynamic_blocks_specification.md`, milestones M0 to M10): M0, the data slots (ids on a definition's objects, a schema on a definition, values on an insert), is built (24-11-2026), and so is M1, evaluating a linear parameter with Move and Stretch and its end grip (core only; 24-11-2026). M2, saving schemas, values and object ids in `.odraft`, is built too (24-11-2026). M3, authoring in the Block Editor (`BPARAMETER`, `BACTION`, `BACTIONSET`, `RESETBLOCK`, BSAVE carrying the schema, a schema overlay), is built too (24-11-2026). M4, the Properties rows for the values, the diamond grip, the kept pictures for a selected insert and tests that picking, snapping and plotting follow the values, is built too (24-11-2026). M5, writing dynamic blocks to DXF as their current pictures with a note (and the first timing), is built too (24-11-2026), which completes the baseline. MI3, building imported AutoCAD dynamic blocks (linear parameter with stretch and move) from the decoded values, is built too (24-11-2026). M6, the rest of the parameter and action vocabulary, has begun: the rotation parameter with the rotate action is built (24-11-2026), with import of both. The flip parameter with the flip action is built too (24-11-2026). The scale action is built too (24-11-2026). The polar parameter, with the actions that follow it, is built too (24-11-2026). XY and point parameters are built too (24-11-2026). Left in M6: a linear parameter's start grip and Midpoint base behaviour. M7 has begun: the visibility parameter with its named states is built (24-11-2026), with Block Editor commands, a Properties drop-down, the native file and DXF import. The lookup parameter with its table, `BLOOKUPTABLE`, the reverse lookup and the Properties drop-down are built too (24-11-2026). M7 is closed apart from: the visibility and lookup grips and menus, a table dialog, lookup inputs of other kinds, and lookups that set attributes. Nothing shows in the app until the authoring commands, grips and Properties rows (M3 and M4) and DXF export (M5) are in.
9. Blocks for daily work (planned 27-11-2026 from issue #260; plan in `docs/parking_lot.md`, "Content Manager"), in order: (rename a block of the drawing, and change its unit: built by Brice, #278 and #279, 27-11-2026); (double-click in "This drawing" inserts, and ask about a taken name only when the definitions differ: built, 27-11-2026); the `.oblock` file (one block, opens in the block editor only) with WBLOCK (built, 27-11-2026: the file, WBLOCK, the Block Editor tab and the DXF export and import; left: the import window's switch and wiring the DXF import into the Content Manager); (the library moved to one SQLite file: built, 27-11-2026); a docked Blocks palette; batch import and export; linked AutoCAD folders and an insertion layer per category.
10. Housekeeping: make the Document mutators return `Result`, a progress bar for large files.

## Not now

3D, a cloud or collaboration layer, macOS (unless a contributor takes it on), and any feature that would be only half built. A visible "not yet" is better than a feature that misleads.

## Principles for choosing

- Reliability and evidence before breadth: a claim is made only once a test or a check backs it.
- Prefer small steps that can be reviewed and released.
- Linux, then Windows, then the rest; AutoCAD behaviour and wording unless there is a reason not to.
- Review the plan every few releases; drop items that no user asks for.

## Related files

- `docs/parking_lot.md`: everything known to be missing or left for later, by area.
- `docs/ARCHITECTURE.md`: how what is built works, and the rules new code follows.
- `docs/history.md`: the archived development diary (was `docs/ui_plan.md`), kept for the reasoning behind decisions made up to 21-11-2026.
- `docs/code_review_findings.md`: the engineering review and its current status. Only OD-007 (modal flags) is open.
