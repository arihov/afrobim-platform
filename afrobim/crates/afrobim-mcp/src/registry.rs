//! The MCP tool list: names, titles, descriptions, strict input schemas and the annotations
//! that tell a client which tools only read, plus the binding from JSON arguments to the typed
//! tool methods (port of C# `ToolRegistry`).
//!
//! The first five tools are the C# server's, with the same names, parameters and result shapes.
//! The `document_*` tools are new: they work on an `.afrobim` model file.

use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::documents::AddSign;
use crate::tools::{ToolError, Tools};

const PACK_PARAM: &str = "Rule pack id from list_packs, e.g. 'UK-TSM-2019' (UK Traffic Signs Manual Chapter 7, the default). Omit for the default pack.";

const DOC_PACK_PARAM: &str = "Rule pack id from list_packs. Omit to use the document's own standard, or the default pack when it names none.";

const ROW_FORMAT: &str = "A schedule CSV data line in the canonical column order code,kind,speed_kph,approach_class,background,\
destinations[,variant,scheme,symbol,info_template,info_arrow,info_distance,class,catalog_key,mirror,text_lines,embedded]; trailing \
optional columns may be omitted. Destinations are 'Name:Direction:Order[:Distance[:Symbol[:RouteNumber[:Route[:NumberStatus]]]]]' \
joined by ';'. Example: 'ADS-1,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0'.";

const DOC_PATH: &str =
    "Path to the .afrobim document (absolute, or relative to the server's root folder).";

/// One declared parameter of a tool.
pub struct Param {
    pub name: &'static str,
    pub required: bool,
    pub schema: Value,
}

fn optional(name: &'static str, kind: &str, description: &str) -> Param {
    Param {
        name,
        required: false,
        schema: json!({ "description": description, "type": [kind, "null"], "default": null }),
    }
}

fn required(name: &'static str, kind: &str, description: &str) -> Param {
    Param {
        name,
        required: true,
        schema: json!({ "description": description, "type": kind }),
    }
}

fn string_list(name: &'static str, description: &str) -> Param {
    Param {
        name,
        required: false,
        schema: json!({ "description": description, "type": ["array", "null"], "items": { "type": "string" }, "default": null }),
    }
}

pub type Handler = fn(&mut Tools, &Args) -> Result<String, ToolError>;

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: String,
    pub read_only: bool,
    /// May overwrite or remove something that already exists.
    pub destructive: bool,
    /// Calling it again with the same arguments changes nothing further.
    pub idempotent: bool,
    pub params: Vec<Param>,
    pub call: Handler,
}

impl ToolSpec {
    /// A strict schema: an argument the tool does not declare is refused.
    pub fn input_schema(&self) -> Value {
        let properties: Map<String, Value> = self
            .params
            .iter()
            .map(|p| (p.name.to_string(), p.schema.clone()))
            .collect();
        let required: Vec<&str> = self
            .params
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name)
            .collect();
        let mut schema =
            json!({ "type": "object", "properties": properties, "additionalProperties": false });
        if !required.is_empty() {
            schema["required"] = json!(required);
        }
        schema
    }

    /// The tool as `tools/list` describes it.
    pub fn descriptor(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema(),
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.read_only,
                "destructiveHint": self.destructive,
                "idempotentHint": self.idempotent,
                "openWorldHint": false,
            },
        })
    }

    /// Rejects a call that passes an argument the tool does not declare, so a misspelt
    /// parameter fails loudly instead of being silently ignored.
    pub fn unknown_argument(&self, args: &Map<String, Value>) -> Option<String> {
        let unknown: Vec<&str> = args
            .keys()
            .map(String::as_str)
            .filter(|k| !self.params.iter().any(|p| p.name == *k))
            .collect();
        if unknown.is_empty() {
            return None;
        }
        let accepted: Vec<&str> = self.params.iter().map(|p| p.name).collect();
        Some(format!(
            "unknown argument(s) for {}: {}; accepted: {}",
            self.name,
            unknown.join(", "),
            if accepted.is_empty() {
                "none".to_string()
            } else {
                accepted.join(", ")
            }
        ))
    }
}

/// The arguments of one call. A JSON `null` is an omitted argument.
pub struct Args<'a>(pub &'a Map<String, Value>);

