# Uganda standards in AfroBIM

Status: the values below were read by an automated reader from a third-party hosted copy of the manual. They are **not reviewed by an engineer**. Check every number against the Ministry's printed copy before relying on it. All units are metric.

## 1. Standards hierarchy

1. **Primary: Uganda Ministry of Works and Transport (MoWT) Road Design Manual (2010 series).** The default basis for new projects (pack id `ug-mowt-2010`, file `afrobim/crates/afrobim-defaults/data/ug-mowt-2010.json`). Design vehicles are DV-1 to DV-5 of Volume 1, Table 5-1.
2. **Secondary, only where the Ugandan manual is silent:**
   - UK DMRB (pack `dmrb`, still selectable per project): link, junction, roundabout and drainage detail not given in the Ugandan manuals.
   - SATCC / SADC road design guidelines and South African TRH or SANRAL methods: regional practice for drainage and pavement items.
   - AASHTO Green Book: vehicle turning paths and sight distance background (the Ugandan DV table matches the AASHTO P, SU, BUS and WB figures).
   - TRL Overseas Road Notes (for example ORN 31 pavements, ORN 6 geometry, ORN 9 drainage): tropical low-volume road practice.
3. A value taken from a secondary reference must say so in its `source` field and in the drawing or report that uses it.

### MoWT Road Design Manual volumes

