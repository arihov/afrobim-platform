//! A document with at least one element of every road module, built through each module's own
//! API on one alignment with a profile: road markings, a side drain, two pipe culverts (one with
//! two barrels), a portal and a cantilever gantry with a sign hung on the portal, a street
//! lighting installation and a single column, a barrier run with terminals and a transition, a
//! pavement, a ground surface and a corridor.
//!
//! Every dimension here is a test value, not an engineering value.

use std::collections::BTreeMap;
use std::f64::consts::FRAC_PI_2;

use afrobim_alignment::{Alignment, AlignmentBuilder, Profile, Pvi, Turn};
use afrobim_corridor::{CorridorDefinition, Daylight, Link, Slope, Template, Tin};
use afrobim_drainage::{PipeCulvertDefinition, SideDrainDefinition};
use afrobim_gantry::{Direction, GantryDefinition, SignPosition};
use afrobim_ifc::IfcElementInput;
use afrobim_kernel::{Document, ElementId, Generated, ModuleRegistry, Pt};
use afrobim_lighting::{Arrangement, ColumnSpec, Installation, Luminaire, Side, SingleColumn};
use afrobim_markings::{LineType, MarkingDefinition};
use afrobim_pavement::{Layer, Material, PavementDefinition};
use afrobim_restraint::{
    BarrierRun, ContainmentLevel, EndTreatment, ImpactSeverityLevel, TerminalClass, TrafficFace, Transition, WorkingWidthClass,
};
use serde_json::json;

pub struct RoadFixture {
    pub document: Document,
    pub alignment: Alignment,
    pub generated: BTreeMap<ElementId, Generated>,
}

impl RoadFixture {
    pub fn id(&self, name: &str) -> ElementId {
        self.document.elements().iter().find(|e| e.name == name).unwrap_or_else(|| panic!("no element {name}")).id
    }

    /// Every element with what its module generated for it, as the application hands them over.
    pub fn inputs(&self) -> Vec<IfcElementInput<'_>> {
        self.document.elements().iter().map(|element| IfcElementInput { element, generated: self.generated.get(&element.id) }).collect()
    }

    /// Every element with no generated output at all: the exporter runs the modules itself.
    pub fn bare_inputs(&self) -> Vec<IfcElementInput<'_>> {
        self.document.elements().iter().map(|element| IfcElementInput { element, generated: None }).collect()
    }
}

fn registry() -> ModuleRegistry {
    let mut r = ModuleRegistry::default();
    r.register(Box::new(afrobim_markings::MarkingModule));
    r.register(Box::new(afrobim_drainage::SideDrainModule::default()));
    r.register(Box::new(afrobim_drainage::PipeCulvertModule::default()));
    r.register(Box::new(afrobim_gantry::GantryModule));
    r.register(Box::new(afrobim_lighting::LightingModule));
    r.register(Box::new(afrobim_restraint::RestraintModule));
    r.register(Box::new(afrobim_pavement::PavementModule::default()));
    r.register(Box::new(afrobim_corridor::SurfaceModule::default()));
    r.register(Box::new(afrobim_corridor::CorridorModule));
    r
}

fn spec() -> ColumnSpec {
    ColumnSpec {
        column_type: "test column (text)".into(),
        mounting_height_m: 10.0,
        bracket_outreach_m: 1.5,
        tilt_deg: 5.0,
        luminaire: Luminaire {
            product: "Test luminaire".into(),
            lumen_output_lm: 12_000.0,
            power_w: 90.0,
            photometric_file: Some("test.ies".into()),
            luminous_intensity_class: Some("G4".into()),
            cct_k: Some(4000.0),
            cri: Some(70.0),
            led: Some(true),
            risk_group: Some("RG0".into()),
            mass_kg: Some(6.5),
            windage_m2: Some(0.1),
        },
    }
}

/// 200 m north from (1000, 5000), 150 m of right-hand curve of radius 300 m, then 250 m straight.
/// Stations 0 to 600, with a profile rising from 50 m to 53 m.
pub fn alignment() -> Alignment {
    let mut a = AlignmentBuilder::new("MC10", 0.0, 1000.0, 5000.0, FRAC_PI_2).line(200.0).arc(300.0, 150.0, Turn::Right).line(250.0).build().unwrap();
    a.profile = Some(Profile::new("FG", vec![Pvi::point(0.0, 50.0), Pvi::point(600.0, 53.0)]).unwrap());
    a
}