impl<'a> Args<'a> {
    fn get(&self, name: &str) -> Option<&'a Value> {
        self.0.get(name).filter(|v| !v.is_null())
    }

    pub fn string(&self, name: &str) -> Result<Option<&'a str>, ToolError> {
        match self.get(name) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.as_str())),
            Some(_) => Err(ToolError::new(format!("'{name}' must be a string"))),
        }
    }

    pub fn required_string(&self, name: &str) -> Result<&'a str, ToolError> {
        self.string(name)?
            .ok_or_else(|| ToolError::new(format!("missing required argument '{name}'")))
    }

    pub fn integer(&self, name: &str) -> Result<Option<i64>, ToolError> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v
                .as_i64()
                .or_else(|| {
                    v.as_f64()
                        .filter(|f| f.fract() == 0.0 && f.abs() < 9.0e15)
                        .map(|f| f as i64)
                })
                .map(Some)
                .ok_or_else(|| ToolError::new(format!("'{name}' must be an integer"))),
        }
    }

    pub fn unsigned(&self, name: &str) -> Result<Option<u64>, ToolError> {
        match self.integer(name)? {
            Some(n) => u64::try_from(n)
                .map(Some)
                .map_err(|_| ToolError::new(format!("'{name}' must not be negative"))),
            None => Ok(None),
        }
    }

    pub fn number(&self, name: &str) -> Result<Option<f64>, ToolError> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v
                .as_f64()
                .map(Some)
                .ok_or_else(|| ToolError::new(format!("'{name}' must be a number"))),
        }
    }

    pub fn boolean(&self, name: &str) -> Result<Option<bool>, ToolError> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v
                .as_bool()
                .map(Some)
                .ok_or_else(|| ToolError::new(format!("'{name}' must be true or false"))),
        }
    }

    pub fn strings(&self, name: &str) -> Result<Option<Vec<String>>, ToolError> {
        let Some(v) = self.get(name) else {
            return Ok(None);
        };
        let wrong = || ToolError::new(format!("'{name}' must be an array of strings"));
        let items = v.as_array().ok_or_else(wrong)?;
        // A null item is an empty key, which the tool reports by name rather than rejecting the call.
        items
            .iter()
            .map(|i| {
                if i.is_null() {
                    Ok(String::new())
                } else {
                    i.as_str().map(str::to_string).ok_or_else(wrong)
                }
            })
            .collect::<Result<_, _>>()
            .map(Some)
    }

    pub fn object(&self, name: &str) -> Result<Option<&'a Map<String, Value>>, ToolError> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v
                .as_object()
                .map(Some)
                .ok_or_else(|| ToolError::new(format!("'{name}' must be an object"))),
        }
    }

    pub fn value(&self, name: &str) -> Option<&'a Value> {
        self.get(name)
    }
}

/// Compact JSON with the fields in declaration order and nulls written out, as the C# server
/// serialises its results.
fn text<T: Serialize>(result: T) -> Result<String, ToolError> {
    serde_json::to_string(&result)
        .map_err(|e| ToolError::new(format!("could not serialise the result: {e}")))
}

