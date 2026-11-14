//! LandXML 1.2 export of an alignment with its profile and station equations.
//!
//! Element and attribute names were checked against the schema retrieved from
//! <http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd> and the output validated against
//! it with `xmllint --schema` (see the README for the command). What is written:
//!
//! - root `LandXML` with the required `date`, `time`, `version="1.2"`, the namespace, and
//!   `Units/Metric` with every required unit attribute (metres; the schema requires them);
//! - `Application` naming this crate;
//! - `Alignments/Alignment` with the required `name`, `length`, `staStart`;
//! - `CoordGeom` with `Line` (`Start`, `End`), `Curve` (`Start`, `Center`, `End`, `PI`, `rot`,
//!   `radius`, `length`, `delta`) and `Spiral` (`Start`, `PI`, `End`, `rot`, `spiType="clothoid"`,
//!   `length`, `radiusStart`, `radiusEnd`, with `INF` for the tangent end). The schema requires a
//!   `PI` in a `Spiral`: it is the intersection of the tangents at the two ends. Points are text
//!   `northing easting` (northing first, as the schema's point type has it);
//! - `StaEquation` (`staInternal`, `staAhead`, `staBack`, `staIncrement`) for each station equation;
//! - `Profile/ProfAlign` with `PVI`, `ParaCurve@length`, `UnsymParaCurve@lengthIn/lengthOut` and
//!   `CircCurve@radius/length`, each with text `station elevation`.
//!
//! Numbers are written with Rust's shortest representation that reads back to the same `f64`.
//!
//! Not written: superelevation, existing-ground profiles (`ProfSurf`), cross-sections, the
//! redundant direction attributes (`dir`, `dirStart`, `dirEnd`: they depend on bearing
//! conventions and the importer ignores them). **Direction and `staStart` of each element** are
//! left out; consumers recompute them from the points, as the `afrobim-alignment` importer does.

use afrobim_alignment::{Alignment, AlignmentElement, ElementKind, StationDirection, VerticalCurve};

/// What goes in the file header.
#[derive(Debug, Clone, PartialEq)]
pub struct LandXmlOptions {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// `HH:MM:SS`.
    pub time: String,
    /// Name of the project for the `Project` element; not written when empty.
    pub project: String,
}

impl Default for LandXmlOptions {
    fn default() -> Self {
        let (date, time) = now_utc();
        LandXmlOptions { date, time, project: String::new() }
    }
}

