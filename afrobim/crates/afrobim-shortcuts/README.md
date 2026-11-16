# afrobim-shortcuts

Data shortcuts for AfroBIM, modelled on Civil 3D data shortcuts and references. A team splits a
road project into several AfroBIM documents (survey and existing ground; alignments and profiles;
corridor; drainage; signs and markings). Each document references named objects from the others,
read-only, and is told when a source changed.

## Concepts

- **Shortcut project**: a folder with one small manifest file, `afrobim-shortcuts.json`. The
  documents can be in the folder, in sub-folders, or elsewhere.
- **Shortcut**: one shared object. It has a key, the object's name and category, the source
  document, the element id in it, a content hash, a revision counter, who and when (text the
  caller supplies; the crate reads no clock and no user name), an optional note, a status label
  and the source's coordinate system.
- **Publish**: put an element of a document into the manifest, or update its entry. The hash is
  taken at that moment. The revision goes up only when the content changed.
- **Reference**: a copy of a published object inside a host document. It is an ordinary element
  of the source's own category, at the source's own coordinates, with the source's `data`, so the
  host opens and draws without the source present. It is marked as a reference and locked.
- **Synchronise**: refresh the copies of references whose shortcut was published again, and say
  what changed.
- **Promote**: turn a reference into an independent, editable element.
- **Document reference**: attach a whole document as an underlay.

A reference follows the published state, not the source file. If the source holds edits that were
not published, status says so (`unpublished_changes`) and synchronise skips that reference.

## Rules the crate keeps

- A reference never modifies its source file or the manifest.
- Nothing is moved or transformed. Referenced objects appear at their source coordinates. If the
  source declares a coordinate system different from the host's, the kernel's `CrsCheck` is run
  and its warnings are reported (`crs_differs`, `crs_messages`, `CreateReport::crs_messages`).
  Nothing else happens.
- Changes are reported, not silent: publish returns what it did (created, updated, unchanged, and
  what the shortcut used to point at); synchronise returns a field-level summary per reference.
- Paths are stored with forward slashes, relative to the project folder when the file is inside
  it, absolute only when it is outside. They work on Windows, macOS and Linux.

## Workflow

```text
survey team:   publish "Existing ground"            -> manifest revision 1
roads team:    create_reference(roads, "civil-surface:Existing ground")
               add alignment + ground profile
               publish_closure(profile)              -> alignment and profile published
corridor team: create_reference(corridor, "civil-profile:Main ground")
                 -> brings the profile, its alignment and its surface, ids rewritten
survey team:   new survey, publish again             -> revision 2
anyone:        stale(manifest, recorded_references(host))     (cheap, opens no file)
               status(host, project)                          (up to date / out of date / ...)
               synchronise(host, project, All)                (refresh, report the differences)
```

The application calls `check_editable(doc, id)` before it modifies, moves, erases or recomputes
an element, and `locked_among` for a selection. After a synchronise it rebuilds what it derived
from `SyncReport::changed_elements()`. As a help, each reference gets the property named by
`ReferenceOptions::revision_property` (default `civil.rev`) set to a value derived from its data,
so a scene cache keyed on it is rebuilt.

## Public API (main entry points)

| Area | Items |
|---|---|
| Project | `ShortcutProject::{create, open, save, list, get, publish, publish_closure, remove, set_status, repair_source_path, validate}` |
| References | `create_reference`, `status`, `synchronise`, `promote`, `check_editable`, `locked_among`, `set_locked`, `reference_info`, `is_reference`, `promoted_from` |
| Dependencies | `DependencyTable::{civil, empty, add, from_json, to_json}`, `DependencyRule`, `local_closure` |
| Stale checks | `recorded_references`, `stale`, `changed_sources`, `FileStamp::of`, `file_hash` |
| Whole documents | `attach_document`, `detach_document`, `set_document_view`, `document_status`, `refresh_document`, `load_underlay`, `document_hash` |
| Hashing | `canonical_json`, `element_hash`, `value_hash`, `sha256_hex` |

## Content hash

`element_hash` is SHA-256 (the `sha2` crate, MIT or Apache-2.0) of canonical JSON of
`{category, name, origin, width, height, site, data}`. Canonical JSON sorts object keys, writes no
whitespace, and writes numbers in one form (an integer when the value is whole and below 2^53,
else the shortest decimal that reads back to the same number). Saving an unchanged source again,
reordering keys or writing `1.0` as `1` does not change the hash. The layer, the host id and the
free-form properties are not part of it.

## Dependency table

Which fields of an element's `data` name other objects is data (`DependencyTable`), so the
application can extend it with `add` or load a table from JSON. Paths are dot separated; `[]` walks
an array. The built-in `DependencyTable::civil()`:

