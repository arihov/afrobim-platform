# Culvert reference: single 1 x 1200 mm CPC at Km 39+136

`km39136-1x1200-cpc-bbs.csv` is the full bar bending schedule transcribed from the owner's drawing (pages 5 and 6 of "Details CPC 1x1200mm Km 39+136"): 85 rows, total 1,498.176 kg, which matches the drawing. Every weight equals total number x cutting length x BS 4449 nominal mass (Y8 0.395, Y10 0.617, Y12 0.888 kg/m).

## How the drawing's spreadsheet works out cutting lengths

Derived from the schedule itself and checked on all 85 rows (85 of 85 reproduce). The bend radius is r = 2d (BS 8666 minimum for B500 bars up to 16 mm), and the result is rounded to the nearest 10 mm with halves rounded up.

| Drawing code | BS 8666 shape | Cutting length |
|---|---|---|
| 100 | 00 (straight) | A |
| 111 | 11 (L) | A + B - 0.5r - d |
| 113 | cranked end | A + C |
| 117 | half round | A |
| 200 | 21 (U) | A + B + C - r - 2d |
| 202 | U with one angled leg | A + B + C - 0.5r - d |
| 213 | three straights | A + B + E |
| 500 | 51 (closed link with hooks) | 2(A + B) + 2C - 2.5r - 5d |
| 900 | four-bend crank | A + B + C + D + E - 4(0.5r + d) |

## Known difference on the same drawing

The payable items table on page 3 lists 1.329 t of steel; the schedule totals 1,498.176 kg.

### Check of the steel figure (2026-11-25)

Neither figure looks right. The apron rows (01a to 01l, 02, 03a to 03k) use **4** members, but the structure has **2** aprons (one at the inlet, one at the outlet), and the drawing callouts confirm 2:

| Mark | Drawing callout (per apron) | Bars per apron | Correct total (2 aprons) | Schedule total |
|---|---|---|---|---|
| 02 | 9Y12-02-200 U-Bars T&B | 9 x 2 layers = 18 | 36 | 72 |
| 01a to 01l | 12Y12-01(a-l), two splay zones, T&B | 1 x 2 zones x 2 layers = 4 per mark | 8 per mark | 16 per mark |
| 03a to 03k | 11Y12-03(a-k) T&B | 2 per mark | 4 per mark | 8 per mark |

Every other element uses the right member count: 4 wing walls, 2 headwalls, 2 toe beams. The concrete in the payable table (C25 end structures 9.646 m3) also matches 2 aprons: each is a trapezoid 1,850 mm wide at the headwall, 6,470 mm at the wing tips and 2,310 mm long, 200 mm thick, so about 1.92 m3 each.

A density check agrees. A 200 mm slab with Y12 at 200 mm both ways, top and bottom, carries about 17.8 kg/m2. Two aprons of about 9.6 m2 each come to about 340 kg, or about 400 kg with the U-bar legs. That matches **half** the schedule's apron steel (792.380 / 2 = 396.190 kg), not all of it.

Corrected schedule total: 1,498.176 - 396.190 = **1,101.986 kg (about 1.10 t)**, roughly 114 kg/m3 of C25 concrete, which is normal for culvert end structures. The payable 1.329 t is therefore about 0.23 t too high, and the schedule 0.40 t too high.

A smaller point to check: the headwall callout "6Y8-11(a-f)" appears on both sides of the pipe, which suggests 2 per mark per headwall (4 in total), while the schedule has 2. That would add 4.107 kg.

The reference CSV keeps the drawing exactly as issued, so the golden test still totals 1,498.176 kg. The engine should generate 2 aprons.

The engine (`afrobim-culvert`) follows the schedule, not the payable table, and reproduces all 85 rows (see `afrobim/crates/afrobim-culvert/README.md`).
