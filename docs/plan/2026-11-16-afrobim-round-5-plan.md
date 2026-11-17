# AfroBIM round 5 plan: civil core, coordinate systems, interface finish

Date: 2026-11-16. Owner: AfroBIM. Status: in progress.

## Aim

Make the civil design objects and the coordinate system the core of the application, in the way
Civil 3D organises a drawing, and give the interface a finished look.

## Defaults taken (changeable by the owner)

| Matter | Default |
|---|---|
| Coordinate handling | As Civil 3D: one project coordinate system; objects sit at their source coordinates; nothing is reprojected or shifted on import. A file in another system is reported, not silently moved |
| Projection maths | WGS 84 / UTM is held. Any other system (for example Arc 1960 / UTM) gets projection maths only if its parameters are fetched from the EPSG registry and cited; otherwise it is carried as a named system |
| Zoom | No fixed limits in 2D or 3D; only guards against zero, infinite and not-a-number values |
| Icons | An open-licence icon font bundled in the binary |
| Glass theme | Translucent tinted panels and windows, rounded corners, soft shadows. Live background blur is not available in the egui glow renderer and is not promised |
| Workspaces | The ribbon shows the tabs of the chosen module; a switcher changes it |

## Wave 5A: engines (parallel, each in its own worktree, no application edits)

| ID | Work package | Crate |
|---|---|---|
| E1 | Coordinate system core, project setting, unlimited zoom, zoom to selection | afrobim-kernel |
| E2 | Surfaces: Delaunay build, breaklines, boundaries, contours, analysis, volume between surfaces | new afrobim-terrain |
| E3 | LandXML complete: CoordinateSystem, CgPoints, Parcels, ProfSurf, CrossSects, PlanFeatures, surface source data; read and write | afrobim-alignment, afrobim-corridor (landxml.rs), afrobim-design (landxml.rs) |
| E4 | Assemblies and subassemblies as objects, sample lines, section views, material volumes by shape | afrobim-corridor |
| E5 | COGO points, point groups, description keys, parcels, inverse and traverse | new afrobim-cogo |

## Wave 5B: application (in sequence)

| ID | Work package |
|---|---|
| U1 | Interface foundation: theme, icon font, dialog kit, menus, workspaces (runs beside wave 5A) |
| U2 | Coordinate system wired through: project setting, status bar, LandXML at true coordinates, IFC map conversion, zoom |
| U3 | Civil objects live: toolspace tree, commands and dialogs for points, surfaces, alignments, profiles, sample lines, corridors, assemblies, quantities, parcels; IFC mapping |
| U4 | Every remaining dialog moved to the dialog kit |
| U5 | User manual updated |

## Not in this round

Pipe networks, grading objects, intersections and roundabout wizards, survey database and
traverse adjustment, point clouds, data shortcuts, reprojection between systems, code signing.