/// Every tool, in the order `tools/list` returns them.
pub fn all() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "list_packs",
            title: "List rule packs",
            description: "List the rule packs (road-sign standards) installed next to the default pack, with their catalog size, \
                          which one is the default, and whether the Transport fonts were found."
                .into(),
            read_only: true,
            destructive: false,
            idempotent: true,
            params: vec![],
            call: |t, _| text(t.list_packs()?),
        },
        ToolSpec {
            name: "search_catalog",
            title: "Search the sign catalog",
            description: "Search a rule pack's standard-sign catalog. Returns each page's key (pass it to layout_sign, export_dxf or \
                          document_add_sign), layout family, series, colour scheme and description. All filters are optional and \
                          case-insensitive; with none, the whole catalog is returned."
                .into(),
            read_only: true,
            destructive: false,
            idempotent: true,
            params: vec![
                optional("pack", "string", PACK_PARAM),
                optional("query", "string", "Text matched against the key, description and family, e.g. 'stack', 'roundabout', 'UK04'."),
                optional("series", "string", "Exact series filter, e.g. 'UK' (direction signs) or 'D' (TSRGD warning and regulatory diagrams) for the UK pack."),
            ],
            call: |t, a| text(t.search_catalog(a.string("pack")?, a.string("query")?, a.string("series")?)?),
        },
        ToolSpec {
            name: "layout_sign",
            title: "Lay out one sign",
            description: "Lay out one sign and return its diagnostics (with standard clause citations), finished size in mm, stroke \
                          width (sw, mm), primitive counts, and an SVG preview of the export geometry (lettering as Transport glyph \
                          outlines). Give exactly one of 'key' (a catalog key from search_catalog) or 'schedule_row'. Writes no files."
                .into(),
            read_only: true,
            destructive: false,
            idempotent: true,
            params: vec![
                optional("pack", "string", PACK_PARAM),
                optional("key", "string", "Catalog key, e.g. 'UK04.1'. Mutually exclusive with schedule_row."),
                optional("schedule_row", "string", &format!("{ROW_FORMAT} Mutually exclusive with key.")),
                optional(
                    "speed_kph",
                    "integer",
                    "Design speed in km/h for a catalog key (ignored with schedule_row, which carries its own). Omit to use the catalog default.",
                ),
            ],
            call: |t, a| text(t.layout_sign(a.string("pack")?, a.string("key")?, a.string("schedule_row")?, a.integer("speed_kph")?)?),
        },
        ToolSpec {
            name: "export_dxf",
            title: "Export DXF files",
            description: "Write one R12 DXF per sign (lettering as Transport glyph outlines, millimetres) into out_dir and return the \
                          files written plus each sign's diagnostics. Give exactly one of 'signs' (catalog keys) or 'csv_path' (a \
                          schedule CSV). Files are only ever written directly inside out_dir; existing DXFs of the same name are \
                          overwritten."
                .into(),
            read_only: false,
            destructive: true, // overwrites same-named DXFs in out_dir
            idempotent: true,
            params: vec![
                required("out_dir", "string", "Output folder (absolute, or relative to the server's root folder). Created if missing. Nothing is written outside it."),
                optional("pack", "string", PACK_PARAM),
                string_list("signs", "Catalog keys to export, e.g. ['UK04.1','UK05']. Mutually exclusive with csv_path."),
                optional("csv_path", "string", "Path to a sign-schedule CSV (absolute, or relative to the server's root folder). Mutually exclusive with signs."),
            ],
            call: |t, a| {
                let signs = a.strings("signs")?;
                text(t.export_dxf(a.required_string("out_dir")?, a.string("pack")?, signs.as_deref(), a.string("csv_path")?)?)
            },
        },
        ToolSpec {
            name: "validate_schedule",
            title: "Validate a sign schedule",
            description: "Check every row of a sign-schedule CSV against the rule pack without writing anything. Returns, per row, the \
                          parse or layout errors and warnings, each with the standard clause it cites."
                .into(),
            read_only: true,
            destructive: false,
            idempotent: true,
            params: vec![
                required("csv_path", "string", "Path to a sign-schedule CSV (absolute, or relative to the server's root folder)."),
                optional("pack", "string", PACK_PARAM),
            ],
            call: |t, a| text(t.validate_schedule(a.required_string("csv_path")?, a.string("pack")?)?),
        },
        ToolSpec {
            name: "document_new",
            title: "Create an AfroBIM document",
            description: "Create an empty AfroBIM model file (.afrobim) that the desktop app can open, with the rule pack as its design \
                          standard. Add signs with document_add_sign. Refuses to replace an existing file unless overwrite is true."
                .into(),
            read_only: false,
            destructive: true, // with overwrite: true it replaces a document
            idempotent: true,
            params: vec![
                required("path", "string", "Path of the .afrobim file to create (absolute, or relative to the server's root folder). Folders are created if missing."),
                optional("pack", "string", PACK_PARAM),
                Param {
                    name: "overwrite",
                    required: false,
                    schema: json!({ "description": "Replace an existing file with an empty document. Default false.", "type": "boolean", "default": false }),
                },
            ],
            call: |t, a| text(t.document_new(a.required_string("path")?, a.string("pack")?, a.boolean("overwrite")?.unwrap_or(false))?),
        },
        ToolSpec {
            name: "document_add_sign",
            title: "Add a sign to a document",
            description: "Lay out one sign, add it to an .afrobim document as a sign element and save the document. Give exactly one of \
                          'key' (a catalog key), 'schedule_row' or 'definition'. Returns the element id, its mark, position and \
                          finished size in mm, and the sign's diagnostics; a sign with errors is still added so it can be corrected. \
                          Without x_mm and y_mm the sign is placed after the last element, in rows."
                .into(),
            read_only: false,
            destructive: false,
            idempotent: false, // each call adds another sign
            params: vec![
                required("path", "string", DOC_PATH),
                optional("key", "string", "Catalog key from search_catalog, e.g. 'UK04.1'. Mutually exclusive with schedule_row and definition."),
                optional("schedule_row", "string", &format!("{ROW_FORMAT} Mutually exclusive with key and definition.")),
                optional(
                    "definition",
                    "object",
                    "A sign definition object as document_list returns it, e.g. {\"kind\":\"StackType\",\"speed_kph\":60,\"background\":\"Green\",\
                     \"destinations\":[{\"name\":\"Kasese\",\"direction\":\"Ahead\",\"distance\":\"12\"}]}. 'kind', 'speed_kph' and 'background' are \
                     required; enum values are written as in document_list (StackType, DirectionSign, Ahead, Green, ...). Mutually exclusive \
                     with key and schedule_row.",
                ),
                optional("speed_kph", "integer", "Design speed in km/h for a catalog key (ignored otherwise). Omit to use the catalog default."),
                optional(
                    "mark",
                    "string",
                    "The mark the sign is scheduled under, e.g. 'ADS-001'. Defaults to the sign's code. Made unique in the document (ADS-001 becomes ADS-002).",
                ),
                optional("x_mm", "number", "Model X of the sign's lower-left corner in millimetres. Give with y_mm, or omit both."),
                optional("y_mm", "number", "Model Y of the sign's lower-left corner in millimetres (Y up). Give with x_mm, or omit both."),
                optional("properties", "object", "Free-form properties stored on the element, e.g. {\"Material\":\"Class RA2\",\"Drawing\":\"TS-101\"}."),
                optional("pack", "string", DOC_PACK_PARAM),
            ],
            call: |t, a| {
                text(t.document_add_sign(&AddSign {
                    path: a.required_string("path")?,
                    pack: a.string("pack")?,
                    key: a.string("key")?,
                    schedule_row: a.string("schedule_row")?,
                    definition: a.value("definition"),
                    speed_kph: a.integer("speed_kph")?,
                    mark: a.string("mark")?,
                    x_mm: a.number("x_mm")?,
                    y_mm: a.number("y_mm")?,
                    properties: a.object("properties")?,
                })?)
            },
        },
        ToolSpec {
            name: "document_list",
            title: "List a document's elements",
            description: "Read an .afrobim document: its design standard, layers, overall extents and every element with its id, mark, \
                          position and size in mm, properties and definition. Changes nothing."
                .into(),
            read_only: true,
            destructive: false,
            idempotent: true,
            params: vec![required("path", "string", DOC_PATH)],
            call: |t, a| text(t.document_list(a.required_string("path")?)?),
        },
        ToolSpec {
            name: "document_remove_element",
            title: "Remove an element from a document",
            description: "Remove one element from an .afrobim document, by id or by mark, and save the document. To change a sign, \
                          remove it and add it again with document_add_sign."
                .into(),
            read_only: false,
            destructive: true,
            idempotent: false,
            params: vec![
                required("path", "string", DOC_PATH),
                optional("id", "integer", "Element id from document_list or document_add_sign. Mutually exclusive with mark."),
                optional("mark", "string", "Element mark, e.g. 'ADS-001' (case-insensitive). Mutually exclusive with id."),
            ],
            call: |t, a| text(t.document_remove_element(a.required_string("path")?, a.unsigned("id")?, a.string("mark")?)?),
        },
        ToolSpec {
            name: "document_save",
            title: "Check and save a document",
            description: "Lay out every sign in an .afrobim document again, bring each stored size up to date, report the errors and \
                          warnings per sign, and save the document: in place, or as a copy when save_as is given. The other document \
                          tools save on their own; use this to check a document that was edited elsewhere or to save it under a new name."
                .into(),
            read_only: false,
            destructive: true, // overwrites the document, or the save_as file
            idempotent: true,
            params: vec![
                required("path", "string", DOC_PATH),
                optional("save_as", "string", "Path of the .afrobim file to write instead of the original (absolute, or relative to the server's root folder)."),
                optional("pack", "string", DOC_PACK_PARAM),
            ],
            call: |t, a| text(t.document_save(a.required_string("path")?, a.string("save_as")?, a.string("pack")?)?),
        },
        ToolSpec {
            name: "document_export_dxf",
            title: "Export a document to DXF",
            description: "Write the signs of an .afrobim document to one R12 DXF in millimetres, each at its model position (lettering as \
                          Transport glyph outlines, one layer per sign colour), as the desktop app's Export DXF does. Returns the \
                          elements written and those left out. An existing file at out_path is overwritten."
                .into(),
            read_only: false,
            destructive: true, // overwrites out_path
            idempotent: true,
            params: vec![
                required("path", "string", DOC_PATH),
                required("out_path", "string", "Path of the .dxf file to write (absolute, or relative to the server's root folder). Folders are created if missing."),
                string_list("marks", "Marks of the elements to export, e.g. ['ADS-001','ADS-002']. Omit to export every sign."),
                optional("pack", "string", DOC_PACK_PARAM),
            ],
            call: |t, a| {
                let marks = a.strings("marks")?;
                text(t.document_export_dxf(a.required_string("path")?, a.required_string("out_path")?, marks.as_deref(), a.string("pack")?)?)
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    fn args(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn the_tools_are_listed_in_order_with_strict_schemas_and_descriptions() {
        let tools = all();
        let names: Vec<&str> = tools.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "list_packs",
                "search_catalog",
                "layout_sign",
                "export_dxf",
                "validate_schedule",
                "document_new",
                "document_add_sign",
                "document_list",
                "document_remove_element",
                "document_save",
                "document_export_dxf"
            ]
        );
        for t in &tools {
            let d = t.descriptor();
            assert!(!d["description"].as_str().unwrap().trim().is_empty());
            assert!(
                !d["description"].as_str().unwrap().contains("  "),
                "{}: description has a run of spaces",
                t.name
            );
            assert_eq!(d["inputSchema"]["type"], "object");
            assert_eq!(d["inputSchema"]["additionalProperties"], false);
            assert_eq!(d["annotations"]["title"], d["title"]);
            assert_eq!(d["annotations"]["openWorldHint"], false);
            assert!(!(t.read_only && t.destructive), "{}", t.name);
            for p in &t.params {
                assert!(
                    !p.schema["description"].as_str().unwrap().is_empty(),
                    "{}.{}",
                    t.name,
                    p.name
                );
            }
        }
    }

    #[test]
    fn schemas_match_the_csharp_server_for_the_ported_tools() {
        let tools = all();
        let schema = |name: &str| {
            tools
                .iter()
                .find(|t| t.name == name)
                .unwrap()
                .input_schema()
        };
        assert_eq!(
            schema("list_packs"),
            json!({ "type": "object", "properties": {}, "additionalProperties": false })
        );
        let export = schema("export_dxf");
        assert_eq!(export["required"], json!(["out_dir"]));
        assert_eq!(export["properties"]["out_dir"]["type"], "string");
        assert_eq!(
            export["properties"]["signs"]["type"],
            json!(["array", "null"])
        );
        assert_eq!(export["properties"]["csv_path"]["default"], Value::Null);
        let layout = schema("layout_sign");
        assert!(layout.get("required").is_none());
        assert_eq!(
            layout["properties"]["speed_kph"]["type"],
            json!(["integer", "null"])
        );
        let keys: Vec<&String> = layout["properties"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["key", "pack", "schedule_row", "speed_kph"]);
        assert_eq!(schema("validate_schedule")["required"], json!(["csv_path"]));
    }

    #[test]
    fn only_the_reading_tools_are_marked_read_only() {
        let read_only: Vec<&str> = all()
            .iter()
            .filter(|t| t.read_only)
            .map(|t| t.name)
            .collect();
        assert_eq!(
            read_only,
            [
                "list_packs",
                "search_catalog",
                "layout_sign",
                "validate_schedule",
                "document_list"
            ]
        );
    }

    #[test]
    fn unknown_arguments_are_refused_by_name() {
        let tools = all();
        let search = tools.iter().find(|t| t.name == "search_catalog").unwrap();
        assert_eq!(
            search
                .unknown_argument(&args(json!({ "qery": "x", "pack": null })))
                .unwrap(),
            "unknown argument(s) for search_catalog: qery; accepted: pack, query, series"
        );
        assert_eq!(
            search.unknown_argument(&args(json!({ "query": "x" }))),
            None
        );
        assert_eq!(search.unknown_argument(&args(json!({}))), None);
        let list = tools.iter().find(|t| t.name == "list_packs").unwrap();
        assert_eq!(
            list.unknown_argument(&args(json!({ "x": 1 }))).unwrap(),
            "unknown argument(s) for list_packs: x; accepted: none"
        );
    }

    #[test]
    fn arguments_are_type_checked_and_null_means_omitted() {
        let map = args(json!({
            "s": "text", "n": null, "i": 60, "whole": 60.0, "frac": 60.5, "neg": -1, "f": 1.5, "b": true,
            "list": ["a", null], "bad_list": ["a", 1], "o": {"k": "v"}
        }));
        let a = Args(&map);
        assert_eq!(a.string("s").unwrap(), Some("text"));
        assert_eq!(a.string("n").unwrap(), None);
        assert_eq!(a.string("absent").unwrap(), None);
        assert_eq!(a.string("i").unwrap_err().0, "'i' must be a string");
        assert_eq!(
            a.required_string("absent").unwrap_err().0,
            "missing required argument 'absent'"
        );
        assert_eq!(a.integer("i").unwrap(), Some(60));
        assert_eq!(a.integer("whole").unwrap(), Some(60));
        assert_eq!(
            a.integer("frac").unwrap_err().0,
            "'frac' must be an integer"
        );
        assert_eq!(a.integer("s").unwrap_err().0, "'s' must be an integer");
        assert_eq!(
            a.unsigned("neg").unwrap_err().0,
            "'neg' must not be negative"
        );
        assert_eq!(a.unsigned("i").unwrap(), Some(60));
        assert_eq!(a.number("f").unwrap(), Some(1.5));
        assert_eq!(a.number("i").unwrap(), Some(60.0));
        assert!(a.number("s").is_err());
        assert_eq!(a.boolean("b").unwrap(), Some(true));
        assert!(a.boolean("s").is_err());
        assert_eq!(
            a.strings("list").unwrap(),
            Some(vec!["a".to_string(), String::new()])
        );
        assert_eq!(
            a.strings("bad_list").unwrap_err().0,
            "'bad_list' must be an array of strings"
        );
        assert!(a.strings("s").is_err());
        assert_eq!(a.object("o").unwrap().unwrap().len(), 1);
        assert!(a.object("s").is_err());
        assert!(a.value("n").is_none() && a.value("o").is_some());
    }

    /// Every tool, called through its JSON binding the way `tools/call` calls it.
    #[test]
    fn every_handler_binds_its_arguments_and_returns_json() {
        let tmp = testing::TempDir::new("registry");
        let dir = tmp.path().to_string_lossy().into_owned();
        let doc = format!("{dir}/m.afrobim");
        let mut tools = Tools::new(testing::settings());
        let specs = all();
        let mut call = |name: &str, arguments: Value| -> Value {
            let spec = specs.iter().find(|t| t.name == name).unwrap();
            let map = args(arguments);
            assert_eq!(spec.unknown_argument(&map), None, "{name}");
            let text =
                (spec.call)(&mut tools, &Args(&map)).unwrap_or_else(|e| panic!("{name}: {e}"));
            serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{name}: result is not JSON: {e}"))
        };

        assert_eq!(call("list_packs", json!({}))["default_pack"], "UK-TSM-2019");
        assert_eq!(
            call(
                "search_catalog",
                json!({ "query": "stack", "series": "UK", "pack": "UK-TSM-2019" })
            )["count"],
            3
        );
        let laid = call("layout_sign", json!({ "key": "UK04.1", "speed_kph": 64 }));
        assert_eq!(
            (laid["width_mm"].as_f64(), laid["catalog_key"].as_str()),
            (Some(2111.9), Some("UK04.1"))
        );
        let exported = call(
            "export_dxf",
            json!({ "out_dir": format!("{dir}/out"), "signs": ["UK04.1"] }),
        );
        assert_eq!(exported["files"].as_array().unwrap().len(), 1);
        let checked = call(
            "validate_schedule",
            json!({ "csv_path": "examples/uk-directional.csv" }),
        );
        assert_eq!(
            (checked["valid"].as_bool(), checked["rows"].as_u64()),
            (Some(true), Some(20))
        );

        assert_eq!(
            call("document_new", json!({ "path": doc, "overwrite": false }))["elements"],
            0
        );
        let added = call(
            "document_add_sign",
            json!({ "path": doc, "key": "UK04.1", "mark": "ADS-001", "x_mm": 100, "y_mm": 200.5, "properties": { "Material": "RA2" } }),
        );
        assert_eq!(
            (
                added["id"].as_u64(),
                added["mark"].as_str(),
                added["y_mm"].as_f64()
            ),
            (Some(1), Some("ADS-001"), Some(200.5))
        );
        let definition = json!({ "kind": "DirectionSign", "speed_kph": 60, "background": "Green", "destinations": [{ "name": "Kasese", "direction": "Left" }] });
        assert_eq!(
            call(
                "document_add_sign",
                json!({ "path": doc, "definition": definition, "mark": "DS-001" })
            )["kind"],
            "direction"
        );
        let listed = call("document_list", json!({ "path": doc }));
        assert_eq!(
            (
                listed["count"].as_u64(),
                listed["elements"][0]["properties"]["Material"].as_str()
            ),
            (Some(2), Some("RA2"))
        );
        assert_eq!(
            call(
                "document_save",
                json!({ "path": doc, "save_as": format!("{dir}/copy.afrobim") })
            )["signs_checked"],
            2
        );
        let dxf = call(
            "document_export_dxf",
            json!({ "path": doc, "out_path": format!("{dir}/m.dxf"), "marks": ["ADS-001", "DS-001"] }),
        );
        assert_eq!(dxf["written"].as_array().unwrap().len(), 2);
        assert_eq!(
            call("document_remove_element", json!({ "path": doc, "id": 2 }))["elements"],
            1
        );
        assert_eq!(
            call(
                "document_remove_element",
                json!({ "path": doc, "mark": "ADS-001" })
            )["elements"],
            0
        );
    }

    #[test]
    fn a_missing_or_mistyped_argument_is_a_tool_error() {
        let mut tools = Tools::new(testing::settings());
        let specs = all();
        let mut fail = |name: &str, arguments: Value| -> String {
            let spec = specs.iter().find(|t| t.name == name).unwrap();
            (spec.call)(&mut tools, &Args(&args(arguments)))
                .unwrap_err()
                .0
        };
        assert_eq!(
            fail("export_dxf", json!({})),
            "missing required argument 'out_dir'"
        );
        assert_eq!(
            fail("validate_schedule", json!({ "csv_path": null })),
            "missing required argument 'csv_path'"
        );
        assert_eq!(
            fail(
                "layout_sign",
                json!({ "key": "UK04.1", "speed_kph": "fast" })
            ),
            "'speed_kph' must be an integer"
        );
        assert_eq!(
            fail("search_catalog", json!({ "query": 7 })),
            "'query' must be a string"
        );
        assert_eq!(
            fail("export_dxf", json!({ "out_dir": "x", "signs": "UK04.1" })),
            "'signs' must be an array of strings"
        );
        assert_eq!(
            fail(
                "document_new",
                json!({ "path": "x.afrobim", "overwrite": "yes" })
            ),
            "'overwrite' must be true or false"
        );
        assert_eq!(
            fail(
                "document_add_sign",
                json!({ "path": "x.afrobim", "key": "UK04.1", "x_mm": "left" })
            ),
            "'x_mm' must be a number"
        );
        assert_eq!(
            fail(
                "document_remove_element",
                json!({ "path": "x.afrobim", "id": -3 })
            ),
            "'id' must not be negative"
        );
    }
}
