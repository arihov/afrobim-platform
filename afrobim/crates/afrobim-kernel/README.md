# afrobim-kernel

The document every module and front end shares: elements, layers, selection, view, drafting aids,
the canvas controller and undo. The kernel knows nothing about signs, markings or roads.

This README covers the parts added for project-wide settings and the speed band rule.

## Project settings (document level)

A setting that belongs to the whole document, not to one element, is stored in one element of
category `project_settings` (layer `Project`), whose `data` is a JSON object with one key per
setting. This follows the pattern the sheets crate uses for `ProjectInfo` (one `project_info`
element). The kernel had no earlier store, so this is a minimal one.

```rust
use afrobim_kernel::{project_setting, set_project_setting, Document};
let mut doc = Document::default();
set_project_setting(&mut doc, "my_key", serde_json::json!(42));
assert_eq!(project_setting(&doc, "my_key"), Some(serde_json::json!(42)));
```

The element is added the first time a setting is written, other keys are kept, and it is saved
with the document. It draws nothing. A front end should register `ProjectSettingsModule` so the
element does not report "no module installed", and may hide the category from its element lists
as the app does for `project_info`.

## Speed band rule (`speed_band`)

UK requirements are stated in mph bands ("50 mph or more"). Uganda posts km/h limits, and
80 km/h is 49.71 mph. The project decides once how a limit meets a threshold; the engines are not
changed. The app calls `resolve` for each check and passes the result to the engine.

Conversion: 1 international mile = 1.609344 km exactly. That is a definition, so mph = km/h / 1.609344.

### The rule, stored as a project setting

`SpeedBandRule` is serde, stored under the key `speed_band_rule` of the project settings:

| Rule | Meaning |
|------|---------|
| `RoundUp { tolerance }` | The project default. See below. |
| `Exact` | Use the exact converted value. |
| `AskEachTime` | Return the exact value and an `Ask`; the caller asks the user (see `Resolved::answered`). A question is returned only where the default rule would have rounded up. |
| `Table(Vec<(km/h limit, mph to use)>)` | Look the km/h limit up in the user's table. An unlisted limit, or a limit given in mph, uses the exact value and is marked `Basis::TableMiss`. |

`speed_band_rule(&doc)` returns the stored rule, or `RoundUp` with `ConversionRounding` when none
is stored or the stored value cannot be read. `set_speed_band_rule(&mut doc, &rule)` stores it.

### RoundUp, exactly

Let `v` be the exact converted value in mph and `t` the smallest threshold that is greater than
or equal to `v`.

* If `v == t`, `t` is used (nothing is rounded).
* If `v < t` and `t - v` is within the tolerance, `t` is used: the requirement of the band at `t` applies.
* Otherwise the exact value `v` is used.

The tolerance is a field of the rule, `RoundUpTolerance`:

* `ConversionRounding` (the default): `t - v <= 0.5` mph. This is the case where `v` rounds to `t` at the nearest whole mph (halves round up). So 80 km/h (49.71 mph) is treated as 50, and 70 km/h (43.50 mph) is not treated as 50. It applies to a limit given in km/h only, because it is about conversion; a limit already in mph is used exactly.
* `Mph(x)`: `t - v <= x` mph, a wider band the user chooses. Applies to either unit.
* `NextBandUp`: any `v < t` uses `t`, always the next band up. Applies to either unit. A value above every threshold has no band above it and is used exactly.

The tolerance is not invented here: 0.5 mph is the definition of "rounds to the threshold at the
nearest whole mph", and anything wider is the user's choice.

### Calling it

The thresholds are never built in. The calling engine supplies the ascending mph thresholds of
its own check.

```rust
use afrobim_kernel::speed_band::{resolve, speed_band_rule, SpeedUnit};

// `doc` is the project document; `limit` is the posted limit in km/h.
let rule = speed_band_rule(&doc);
let r = resolve(80.0, SpeedUnit::Kmh, &[30.0, 40.0, 50.0, 60.0], &rule);
assert_eq!(r.mph_used, 50.0);
assert_eq!(r.band_note, "80 km/h = 49.71 mph; project rule 'round up to the next band' treats it as 50 mph");
// Pass r.mph_used to the engine; print r.band_note in the calculation.
```

`Resolved` holds `mph_used`, `exact_mph`, `band_note` (the sentence for the calculation),
`basis` (`Exact`, `RoundedUp`, `Table`, `TableMiss`, `NeedsAnswer`) and `ask`. With `AskEachTime`,
when `r.ask` is `Some`, put the choice to the user, then call `r.answered(use_band)`; the result
carries the chosen value and a note saying the user chose it.

### Limits

* The thresholds, and which checks use which, belong to the engines and are not checked here. An unsorted list is sorted.
* Only the two units km/h and mph are known.
* The table matches a km/h limit to within 0.000001 km/h.
* The rule decides how a limit meets a threshold. Whether a requirement should apply at that threshold is the standard's question, not this module's.
