# afrobim-survey

A survey database as an engine (no user interface), modelled on the objects of the Autodesk Civil 3D
survey database as described in its help: survey database settings, equipment records, networks with
control points, setups and observations, levelling runs, GNSS baselines, figures with a figure prefix
table, import events and a change log. On top of it: reductions with a stated correction chain, radiation,
import of field books and raw files, traverse closure (reusing `afrobim-cogo`), and a least squares network
adjustment with quality measures.

Metric, f64, decimal degrees internally, vertical angles stored as zenith angles. Coordinates are kept
exactly as given and tested at true UTM magnitude (northing 9 960 000 m). Other units enter only through
explicit input factors (`DISTFACTOR`, `distance_factor_m`, `feet_to_m`). No value from memory is presented
as a standard: **instrument standard errors, centring errors, tolerances, the refraction coefficient, the
earth radius, the atmospheric ppm, the prism constant, the grid scale factor, the confidence level, the
significance level, the convergence criterion and the blunder threshold are all inputs.** Nothing has a
default value (`AdjustOptions::new` takes every number; `Equipment::exact` is all zeros and only for
noise-free simulations).

Dependencies (all MIT or Apache-2.0, pure Rust, no C libraries): `serde`, `serde_json` (with its
`float_roundtrip` feature, so that coordinates survive a JSON round trip to the last bit), `thiserror`, and
the workspace crate `afrobim-cogo`. Linear algebra (Cholesky) is written in `linalg.rs`; SHA-256 in
`hash.rs`; the distributions in `stats.rs`.

## Database file

`SurveyDatabase::to_json` / `from_json`: one self-contained JSON file with a format tag
(`"afrobim-survey-db"`) and a version (1). Other tags and newer versions are refused. It holds settings
(distance unit label, angle unit, angle direction, vertical angle type, coordinate system as free text and
an optional EPSG code, and the corrections each switched on or off with its parameters: prism constant,
atmospheric ppm, curvature and refraction with earth radius and k, sea level reduction with radius, grid
scale factor), equipment, networks, import events (file name, SHA-256 of the text, the time **given by the
caller**, the format and options used, what was added, warnings, optionally the source text), the figure
prefix table, the linework code set and the change log. The crate reads no clock.

Import events can be removed (`remove_import_event`: everything the event added goes) or reprocessed
(`reprocess_import_event`: remove and read again from the kept source text, optionally with other reader
options; the old state is restored if the new import fails). Duplicate point names follow an explicit
policy (error, skip, replace).

## Reductions (`reduce.rs`), in this order