fn now_utc() -> (String, String) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (format!("{y:04}-{m:02}-{d:02}"), format!("{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn pt(p: (f64, f64)) -> String {
    format!("{} {}", p.1, p.0)
}

/// Intersection of the tangent at the start and at the end of an element (the LandXML `PI`).
fn tangent_intersection(e: &AlignmentElement) -> (f64, f64) {
    let (x1, y1, h1) = e.end();
    let (x0, y0, h0) = (e.x, e.y, e.heading_rad);
    let (d0, d1) = ((h0.cos(), h0.sin()), (h1.cos(), h1.sin()));
    let cross = d0.0 * d1.1 - d0.1 * d1.0;
    if cross.abs() < 1e-12 {
        return ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    }
    // x0 + t d0 = x1 + u d1
    let (dx, dy) = (x1 - x0, y1 - y0);
    let t = (dx * d1.1 - dy * d1.0) / cross;
    (x0 + t * d0.0, y0 + t * d0.1)
}

/// The alignment as a LandXML 1.2 document.
pub fn write_landxml(alignment: &Alignment, options: &LandXmlOptions) -> String {
    let mut o = String::new();
    o.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    o.push_str(&format!(
        "<LandXML xmlns=\"http://www.landxml.org/schema/LandXML-1.2\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"http://www.landxml.org/schema/LandXML-1.2 http://www.landxml.org/schema/LandXML-1.2/LandXML-1.2.xsd\" date=\"{}\" time=\"{}\" version=\"1.2\" language=\"English\">\n",
        esc(&options.date),
        esc(&options.time)
    ));
    o.push_str("  <Units>\n    <Metric areaUnit=\"squareMeter\" linearUnit=\"meter\" volumeUnit=\"cubicMeter\" temperatureUnit=\"celsius\" pressureUnit=\"milliBars\" angularUnit=\"radians\" directionUnit=\"radians\"/>\n  </Units>\n");
    if !options.project.is_empty() {
        o.push_str(&format!("  <Project name=\"{}\"/>\n", esc(&options.project)));
    }
    o.push_str("  <Application name=\"AfroBIM afrobim-design\" desc=\"Concept alignment design\"/>\n");
    o.push_str("  <Alignments>\n");
    o.push_str(&format!("    <Alignment name=\"{}\" length=\"{}\" staStart=\"{}\">\n", esc(&alignment.name), alignment.length(), alignment.start_station_m));
    o.push_str("      <CoordGeom>\n");
    for e in &alignment.elements {
        let (x1, y1, h1) = e.end();
        let start = (e.x, e.y);
        let end = (x1, y1);
        match e.kind() {
            ElementKind::Line => {
                o.push_str(&format!("        <Line length=\"{}\">\n          <Start>{}</Start>\n          <End>{}</End>\n        </Line>\n", e.length, pt(start), pt(end)));
            }
            ElementKind::Arc => {
                let k = e.curvature_start;
                let r = 1.0 / k.abs();
                let centre = (e.x - e.heading_rad.sin() / k, e.y + e.heading_rad.cos() / k);
                let rot = if k > 0.0 { "ccw" } else { "cw" };
                let delta = (h1 - e.heading_rad).abs();
                let pi = tangent_intersection(e);
                o.push_str(&format!(
                    "        <Curve rot=\"{rot}\" radius=\"{r}\" length=\"{}\" delta=\"{delta}\">\n          <Start>{}</Start>\n          <Center>{}</Center>\n          <End>{}</End>\n          <PI>{}</PI>\n        </Curve>\n",
                    e.length,
                    pt(start),
                    pt(centre),
                    pt(end),
                    pt(pi)
                ));
            }
            ElementKind::Spiral => {
                let rad = |k: f64| if k == 0.0 { "INF".to_string() } else { format!("{}", 1.0 / k.abs()) };
                let sum = e.curvature_start + e.curvature_end;
                let rot = if sum > 0.0 { "ccw" } else { "cw" };
                o.push_str(&format!(
                    "        <Spiral length=\"{}\" radiusStart=\"{}\" radiusEnd=\"{}\" rot=\"{rot}\" spiType=\"clothoid\">\n          <Start>{}</Start>\n          <PI>{}</PI>\n          <End>{}</End>\n        </Spiral>\n",
                    e.length,
                    rad(e.curvature_start),
                    rad(e.curvature_end),
                    pt(start),
                    pt(tangent_intersection(e)),
                    pt(end)
                ));
            }
        }
    }
    o.push_str("      </CoordGeom>\n");
    for q in &alignment.station_equations.equations {
        let dir = match q.direction {
            StationDirection::Increasing => "increasing",
            StationDirection::Decreasing => "decreasing",
        };
        let back = q.back_m.map_or(String::new(), |b| format!(" staBack=\"{b}\""));
        o.push_str(&format!("      <StaEquation staAhead=\"{}\"{back} staInternal=\"{}\" staIncrement=\"{dir}\"/>\n", q.ahead_m, q.internal_m));
    }
    if let Some(p) = &alignment.profile {
        o.push_str(&format!("      <Profile name=\"{}\">\n        <ProfAlign name=\"{}\">\n", esc(&p.name), esc(&p.name)));
        for (i, v) in p.pvis.iter().enumerate() {
            let text = format!("{} {}", v.station_m, v.elevation_m);
            match v.curve {
                VerticalCurve::None => o.push_str(&format!("          <PVI>{text}</PVI>\n")),
                VerticalCurve::Parabola { length_m } => o.push_str(&format!("          <ParaCurve length=\"{length_m}\">{text}</ParaCurve>\n")),
                VerticalCurve::AsymmetricParabola { length_in_m, length_out_m } => {
                    o.push_str(&format!("          <UnsymParaCurve lengthIn=\"{length_in_m}\" lengthOut=\"{length_out_m}\">{text}</UnsymParaCurve>\n"))
                }
                VerticalCurve::Circular { radius_m } => {
                    // The arc length of the circle between the tangent points.
                    let (g1, g2) = (p.grade_between(i - 1).unwrap_or(0.0), p.grade_between(i).unwrap_or(0.0));
                    let arc = radius_m * ((g2.atan() - g1.atan()).abs());
                    o.push_str(&format!("          <CircCurve length=\"{arc}\" radius=\"{radius_m}\">{text}</CircCurve>\n"));
                }
            }
        }
        o.push_str("        </ProfAlign>\n      </Profile>\n");
    }
    o.push_str("    </Alignment>\n  </Alignments>\n</LandXML>\n");
    o
}
