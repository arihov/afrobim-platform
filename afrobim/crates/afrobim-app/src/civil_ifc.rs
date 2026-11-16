//! What each civil object becomes in the IFC export. Only mappings verified against the IFC4X3_ADD2
//! schema (through IfcOpenShell) are written; the rest are reported as not mapped.

/// One line for the Properties panel.
pub fn mapping_note(category: &str) -> &'static str {
    match category {
        crate::civil::SURFACE => "Not mapped yet",
        crate::civil::CORRIDOR => "Not mapped yet",
        _ => "Not mapped: stays in the AfroBIM document and in LandXML",
    }
}