1. prism constant (slope distances); 2. atmospheric ppm (slope distances); 3. face reduction (face 2
readings brought to face 1, rows averaged, spread reported as range and sample standard deviation, face
difference and index error reported when both faces exist; equipment collimation and index error applied
only to rows not balanced by the other face); 4. slope to horizontal and vertical component; 5. curvature
and refraction on the height difference, `(1 - k) D^2 / (2 R)`; 6. sea level reduction `D R / (R + h)` with
h the mean height of the line; 7. grid scale factor. Derivations are in the module comment. The refraction
coefficient is defined as k = R/r (Baselga, Garcia-Asenjo and Garrigues, 2014, Practical formulas for the
refraction coefficient, J. Surv. Eng. 140(2), eq. 1, read from the open copy
http://hdl.handle.net/10251/61536); its value is yours. First order in D/R; no grid convergence.

`compute_points` computes non control points by radiation in order of dependency (a station can be a
point computed from an earlier setup), averages repeated results and reports their spread and the
differences from given control.

## Formats read

* **This crate's field book** (`fieldbook.rs`, documented in the module): keyworded lines `UNITS`,
  `CONTROL`, `POINT`, `SETUP`, `OBS`, `DIRECTION`, `LEVEL`/`LEG`, `BASELINE`, `FIGURE`/`VERTEX`.
  Strict parser, errors carry the line number (`line 12: ...`). `write_fieldbook` writes it back with full
  precision.
* **Autodesk Civil 3D field book (.fbk), a subset** (`fbk.rs`). Source: Autodesk Civil 3D 2027 help pages
  (About Point Creation Commands, About Point Location Commands, observation by angle and distance,
  Face1/Face2 observation, new instrument setup, About Figure Commands, To Continue / End a Survey Figure),
  retrieved 2026-11-16 through the Autodesk Product Help search. Implemented: `NE`, `NE SS`, `NEZ`, `AZ`,
  `STN`/`STA`, `BS`, `PRISM`, `AD [VA|VD]`, `F1`/`F2 [VA|VD]`, `BEG`, `CONT`, `PT`, `CLOSE`, `END`. Autodesk's
  support site states that no specification document for the .fbk format exists, only the help pages,
  and no real field book could be obtained, so this is a reader for simple field books checked against the
  examples printed in the help, not a complete FBK reader. Other commands are errors (strict) or warnings.
* **Leica GSI-8 and GSI-16** (`gsi.rs`). Source: the format description of the Total Open Station
  documentation (https://totalopenstation.readthedocs.io/en/stable/input_formats/if_leica_gsi.html: word
  layout, word indexes, unit codes, sample lines) and the Autodesk Map 3D help page "Leica GSI formats"
  (sample lines). No Leica document was available. Word indexes 11, 21, 22, 31, 32, 33, 41, 81 to 88 are
  read; the others are ignored and counted in a warning. A GSI file does not say which target is the
  backsight or which face a reading is on: the reader options choose the backsight rule.
* **Topcon GTS-7, structure only** (`topcon.rs`). Source: Maptek Vulcan help, Survey Appendix C, TOPCON
  GTS-7 (record types and field order). The page does not give the angle encoding or the UNITS values, so
  these are options, and no real file was available: tested only on records built from the field list.
* **Delimited observation tables** (`csv.rs`) with a column mapping by index or by header name.

**Not obtained, not implemented:** Trimble JobXML (the search found only third party remarks, no schema
or sample; the schema is in the header of every JobXML file, which was not available); the official Leica
GSI specification; the semantics of Topcon GTS-7 beyond the field list; Sokkia SDR, Nikon, Trimble DC and
other raw formats; the remaining Civil 3D survey commands (ZD, BD, DD, STADIA, offsets, curves CRV/XC, HI,
auto point numbering). The tutorial line `FC1 VA 105 ...` printed in one Autodesk help page is not
recognised: the command is not documented in any page that could be read.

## Adjustment (`adjust.rs`, `level.rs`, `lsq.rs`)

Delivered: **2D** (plan) network adjustment of directions (one orientation unknown per setup, the
backsight contributing its reading; angles at a station are contained in the readings), distances, known
azimuths with a stated sigma, GNSS baseline components (dE, dN with stated sigmas, no correlations), and
control coordinates with stated sigmas (weighted control); **1D** levelling adjustment with the same
quality measures (height differences of levelling runs, baseline dU, weighted control heights). **Not
delivered: 3D adjustment**, trigonometric height differences from total stations in the adjustment, free
network adjustment with minimal constraints or inner constraints (a network without enough fixed
components is reported as singular), correlated observations, baselines with full covariance, variance
component estimation, grid convergence.

* Estimation: weighted Gauss-Newton iteration, N dx = r with N = A'WA, stopping when `dx' N dx <=
  convergence` (an input; a value near 1e-10 is reachable, much below the rounding floor of about 1e-12 is not)
  or at `max_iterations` (then `converged` is false). Teunissen, *Network Quality Control*, TU Delft OPEN
  Publishing 2024, doi 10.59490/tb.100 (open access, read), sections 2.4.2 and 2.4.3 (Figure 2.7) and
  2.7 (Q_x = (A'WA)^-1). The observation equations are derived in the comments of `adjust.rs`.
* Weights: from the equipment standard errors. Direction: `(sp^2 + sr^2)/n + (ei^2 + et^2)/D^2`; distance:
  `(a^2 + (bD)^2)/n + ei^2 + et^2`; levelling leg: `a^2 + b^2 L_km`. The centring terms are the standard
  propagation of independent horizontal centring errors, derived in the comments, included by option;
  they are systematic over the rows of a setup so are not divided by n. These formulas are not a regulation.
* Outputs: adjusted coordinates, residuals (adjusted minus observed), standard errors of residuals,
  redundancy numbers, standardised residuals w_i = v_i / sd(v_i) (Teunissen eq. 3.49, diagonal case),
  a posteriori variance factor v'Pv / dof, degrees of freedom, orientation unknowns.
* Global test of the variance factor: T = v'Pv against chi-square with dof degrees of freedom (Teunissen
  eq. 3.43, 3.44), reported two sided at alpha/2 and 1 - alpha/2 and one sided at 1 - alpha. The critical
  values are **computed**, never tabulated: `stats::chi2_quantile` inverts the chi-square distribution
  (regularised incomplete gamma function: series DLMF 8.7.1, continued fraction DLMF 8.9.2 evaluated with the
  modified Lentz method after Press et al., *Numerical Recipes*; ln gamma by the Stirling series of DLMF
  5.11 after shifting). Tested against closed forms (even degrees of freedom) and against numerical integration
  of the density (odd degrees of freedom).
* Blunder flagging: an observation is flagged when `|w| > blunder_threshold` (an input; the helper
  `normal_two_sided_critical(alpha)` computes the value for a significance level). `adjust_with_snooping`
  removes the largest flagged observation, adjusts again, and repeats (Baarda data snooping,
  Teunissen section 3.4.2). Observations with redundancy number below 1e-6 are uncontrolled: no w, no flag.
  Minimal detectable biases are not computed.
* Error ellipses: eigen-decomposition of the 2x2 covariance, major axis azimuth from `tan 2theta = 2 s_en /
  (s_n^2 - s_e^2)` (MIT 12.215 lecture 14), semi-axes `k sqrt(lambda)`. With the a priori variance factor 1
  `k = sqrt(-2 ln(1 - p))`, derived from the chi-square (2 degrees of freedom) distribution function
  `1 - exp(-t/2)`; with covariances scaled by the a posteriori factor `k = sqrt(r ((1 - p)^(-2/r) - 1))`,
  from the F(2, r) distribution function `1 - (1 + 2f/r)^(-r/2)`. Both derivations are in `stats.rs` and the
  closed forms are tested.
* Relative precision between two points (`Adjustment::relative_precision`): covariance of the difference
  vector, standard error of distance and azimuth by propagation, ratio distance / sigma, relative ellipse.
* `report::adjustment_report` and `level_report` give the plain text report.

`traverse_from_setups` defines a traverse from ordered stations and the setups at them (distances from
either direction, bearings chained from the turned angles, optional angular check against a known bearing)
and closes it with `afrobim-cogo` (`Traverse::misclosure`, Bowditch and transit). Least squares adjustment
of the same observations is `adjust_network_2d`.

## Figures (`figures.rs`)

A small linework code set modelled on the Civil 3D help pages "About Special Codes" and "About Curve
Segment Codes": description `NAME [CODE] [NAME CODE ...]` with codes begin `B`, continue `C`, end `E`,
close `CLS`, begin curve `BC`, point on curve `OC`, end curve `EC` (all configurable; `/` starts a
comment). A name alone continues the active figure, or begins one when `auto_begin_on_prefix` is on and
the name matches the prefix table. The figure prefix table maps a name prefix (longest wins) to a style
name, a layer name and a breakline flag. `Figure` keeps vertices and a closed flag; `flatten_figure`
makes a 3D polyline (`Breakline`) for surfaces, with three point arcs flattened to a chord tolerance you give.
Not supported: horizontal and vertical offset codes, right turn, rectangle, recall and connect point, circle,
curves through more than three points (kept as chords with a warning).

`to_cogo_points` / `into_store` export points to `afrobim_cogo::CogoPoint` and `PointStore`
(whole number names become point numbers, others get numbers from an input start).

## Published examples reproduced, and what could not be

Reproduced in the tests:
* Teunissen (2024), section 4.2.4, the free levelling network 1-2-3-4: the overall model test statistic
  (eq. 4.41) and the five w statistics (eq. 4.42), with the equalities w1 = w3 and w4 = w5 stated in the
  text. The printed w formulas contain `h41` where the network has `h42` (misprint, deduced from the
  printed loop conditions; the combinations match).
* Autodesk Civil 3D help, "To Create a Survey Observation Using a Face1 or Face2 Angle": the printed
  point 101 and 102 coordinates are reproduced from the printed azimuths and distances (the radiation
  step only; the step from the field angles to the printed azimuths depends on context the page does not
  give and is not reproduced).
* GSI sample lines of the two documentation pages above parse to the printed values.
* Closed forms: error ellipse of a two distance fix, the rectangle traverse with a 0.02 m error (Bowditch
  and transit corrections), chi-square functions, SHA-256 test vectors (FIPS 180-4).

**No published numerical example of a full 2D network adjustment with coordinates, residuals and
ellipses could be fetched** (the open pages tried returned errors or were not readable as text). The 2D
adjustment is therefore tested against simulated networks with known truth: a noise free network returns
the true coordinates at northing 9 960 000 m to better than 1e-6 m; with errors drawn from exactly the
stated standard errors, v'Pv/dof averages near 1 and the global test passes in the expected share of
runs; a gross error in a distance or in a direction produces the largest |w| on exactly that observation
and snooping removes it and recovers the truth.

## Limits

* Plane grid; no datum, projection, grid convergence or geoid. Azimuths are grid azimuths.
* Corrections are first order; the horizontal angle is not corrected for refraction or deflection of the
  vertical; the zenith angle is taken as measured at the instrument. Sea level uses the height used in the
  coordinates.
* No 3D adjustment; no free network; uncorrelated observations only; the weights follow the stated
  standard errors, so a global test failure means the stated errors or the model are wrong, or there is a
  blunder, nothing more.
* A setup needs a backsight point with coordinates, a known direction, or an azimuth; an azimuth only
  backsight enters the adjustment as an orientation observation with the standard error of one reading.
* Radiation depends on processing order (stations computed from earlier setups are not recomputed after
  later averaging). Adjust for a consistent solution.
* Reading files: whole text in memory; GSI 3 digit word indexes are not read; vertical angles in GSI are
  taken as zenith unless an option says otherwise.
* Import ids and the hash cover the text as given; the time stamp is whatever the caller passes.
