use afrobim_culvert::*;
fn main() {
    let input = CulvertInput::reference();
    let d = design(&input).expect("design");
    let l = &d.layout;
    println!("left {:.1} right {:.1} total {:.1}; grad {:.4}%; inv cl {:.4}; cover cl {:.0} min {:.0}", l.left_len_mm, l.right_len_mm, l.barrel_len_mm, l.gradient*100.0, l.invert_cl_m, l.cover_cl_mm, l.min_cover_mm);
    println!("hw {} overall {:.1} apron+toe {:.1}", l.left.headwall_width_mm, l.left.overall_width_mm, l.left.apron_and_toe_mm);
    let q = &d.quantities;
    println!("blind pipe {:.3} bedding {:.3} surround {:.3} end blind {:.3} hw {:.3} wing {:.3} apron {:.3} toe {:.3} total {:.3} weep {:.2} exc {:.2} back {:.2}", q.barrel_blinding_m3, q.bedding_m3, q.surround_m3, q.end_blinding_m3, q.headwalls_m3, q.wings_m3, q.aprons_m3, q.toes_m3, q.end_structures_m3, q.weep_holes_m, q.excavation_m3, q.backfill_m3);
    for r in &d.bbs.rows { println!("{:6} {:>2}x{:<3}={:<4} Y{} {:>5.0} code {} {:?} {:.3}", r.mark, r.bars_per_element, r.elements, r.total_bars, r.diameter_mm, r.cutting_length_mm, r.shape_code, r.dims, r.mass_kg); }
    println!("total {:.3}", d.bbs.total_kg);
    for c in &d.checks { println!("{} {} {}", c.name, c.passed, c.message); }
}