| Category | Field | By | Target categories | Required |
|---|---|---|---|---|
| `civil-profile` | `alignment` | name | `alignment` | yes |
| `civil-profile` | `surface` | id | `civil-surface`, `surface` | yes |
| `civil-profile-view` | `alignment` | name | `alignment` | yes |
| `civil-profile-view` | `profiles[]` | id | `civil-profile` | yes |
| `civil-corridor` | `model.baselines[].alignment` | name | `alignment` | yes |
| `civil-corridor` | `model.baselines[].regions[].assembly.name` | name | `civil-assembly` | no |
| `civil-corridor` | `ground` | id | `civil-surface`, `surface` | yes |
| `civil-samples` | `group.alignment` | name | `alignment` | yes |
| `civil-samples` | `corridor` | id | `civil-corridor` | no |
| `civil-samples` | `surfaces[]` | id | `civil-surface`, `surface` | yes |
| `corridor` | `alignment` | id | `alignment` | yes |
| `corridor` | `surface` | id | `surface` | yes |

A field that names an element is followed to the shortcut that publishes it. A dependency that is
itself a reference in the source document is followed to its original shortcut. A required
dependency that is not published refuses the reference and names it. An optional one is skipped and
reported in `CreateReport::notes`. In the host copy, id fields are rewritten to the id of the host's
reference to that element (the source's id means nothing in the host); names are copied as they are.
`ReferenceInfo::id_map` records the mapping. Everything else in `data` is copied verbatim.

## File formats

### Manifest, `afrobim-shortcuts.json`, version 1

```json
{
  "format": "afrobim-shortcuts",
  "version": 1,
  "name": "N5 upgrade",
  "shortcuts": [
    {
      "key": "civil-surface:Existing ground",
      "name": "Existing ground",
      "category": "civil-surface",
      "source_path": "survey/ground.afrobim",
      "source_element_id": 2,
      "hash": "9f2c...64 hex digits",
      "revision": 3,
      "published_by": "free text",
      "published_at": "free text",
      "note": "optional",
      "status": "published",
      "file_stamp": { "size": 18234, "modified_ms": 1731747600000 },
      "source_crs": { "...": "the kernel ProjectCrs of the source, when it declares one" }
    }
  ]
}
```

`status` is one of `work-in-progress`, `shared`, `published`, `superseded`, `archived`: plain
labels after the common data environment states. Nothing changes behaviour with them. A manifest
with a newer `version` than this program reads is refused. The file is written to a temporary
name and renamed over the old one.

### Reference, in the host element's `properties`

- `shortcut.reference`: JSON `ReferenceInfo`: `shortcut_key`, `source_path`, `source_element_id`,
  `source_hash`, `source_revision`, `source_name`, `source_category`, `source_layer`, `source_crs`,
  `locked`, `synced_by`, `synced_at`, `id_map`. Version 1 of the reference format.
- `shortcut.locked`: `"1"` while locked.
- `shortcut.promoted_from`: on a promoted element, JSON `PromotedFrom` (key, path, element id,
  hash, revision, who, when).

The reference's layer is the source layer with the prefix `REF-`. Source properties are copied,
except `civil.rev` and any `shortcut.*`.

### Document references, in the host's project settings

The key `document_references` holds a list of `DocumentReference`: `name`, `path`, `hash` (see
`document_hash`), `stamp`, `layers` (empty: all), `locked`, `source_crs`, `element_count`,
`extents`, `attached_by`, `attached_at`. `load_underlay` reads the document, restricted to those
layers, at its own coordinates, without the project settings element. The document is never copied
into the host or modified.

## Tests

`cargo test -p afrobim-shortcuts` builds two or three small documents through the kernel API in
temporary folders and covers: canonical hash stability, publish, closure and id rewriting for
profile, corridor and an application-extended rule, locking and promotion, status in every state,
synchronise with field and bounding-box summary, unpublished edits, repair of a moved source,
validation, stale checks, document references, and that sources and manifest are never modified.

## Limits

- **No file locking.** Two people publishing at once, or a publish while another program writes
  the source, are not coordinated.
- **No merge.** A reference is replaced as a whole on synchronise; edits made to a reference
  (after unlocking it) are lost. Promote to edit.
- **Last publisher wins.** Publishing under an existing key replaces what it points at; the
  revision counter goes up and the outcome reports the previous source. Two writers of the
  manifest at once can overwrite each other, since the whole file is rewritten.
- **No network or cloud store.** A project is a folder on a disk or a share the machine can read.
- **No watching.** The application decides when to call `status`, `stale` or `changed_sources`.
- **No transformation.** A reference in a different coordinate system is reported and left as it
  is. There is no reprojection and none is offered.
- Dependencies are followed through the fields in the table only. Ground profiles that belong to
  an alignment (the reverse direction) are not pulled in by an alignment or a corridor; reference
  them as well. An assembly stored inline in a corridor travels with the corridor's data.
- Dependencies are looked up inside the source document. A source element that points at an
  element of another document is only followed when that element is a reference in the source.
- A new dependency added to a source after a reference was made is reported in
  `ChangeSummary::missing_dependencies`; it is not added to the host automatically.
- The element's own `host` link (a post hosted by a sign) is not copied.
- A shortcut is one element plus what the table says it depends on. Elements are located by id
  and category; if a source document is rebuilt with new ids, repair is manual (publish again).
- Whole-document status hashes the document when the file stamp changed, so a very large file is
  read in full then. Stamps are size and modified time in milliseconds; a copy that restores an old
  modified time with the same size is not noticed by the stamp test (use `file_hash` or
  `use_stamp = false`).
- Hashes are SHA-256 of the canonical content. They detect change, not who made it.
