//! Regenerates REVIEW.md from data/dmrb.json:
//! `cargo run -p afrobim-defaults --example write_review`
use afrobim_defaults::review::review_markdown;
use afrobim_defaults::Pack;

fn main() {
    let md = review_markdown(&Pack::dmrb());
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/REVIEW.md");
    std::fs::write(path, md).expect("write REVIEW.md");
    println!("wrote {path}");
}
