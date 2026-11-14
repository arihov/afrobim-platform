//! Hand-written IFC4 STEP files for the importer tests. Every product is a
//! `IfcBuildingElementProxy` named after the case, so tests find it by name.

#![allow(dead_code)]

/// A face of a faceted brep: outer loop, inner loops (0-based point indices), written reversed.
pub type BrepFace = (Vec<usize>, Vec<Vec<usize>>, bool);

pub struct B {
    lines: Vec<String>,
    bytes: usize,
    pub ctx: u32,
    pub storey: u32,
    pub products: Vec<u32>,
}

pub fn f(v: f64) -> String {
    let s = format!("{v:?}");
    if s.contains('.') || s.contains('e') {
        s
    } else {
        format!("{s}.")
    }
}

impl B {
    /// A project in millimetres with one site, one building and one storey (all unplaced), the
    /// geometric context, and the shape representation sub-contexts the products use.
    pub fn new(schema: &str, unit_line: &str) -> B {
        let mut b = B { lines: Vec::new(), bytes: 0, ctx: 0, storey: 0, products: Vec::new() };
        let unit = b.add(unit_line.to_string());
        let ua = b.add(format!("IFCUNITASSIGNMENT((#{unit}))"));
        let o = b.pt3(0.0, 0.0, 0.0);
        let wcs = b.add(format!("IFCAXIS2PLACEMENT3D(#{o},$,$)"));
        b.ctx = b.add(format!("IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-5,#{wcs},$)"));
        let ctx = b.ctx;
        let project = b.add(format!("IFCPROJECT('{}',$,'Synthetic',$,$,$,$,(#{ctx}),#{ua})", guid(0)));
        let (site_pl, site) = b.spatial_placed("IFCSITE", "Site", None, [0.0; 3], None, 1);
        let _ = site_pl;
        let (_, building) = b.spatial_placed("IFCBUILDING", "Building", None, [0.0; 3], None, 2);
        let (_, storey) = b.spatial_placed("IFCBUILDINGSTOREY", "Storey", None, [0.0; 3], None, 3);
        b.storey = storey;
        b.add(format!("IFCRELAGGREGATES('{}',$,$,$,#{project},(#{site}))", guid(100)));
        b.add(format!("IFCRELAGGREGATES('{}',$,$,$,#{site},(#{building}))", guid(101)));
        b.add(format!("IFCRELAGGREGATES('{}',$,$,$,#{building},(#{storey}))", guid(102)));
        let _ = schema;
        b
    }

    pub fn add(&mut self, body: String) -> u32 {
        let id = self.lines.len() as u32 + 1;
        self.bytes += body.len() + 8;
        self.lines.push(format!("#{id}={body};"));
        id
    }

    /// Bytes written so far, approximately.
    pub fn approx_bytes(&self) -> usize {
        self.bytes
    }

    pub fn pt3(&mut self, x: f64, y: f64, z: f64) -> u32 {
        self.add(format!("IFCCARTESIANPOINT(({},{},{}))", f(x), f(y), f(z)))
    }

    pub fn pt2(&mut self, x: f64, y: f64) -> u32 {
        self.add(format!("IFCCARTESIANPOINT(({},{}))", f(x), f(y)))
    }

    pub fn dir3(&mut self, d: [f64; 3]) -> u32 {
        self.add(format!("IFCDIRECTION(({},{},{}))", f(d[0]), f(d[1]), f(d[2])))
    }

    pub fn dir2(&mut self, d: [f64; 2]) -> u32 {
        self.add(format!("IFCDIRECTION(({},{}))", f(d[0]), f(d[1])))
    }

    pub fn axis3(&mut self, at: [f64; 3], axis: Option<[f64; 3]>, reference: Option<[f64; 3]>) -> u32 {
        let p = self.pt3(at[0], at[1], at[2]);
        let a = axis.map(|d| format!("#{}", self.dir3(d))).unwrap_or_else(|| "$".into());
        let r = reference.map(|d| format!("#{}", self.dir3(d))).unwrap_or_else(|| "$".into());
        self.add(format!("IFCAXIS2PLACEMENT3D(#{p},{a},{r})"))
    }

    pub fn axis2(&mut self, at: [f64; 2], reference: Option<[f64; 2]>) -> u32 {
        let p = self.pt2(at[0], at[1]);
        let r = reference.map(|d| format!("#{}", self.dir2(d))).unwrap_or_else(|| "$".into());
        self.add(format!("IFCAXIS2PLACEMENT2D(#{p},{r})"))
    }

    pub fn local_placement(&mut self, rel_to: Option<u32>, at: [f64; 3], reference: Option<[f64; 3]>) -> u32 {
        let a = self.axis3(at, None, reference);
        let r = rel_to.map(|p| format!("#{p}")).unwrap_or_else(|| "$".into());
        self.add(format!("IFCLOCALPLACEMENT({r},#{a})"))
    }

