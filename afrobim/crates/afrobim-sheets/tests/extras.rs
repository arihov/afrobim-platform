mod common;

use afrobim_sheets::*;
use common::fixture;

fn plan_set() -> SheetSet {
    let view = PlanView::new("MC1", 0.0, 1.0, Scale(1000.0));
    let template = SheetSpec::new("PLAN", Page::new(PaperSize::A3, Orientation::Landscape), vec![ViewDef::full(ViewKind::Plan(view))]);
    let mut set = SheetSet::new("Test set");
    set.sources.push(SheetSource::PlanSeries(PlanSeries { template, plan_view: 0, from_station_m: Some(0.0), to_station_m: Some(300.0), length_m: 300.0, overlap_m: 30.0 }));
    set
}

#[test]
fn numbering_pads_steps_and_wraps_with_prefix_and_suffix() {
    let n = Numbering { prefix: "RD-".into(), digits: 4, start: 10, step: 5, suffix: "-A".into() };
    assert_eq!(n.number(0), "RD-0010-A");
    assert_eq!(n.number(3), "RD-0025-A");
    assert_eq!(Numbering::default().number(0), "001");
    assert_eq!(Numbering::default().number(11), "012");
}

#[test]
fn the_layer_to_pen_mapping_sets_weights_and_colours() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut set = plan_set();
    set.styles.pens.set(afrobim_markings::LAYER, Pen::new(0.7, [10, 20, 30]));
    set.styles.model_colours = false;
    set.styles.pens.set(layers::FRAME, Pen::new(1.0, [0, 0, 0]));
    let composed = compose_set(&model, &set);
    let s = &composed.sheets[0];
    let on = |name: &str| -> Vec<(f64, [u8; 3])> {
        s.items.iter().filter_map(|i| if let Item::Stroke { layer, weight_mm, rgb, .. } = i { (layer == name).then_some((*weight_mm, *rgb)) } else { None }).collect()
    };
    // Marking geometry is filled bands: with model colours off they print in the layer's pen colour.
    let fills: Vec<[u8; 3]> = s.items.iter().filter_map(|i| if let Item::Fill { layer, rgb, .. } = i { (layer == afrobim_markings::LAYER).then_some(*rgb) } else { None }).collect();
    assert!(!fills.is_empty() && fills.iter().all(|c| *c == [10, 20, 30]), "{fills:?}");
    // A pen weight applies to hairlines: the alignment centreline (layer PLAN-ALIGNMENT) takes its own pen.
    assert!(on(layers::PLAN_ALIGNMENT).iter().all(|(w, _)| (*w - 0.35).abs() < 1e-12));
    assert_eq!(on(layers::FRAME).first().map(|f| f.0), Some(1.0));
    // And the DXF carries the pen weight as polyline width on that layer.
    let dxf = composed.dxf(&set.styles, 10.0);
    assert!(dxf.contains("\n40\n1\n41\n1\n"));
}

#[test]
fn every_paper_size_and_orientation_is_the_media_box_of_its_page() {
    let f = fixture();
    let model = Model { doc: &f.doc, registry: &f.registry };
    let mut set = SheetSet::new("sizes");
    let mut want = Vec::new();
    for size in PaperSize::ALL {
        for o in [Orientation::Landscape, Orientation::Portrait] {
            let notes = Notes { heading: Some("N".into()), paragraphs: vec!["x".into()], numbered: false };
            let mut spec = SheetSpec::new("NOTES", Page::new(size, o), vec![ViewDef::full(ViewKind::Notes(notes))]);
            // A4 portrait is only 210 mm wide: put the title block under the views.
            spec.title_block.placement = TitlePlacement::BottomRight;
            spec.title_block.width_mm = 100.0;
            want.push(spec.page.dimensions_mm());
            set.sources.push(SheetSource::Single(spec));
        }
    }
    let composed = compose_set(&model, &set);
    let pdf = composed.pdf("sizes");
    let text: String = pdf.iter().map(|&b| if b < 128 { b as char } else { '?' }).collect();
    let boxes: Vec<(f64, f64)> = text
        .match_indices("/MediaBox [0 0 ")
        .map(|(i, _)| {
            let rest = &text[i + 15..];
            let end = rest.find(']').unwrap();
            let mut it = rest[..end].split(' ').map(|v| v.parse::<f64>().unwrap());
            (it.next().unwrap(), it.next().unwrap())
        })
        .collect();
    assert_eq!(boxes.len(), want.len());
    for ((bw, bh), (w, h)) in boxes.iter().zip(&want) {
        // Points are 1/72 in and 25.4 mm to the inch.
        assert!((bw - w * 72.0 / 25.4).abs() < 0.01 && (bh - h * 72.0 / 25.4).abs() < 0.01, "{bw}x{bh} vs {w}x{h}");
    }
    // Landscape A4 is the long side across; the first page is A0 landscape.
    assert!(boxes[0].0 > boxes[0].1 && boxes[1].0 < boxes[1].1);
}