pub fn road_fixture() -> RoadFixture {
    let a = alignment();
    let mut doc = Document::default();
    doc.standard = "UK-TSM-2019".to_string();
    let alignment = doc.add(afrobim_alignment::CATEGORY, "MC10", Pt::new(0.0, 0.0), 1.0, 1.0, "Alignments", serde_json::to_value(&a).unwrap());

    // Road markings: a broken centre line and a continuous edge line, both on the pavement.
    let centre = MarkingDefinition::new(LineType::Centre1008_1, 50.0, 150.0, 0.0, 60).with_width(100).with_material("thermoplastic");
    afrobim_markings::add_marking(&mut doc, alignment, "RM-001", &centre).unwrap();
    let edge = MarkingDefinition::new(LineType::Edge1012_1, 250.0, 400.0, 3.4, 60).with_width(150);
    afrobim_markings::add_marking(&mut doc, alignment, "RM-002", &edge).unwrap();
    // A marking no pavement carries: it ends past the pavement.
    let beyond = MarkingDefinition::new(LineType::Centre1008_1, 460.0, 560.0, 0.0, 60).with_width(100);
    afrobim_markings::add_marking(&mut doc, alignment, "RM-003", &beyond).unwrap();

    // Drainage.
    let mut drain = SideDrainDefinition::placeholder("MC10", 20.0, 120.0, 6.0);
    drain.manning_n = Some(0.03);
    drain.gradient_m_per_m = 0.01;
    afrobim_drainage::add_side_drain(&mut doc, "SD-001", &drain).unwrap();
    let mut single = PipeCulvertDefinition::placeholder("MC10", 80.0);
    single.material = "concrete pipe (test text)".into();
    afrobim_drainage::add_pipe_culvert(&mut doc, "CU-001", &single).unwrap();
    let mut twin = PipeCulvertDefinition::placeholder("MC10", 180.0);
    twin.barrels = 2;
    twin.barrel_spacing_mm = Some(1800.0);
    twin.skew_deg = 10.0;
    afrobim_drainage::add_pipe_culvert(&mut doc, "CU-002", &twin).unwrap();

    // Gantries, a sign hung on the portal.
    let portal = afrobim_gantry::add_gantry(&mut doc, "G-001", &GantryDefinition::placeholder_portal("MC10", 300.0), Some(&a)).unwrap();
    afrobim_gantry::add_gantry(&mut doc, "G-002", &GantryDefinition::placeholder_cantilever("MC10", 380.0), Some(&a)).unwrap();
    let sign = doc.add("sign", "GS-001", Pt::new(0.0, 0.0), 3000.0, 2000.0, "Signs", json!({ "kind": "Marker", "catalog_key": "TEST-1" }));
    afrobim_gantry::add_sign_to_gantry(&mut doc, portal, sign, SignPosition::Offset { offset_m: 0.0 }, 100.0, Some(Direction::UpStation), Some(&a)).unwrap();

    // Street lighting: an installation of staggered columns and one column on its own.
    let installation = Installation {
        reference: "LZ-01".into(),
        alignment: "MC10".into(),
        start_station_m: 40.0,
        end_station_m: 200.0,
        arrangement: Arrangement::Staggered,
        spacing_m: 40.0,
        end_treatment: Default::default(),
        first_side: Side::Left,
        carriageway_width_m: 7.0,
        reference_offset_m: 0.0,
        setback_m: 1.0,
        column: spec(),
        lighting_class: "M3 (test label)".into(),
        mark_prefix: "LC".into(),
        first_number: 1,
    };
    afrobim_lighting::add_installation(&mut doc, &a, installation).unwrap();
    let column = SingleColumn { mark: "LC-900".into(), alignment: "MC10".into(), station_m: 330.0, offset_m: 0.0, aim: Default::default(), column: spec(), lighting_class: String::new() };
    afrobim_lighting::add_column(&mut doc, &a, column).unwrap();

    // Road restraint: a run with a terminal at the start and a transition at the end.
    let run = BarrierRun {
        name: "VRS-001".into(),
        alignment: "MC10".into(),
        start_station_m: 100.0,
        end_station_m: 180.0,
        offset_m: 5.0,
        traffic_face: TrafficFace::Left,
        containment: ContainmentLevel::N2,
        working_width: WorkingWidthClass::W4,
        impact_severity: ImpactSeverityLevel::A,
        vehicle_intrusion: None,
        system: "as specified (test text)".into(),
        post_spacing_m: Some(2.0),
        beam_height_m: 0.6,
        start_end: EndTreatment::Terminal { performance_class: TerminalClass::T110, energy_absorbing: Some(true), description: "test terminal".into() },
        end_end: EndTreatment::ConnectsTo {
            system: afrobim_restraint::ConnectedSystem::Parapet,
            name: "P1".into(),
            transition: Some(Transition { description: "test transition".into(), containment: Some(ContainmentLevel::N2), impact_severity: None, connected_containment: None }),
        },
        set_back_m: Some(1.2),
    };
    afrobim_restraint::add_barrier_run(&mut doc, &a, run).unwrap();

    // Pavement over the first 450 m, 7 m wide with two layers, one of them area-only.
    let layers = vec![
        Layer::new("Surfacing", Material::HotMixAsphalt, 50.0),
        Layer::new("Base", Material::GranularBase, 150.0),
        Layer::new("Seal", Material::SurfaceDressing, 0.0),
    ];
    let mut pavement = PavementDefinition::new(a.clone(), 7.0, 2.5, layers);
    pavement.start_station_m = 0.0;
    pavement.end_station_m = 450.0;
    afrobim_pavement::add_pavement(&mut doc, "PV-001", &pavement).unwrap();

    // Ground, and the corridor on it.
    let ground = Tin::from_grid(900.0, 4900.0, 25.0, 25.0, 20, 27, |x, y| 51.0 + 3.0 * (y / 80.0).sin() + 0.004 * (x - 1000.0)).unwrap();
    let surface = afrobim_corridor::add_surface(&mut doc, "Ground", &ground, Some("test.xml")).unwrap();
    let template = Template::symmetric(
        "Typical section",
        vec![Link::new("lane", 3.5, Slope::Percent(-2.5)).paved(), Link::new("shoulder", 1.5, Slope::Percent(-4.0))],
        Daylight { cut_run_per_rise: 2.0, fill_run_per_rise: 2.0 },
        0.3,
    );
    let definition = CorridorDefinition::single("Test corridor", template, 20.0);
    afrobim_corridor::add_corridor(&mut doc, alignment, surface, definition).unwrap();

    let modules = registry();
    let generated = doc.elements().iter().filter_map(|e| modules.generate(e).ok().map(|g| (e.id, g))).collect();
    RoadFixture { document: doc, alignment: a, generated }
}