    fn spatial_placed(&mut self, ty: &str, name: &str, rel_to: Option<u32>, at: [f64; 3], reference: Option<[f64; 3]>, n: u64) -> (u32, u32) {
        let pl = self.local_placement(rel_to, at, reference);
        let tail = match ty {
            "IFCSITE" => ",.ELEMENT.,$,$,$,$,$",
            "IFCBUILDING" => ",.ELEMENT.,$,$,$",
            _ => ",.ELEMENT.,$",
        };
        let id = self.add(format!("{ty}('{}',$,'{name}',$,$,#{pl},$,${tail})", guid(n)));
        (pl, id)
    }

    /// An `IfcShapeRepresentation` named `identifier` of `rtype` holding `items`, wrapped in a product shape.
    pub fn shape(&mut self, identifier: &str, rtype: &str, items: &[u32]) -> u32 {
        let list = items.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
        let ctx = self.ctx;
        let rep = self.add(format!("IFCSHAPEREPRESENTATION(#{ctx},'{identifier}','{rtype}',({list}))"));
        self.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{rep}))"))
    }

    pub fn shape_multi(&mut self, reps: &[(&str, &str, Vec<u32>)]) -> u32 {
        let ctx = self.ctx;
        let mut ids = Vec::new();
        for (identifier, rtype, items) in reps {
            let list = items.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
            ids.push(self.add(format!("IFCSHAPEREPRESENTATION(#{ctx},'{identifier}','{rtype}',({list}))")));
        }
        let list = ids.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
        self.add(format!("IFCPRODUCTDEFINITIONSHAPE($,$,({list}))"))
    }

    /// An `IfcBuildingElementProxy` contained in the storey.
    pub fn proxy(&mut self, name: &str, placement: u32, shape: Option<u32>) -> u32 {
        let n = 1000 + self.products.len() as u64;
        let s = shape.map(|s| format!("#{s}")).unwrap_or_else(|| "$".into());
        let id = self.add(format!("IFCBUILDINGELEMENTPROXY('{}',$,'{name}',$,$,#{placement},{s},$,.NOTDEFINED.)", guid(n)));
        self.products.push(id);
        id
    }

    pub fn rect(&mut self, x: f64, y: f64, at: [f64; 2], reference: Option<[f64; 2]>) -> u32 {
        let p = self.axis2(at, reference);
        self.add(format!("IFCRECTANGLEPROFILEDEF(.AREA.,$,#{p},{},{})", f(x), f(y)))
    }

    pub fn circle(&mut self, r: f64) -> u32 {
        let p = self.axis2([0.0, 0.0], None);
        self.add(format!("IFCCIRCLEPROFILEDEF(.AREA.,$,#{p},{})", f(r)))
    }

    pub fn polyline(&mut self, pts: &[[f64; 2]]) -> u32 {
        let ids: Vec<String> = pts.iter().map(|p| format!("#{}", self.pt2(p[0], p[1]))).collect();
        self.add(format!("IFCPOLYLINE(({}))", ids.join(",")))
    }

    pub fn closed_profile(&mut self, pts: &[[f64; 2]]) -> u32 {
        let mut p = pts.to_vec();
        p.push(pts[0]);
        let c = self.polyline(&p);
        self.add(format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{c})"))
    }

    pub fn profile_with_voids(&mut self, outer: &[[f64; 2]], inner: &[[f64; 2]]) -> u32 {
        let mut o = outer.to_vec();
        o.push(outer[0]);
        let mut i = inner.to_vec();
        i.push(inner[0]);
        let oc = self.polyline(&o);
        let ic = self.polyline(&i);
        self.add(format!("IFCARBITRARYPROFILEDEFWITHVOIDS(.AREA.,$,#{oc},(#{ic}))"))
    }

    pub fn extrusion(&mut self, profile: u32, position: Option<u32>, dir: [f64; 3], depth: f64) -> u32 {
        let pos = match position {
            Some(p) => p,
            None => self.axis3([0.0; 3], None, None),
        };
        let d = self.dir3(dir);
        self.add(format!("IFCEXTRUDEDAREASOLID(#{profile},#{pos},#{d},{})", f(depth)))
    }

    /// A faceted brep: `faces` are (outer loop, inner loops) as 0-based indices into `pts`; the
    /// bool says to write the outer bound reversed with `.F.` orientation (the same face).
    pub fn brep(&mut self, pts: &[[f64; 3]], faces: &[BrepFace]) -> u32 {
        let ids: Vec<u32> = pts.iter().map(|p| self.pt3(p[0], p[1], p[2])).collect();
        let mut face_ids = Vec::new();
        for (outer, inner, reversed) in faces {
            let mut bounds = Vec::new();
            let mut ring: Vec<usize> = outer.clone();
            let orient = if *reversed {
                ring.reverse();
                ".F."
            } else {
                ".T."
            };
            let list = |r: &[usize]| r.iter().map(|&i| format!("#{}", ids[i])).collect::<Vec<_>>().join(",");
            let l = self.add(format!("IFCPOLYLOOP(({}))", list(&ring)));
            bounds.push(self.add(format!("IFCFACEOUTERBOUND(#{l},{orient})")));
            for h in inner {
                let l = self.add(format!("IFCPOLYLOOP(({}))", list(h)));
                bounds.push(self.add(format!("IFCFACEBOUND(#{l},.T.)")));
            }
            let bl = bounds.iter().map(|b| format!("#{b}")).collect::<Vec<_>>().join(",");
            face_ids.push(self.add(format!("IFCFACE(({bl}))")));
        }
        let fl = face_ids.iter().map(|b| format!("#{b}")).collect::<Vec<_>>().join(",");
        let shell = self.add(format!("IFCCLOSEDSHELL(({fl}))"));
        self.add(format!("IFCFACETEDBREP(#{shell})"))
    }

    pub fn triangulated(&mut self, pts: &[[f64; 3]], tris: &[[u32; 3]], pn_index: Option<&[u32]>) -> u32 {
        let coords: Vec<String> = pts.iter().map(|p| format!("({},{},{})", f(p[0]), f(p[1]), f(p[2]))).collect();
        let list = self.add(format!("IFCCARTESIANPOINTLIST3D(({}))", coords.join(",")));
        let idx: Vec<String> = tris.iter().map(|t| format!("({},{},{})", t[0], t[1], t[2])).collect();
        let pn = pn_index.map(|p| format!("({})", p.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","))).unwrap_or_else(|| "$".into());
        self.add(format!("IFCTRIANGULATEDFACESET(#{list},$,.T.,({}),{pn})", idx.join(",")))
    }

    /// Polygonal faces: each is (outer 1-based indices, voids).
    pub fn polygonal(&mut self, pts: &[[f64; 3]], faces: &[(Vec<u32>, Vec<Vec<u32>>)]) -> u32 {
        let coords: Vec<String> = pts.iter().map(|p| format!("({},{},{})", f(p[0]), f(p[1]), f(p[2]))).collect();
        let list = self.add(format!("IFCCARTESIANPOINTLIST3D(({}))", coords.join(",")));
        let nums = |v: &[u32]| format!("({})", v.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        let mut ids = Vec::new();
        for (outer, voids) in faces {
            if voids.is_empty() {
                ids.push(self.add(format!("IFCINDEXEDPOLYGONALFACE({})", nums(outer))));
            } else {
                let v: Vec<String> = voids.iter().map(|h| nums(h)).collect();
                ids.push(self.add(format!("IFCINDEXEDPOLYGONALFACEWITHVOIDS({},({}))", nums(outer), v.join(","))));
            }
        }
        let fl = ids.iter().map(|b| format!("#{b}")).collect::<Vec<_>>().join(",");
        self.add(format!("IFCPOLYGONALFACESET(#{list},.T.,({fl}),$)"))
    }

    pub fn mapped(&mut self, origin: u32, rep_items: &[u32], target: u32) -> u32 {
        let list = rep_items.iter().map(|i| format!("#{i}")).collect::<Vec<_>>().join(",");
        let ctx = self.ctx;
        let rep = self.add(format!("IFCSHAPEREPRESENTATION(#{ctx},'Body','SweptSolid',({list}))"));
        let map = self.add(format!("IFCREPRESENTATIONMAP(#{origin},#{rep})"));
        self.add(format!("IFCMAPPEDITEM(#{map},#{target})"))
    }

    pub fn operator(&mut self, x: Option<[f64; 3]>, y: Option<[f64; 3]>, z: Option<[f64; 3]>, origin: [f64; 3], scale: Option<f64>) -> u32 {
        let d = |b: &mut B, v: Option<[f64; 3]>| v.map(|d| format!("#{}", b.dir3(d))).unwrap_or_else(|| "$".into());
        let (a1, a2, a3) = (d(self, x), d(self, y), d(self, z));
        let o = self.pt3(origin[0], origin[1], origin[2]);
        let s = scale.map(f).unwrap_or_else(|| "$".into());
        self.add(format!("IFCCARTESIANTRANSFORMATIONOPERATOR3D({a1},{a2},#{o},{s},{a3})"))
    }

    pub fn finish(mut self) -> String {
        if !self.products.is_empty() {
            let list = self.products.iter().map(|p| format!("#{p}")).collect::<Vec<_>>().join(",");
            let storey = self.storey;
            self.add(format!("IFCRELCONTAINEDINSPATIALSTRUCTURE('{}',$,$,$,({list}),#{storey})", guid(103)));
        }
        let mut s = String::from("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView]'),'2;1');\nFILE_NAME('synthetic.ifc','2026-01-01T00:00:00',(''),(''),'','AfroBIM test','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n");
        for l in &self.lines {
            s.push_str(l);
            s.push('\n');
        }
        s.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        s
    }
}

/// A 22 character GlobalId-shaped string.
pub fn guid(n: u64) -> String {
    format!("{n:0>22}")
}

pub const CUBOID_FACES: [[usize; 4]; 6] = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];

pub fn cuboid(w: f64, d: f64, h: f64) -> [[f64; 3]; 8] {
    [[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, d, 0.0], [0.0, d, 0.0], [0.0, 0.0, h], [w, 0.0, h], [w, d, h], [0.0, d, h]]
}
