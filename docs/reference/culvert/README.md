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

The payable items table on page 3 lists 1.329 t of steel; the schedule totals 1,498.176 kg. Which one is right is for the owner to confirm.

The engine (`afrobim-culvert`) follows the schedule, not the payable table, and reproduces all 85 rows (see `afrobim/crates/afrobim-culvert/README.md`).