| Volume | Title | Date | Where found |
|---|---|---|---|
| 1 | Geometric Design Manual | January 2010 | Third-party copy read: https://kupdf.net/download/uganda-road-design-manuals-volume-1-geometric-design-manual_59132946dc0d60167d959eae_pdf . Other copies listed by search: https://idoc.pub/documents/uganda-road-design-manuals-volume-1-geometric-design-manual-d4pq30x6ewnp , https://www.scribd.com/document/399624711/Geometric-Design-Manual-Uganda |
| 2 | Drainage Design Manual | 2010 | Cited by a JICA report (https://openjicareport.jica.go.jp/pdf/12380069.pdf) as Volume 2; a copy was NOT FOUND and not read |
| 3 Part I | Pavement Design, Flexible Pavements | 2010 | Cited as Volume 3 Part I in the same JICA report; copy NOT FOUND and not read |
| Bridge Design Manual | | 2010 | NOT FOUND |
| Road Safety, Traffic Signs Manual Vol 1 and 2 | | July 2004 | Volume 1 copy on Scribd (https://www.scribd.com/doc/85575094/Traffic-Signs-Manual-Vol-1); not read |
| Urban Roads Design Manual | July 2023 | Lists "Dimensions of Design Vehicles" as Table 2.25 (https://www.masakacity.go.ug/sites/default/files/2025-04/1745400996584_6808af0ef3d47_3.pdf); not read, so the owner should compare it with Table 5-1 for urban work |

The Ministry's official page for its manuals is on https://works.go.ug ("Manuals for Road & Bridge Works"); the search could not open a direct link to any 2010 volume there.

## 2. Design vehicles, Volume 1, Section 5.1, Table 5-1 (printed page 49)

The manual states that the five design vehicles of Table 5-1 are used for geometric design "until a major change in the vehicle fleet is observed", and that the largest design vehicle likely to use a facility with considerable frequency governs junction radii and horizontal curve radii.

| Symbol | Type | Height (m) | Width (m) | Overall length (m) | Front overhang (m) | Rear overhang (m) | Wheelbase (m) | Min. design turning radius, outer (m) | Min. inside radius (m) |
|---|---|---|---|---|---|---|---|---|---|
| DV-1 | 4x4 passenger car | 1.3 | 2.1 | 5.8 | 0.9 | 1.5 | 3.4 | 7.3 | 4.2 |
| DV-2 | Single unit truck | 4.1 | 2.6 | 9.1 | 1.2 | 1.8 | 6.1 | 12.8 | 8.5 |
| DV-3 | Single unit bus | 4.1 | 2.6 | 12.1 | 2.1 | 2.4 | 7.6 | 12.8 | 7.4 |
| DV-4 | Semitrailer combination | 4.1 | 2.6 | 16.7 | 0.9 | 0.6 | 6.1 and 9.1 | 13.7 | 5.8 |
| DV-5 | Large (interstate) semitrailer | 4.1 | 2.6 | 21.0 | 1.2 | 0.9 | 6.1 and 12.8 | 13.7 | 2.9 |

Source for every cell: MoWT Road Design Manual, Volume 1: Geometric Design Manual (January 2010), Table 5-1, p49, in the third-party copy at the kupdf.net URL above. Turning-path figures 5-1 to 5-5 follow on pp50 to 52.

Checks made: for every vehicle, overall length equals front overhang plus wheelbase(s) plus rear overhang. The DV-1, DV-2, DV-3 numbers also match the AASHTO P, SU and BUS-12 design vehicles.

Reading notes:
- The extracted text has the type names in a header line, not beside each row. The pairing above follows the order of the five names and the dimensions.
- "6.1 and 9.1" and "6.1 and 12.8" are read as tractor and trailer wheelbases.
- The code stores the vehicles under keys `vehicle.*` in the Ugandan pack and shows them in the junction dialog (Kerb returns section), DV-5 as the largest.

## 3. Geometric values held in the Ugandan pack

All from Volume 1, Section 4, Tables 4-2a, 4-2b and 4-3 to 4-9 (printed pages 44 to 48), same third-party copy. Each entry carries a `source` field.

- Design speed, desirable and absolute maximum gradient by class (Ia, Ib, II, III, A, B, C) and terrain (Flat, Rolling, Mountainous, Urban): keys `link.design_speed.<class>`, `link.gradient_desirable_max.<class>`, `link.gradient_absolute_max.<class>`.
- Lane width, carriageway width, shoulder width, roadway width, right of way by class: keys `xs.*` (Table 4-2b).
- By design speed (40 to 120 km/h): stopping sight distance (`link.ssd.min`), minimum radius at 7 % superelevation (`link.horizontal.min_radius_superelevation_7`), crest K for stopping (40 to 90 km/h), sag K (40 to 120 km/h).
- Maximum superelevation 7 % rural, 4 % urban; normal crossfall 2.5 % paved, 4 % gravel; shoulder crossfall 4 %; minimum gradient in cut 0.5 %.

Inconsistencies in the copy read, left unresolved and flagged in the pack notes:
- Urban stopping sight distance at 50 km/h is printed as 58 m in Tables 4-4 and 4-5 and 60 m elsewhere. The pack stores 60 m.
- Crest K for stopping at 100 km/h is 80 in Table 4-3 and 62 in Table 4-4, so values from 100 km/h upward are not stored.
- Passing-sight crest K is 310 at 80 km/h and 307 at 90 km/h (not increasing), so passing K is not stored.
- Tables 4-3 and 4-4 are garbled in the text read; class Ia and Ib values rest on Table 4-2a and a summary table that repeats them.

## 4. NOT FOUND items for the owner to fill from the printed manual

1. Which road classes each of DV-1 to DV-5 governs: Section 5.1 gives no class-to-vehicle table, only the rule of the largest vehicle using the facility with considerable frequency. NOT FOUND.
2. Kerb-to-kerb (swept width) turning figures per vehicle: the manual gives outer minimum turning radius and minimum inside radius. Figures 5-1 to 5-5 also print maximum radii (for example 7.8 m, 13.4 m, 14.1 m, 14.0 m) that could not be paired reliably with a vehicle in the text read. NOT FOUND.
3. Official page numbers of Tables 4-3 to 4-9 individually (stored as the range 44 to 48), and the PDF page index (stored as 0).
4. Class Ia and Ib per-terrain parameters other than design speed and gradient (SSD and K for 100 km/h and above, radius at each terrain), verify against Tables 4-3 and 4-4.
5. Passing sight distance and passing K values (see inconsistency above).
6. Drainage Design Manual (Volume 2, 2010): rainfall return periods, runoff method, culvert minimum diameter and cover, velocity limits. Not read. NOT FOUND.
7. Pavement Design Manual (Volume 3 Part I, flexible, 2010): traffic classes, subgrade classes, design catalogue. Not read. NOT FOUND.
8. Bridge Design Manual (2010): loading, clearances, freeboard. Not read. NOT FOUND.
9. Traffic Signs Manual Volumes 1 and 2 (2004): sign sizes, sign placement and road marking dimensions. Not read.
10. Urban Roads Design Manual (2023) Table 2.25 design vehicle dimensions: not read; compare with Table 5-1.
11. Licence or reuse terms for the manuals: not stated in the copy read.
