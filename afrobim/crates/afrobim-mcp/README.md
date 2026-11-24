# afrobim-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) server that lets an AI assistant
drive the AfroBIM sign engine: search the sign catalog, lay out and check signs, write DXFs, and
build `.afrobim` documents the desktop app can open.

It is the Rust port of the C# `src/SignCore.Mcp` server. It speaks JSON-RPC 2.0 over stdio, one
message per line, with no async runtime and no dependencies beyond the workspace's own crates and
`serde_json`.

## Tools

Ported from the C# server, with the same names, parameters and result JSON:

| Tool | Arguments | Returns |
|------|-----------|---------|
| `list_packs` | none | the rule packs next to the default pack, which is the default, whether the fonts were found |
| `search_catalog` | `pack?`, `query?`, `series?` | catalog pages: key, family, series, scheme, description |
| `layout_sign` | `pack?`, `key` **or** `schedule_row`, `speed_kph?` | diagnostics (with clauses), size in mm, stroke width, x-height, primitive counts, SVG preview |
| `export_dxf` | `out_dir`, `pack?`, `signs` (catalog keys) **or** `csv_path` | one R12 DXF per sign, with each sign's diagnostics |
| `validate_schedule` | `csv_path`, `pack?` | per-row errors and warnings with clause citations |

New in this server, working on an `.afrobim` model file:

| Tool | Arguments | Does |
|------|-----------|------|
| `document_new` | `path`, `pack?`, `overwrite?` | creates an empty document with the pack as its design standard |
| `document_add_sign` | `path`, `key` **or** `schedule_row` **or** `definition`, `speed_kph?`, `mark?`, `x_mm?`, `y_mm?`, `properties?`, `pack?` | lays the sign out, adds it as a sign element, saves; returns its id, mark, position, size and diagnostics |
| `document_list` | `path` | reads the document: standard, layers, extents, and every element with its definition |
| `document_remove_element` | `path`, `id` **or** `mark` | removes one element and saves |
| `document_save` | `path`, `save_as?`, `pack?` | lays every sign out again, refreshes stored sizes, reports errors and warnings per sign, saves in place or as a copy |
| `document_export_dxf` | `path`, `out_path`, `marks?`, `pack?` | writes the document's signs to one R12 DXF, each at its model position |

The document tools are stateless: each call loads the file, works on it and saves it, so the
file on disk is always complete and the desktop app can open it between calls. A sign is stored
exactly as the desktop app stores one (category `sign`, layer `Signs`, the mark as the element
name, the `SignDefinition` as the element data).

Notes that apply to every tool:

- `pack` is a pack id from `list_packs` (`UK-TSM-2019` is the default), never a path.
- A `schedule_row` is one CSV data line in the schedule column order
  (`code,kind,speed_kph,approach_class,background,destinations,…`), for example
  `ADS-1,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0`.
- A `definition` is a sign definition object as `document_list` returns it, for example
  `{"kind":"StackType","speed_kph":60,"background":"Green","destinations":[{"name":"Kasese","direction":"Ahead"}]}`.
- Input schemas are strict: an argument the tool does not declare is refused.
- Every path a tool reads or writes must lie inside the project root (see "Project root and
  read-only mode" below), and relative paths resolve against that root.
- `csv_path` must be an existing `.csv` file. `export_dxf` writes only directly inside the
  `out_dir` it is given: sign codes are made into safe file names, a name that would land outside
  `out_dir` or on a link is refused, and duplicate codes get `-2`, `-3`, … suffixes. Document
  paths must end in `.afrobim` and DXF paths in `.dxf`.
- A tool that fails returns `isError: true` with the reason, so the assistant can correct the
  call. The server keeps running.

## Project root and read-only mode

The server only touches files inside a **project root**, so an assistant cannot read or overwrite
anything else on the machine.

| What | Flag | Environment variable | Default |
|------|------|----------------------|---------|
| Project root | `--root <folder>` | `AFROBIM_ROOT` | the server's working directory |
| Read-only mode | `--read-only` | `AFROBIM_READ_ONLY` (`1`, `true`, `yes` or `on`; `0`, `false`, `no` or `off`) | off |

The flag wins over the environment variable. A root that does not exist, or a read-only value
that is neither a yes nor a no, stops the server at start-up with a message and exit code 2
rather than leaving it less confined than you asked.

- **With no root given the working directory is the root.** The server says so on standard error
  when it starts (`project root: … (no --root or AFROBIM_ROOT given, so the working directory is
  used)`), together with the read-only state. A client that starts the server somewhere broad
  (a desktop client may start it in a folder unrelated to your project) should always be given
  `--root`; the server warns when the root is a whole drive.
- **Every path is resolved before it is used**: `.` and `..` are folded away, then the deepest
  part that exists is resolved through every symbolic link, and the result must lie inside the
  root. This covers `csv_path`, `out_dir`, and every document and DXF path (`path`, `save_as`,
  `out_path`). A path that escapes by `..`, by an absolute path or by a link that points outside is
  refused with a message naming the root.
- **Nothing is written through a link.** A write is refused when any existing part of the path
  below the root is a symbolic link, even one that points back inside the root, and a folder is
  never created before its path has been checked. Reading through a link is allowed when it ends
  inside the root. Write a path under the root as you gave it to `--root` (or as the system
  resolves it); a third spelling of the same folder is refused because it cannot be checked.
- **Read-only mode refuses every tool that writes**: `export_dxf`, `document_new`,
  `document_add_sign`, `document_remove_element`, `document_save` and `document_export_dxf`. The
  other tools work as usual. The refusal comes before the tool runs, and the path check refuses
  a write too if a tool were ever added without the right annotation.
- The rule pack and the fonts are server settings (`--pack`, `--fonts`, `AFROBIM_PACK`,
  `AFROBIM_FONTS`), not tool arguments, so they are not confined to the root. A pack is
  addressed by id only, never by path.
- The check and the use are two steps, so a second local process that swaps a folder for a link
  between them could win the race. The root is not a sandbox against other programs on the
  machine; it stops an assistant, not an attacker with a shell.

```
afrobim-mcp --root D:/projects/kasese-road --read-only
```

## Rule pack and fonts

The server needs a rule pack and the two Transport fonts the pack names. It looks for them the
way the desktop app does:

1. `--pack <file>` and `--fonts <folder>` on the command line;
2. otherwise the `AFROBIM_PACK` (pack file) and `AFROBIM_FONTS` (fonts folder) environment
   variables, when they name something that exists;
3. otherwise `packs/UK-TSM-2019.json` and `fonts/` in any folder above the executable, then
   above the working directory. Run from a checkout and they are found without configuration.

`list_packs`, `search_catalog`, `document_new` and `document_list` work without the fonts; the
other tools need them. If the pack or the fonts are missing the server still starts, and the
tools that need them say what to set. The fonts are third-party files and are not part of
AfroBIM.

Both pack formats load. A single-file JSON pack is read without its fonts. A folder pack such as
`UG-TSM-2004` (a folder holding `pack.xml`) is read the way the desktop app reads it, through the
sign engine (`afrobim-signs`, `xml_pack.rs`), so listing it, searching its catalog and laying out
its signs all need the Transport fonts; without them `list_packs` shows the reason against it.

## Build

```
cd afrobim
cargo build --release -p afrobim-mcp
```

The binary is `afrobim/target/release/afrobim-mcp` (`afrobim-mcp.exe` on Windows).

## Register it in Claude Code

From the repository root:

```
claude mcp add afrobim -- afrobim/target/release/afrobim-mcp
```

or add it to a project-scope `.mcp.json`:

```json
{
  "mcpServers": {
    "afrobim": {
      "type": "stdio",
      "command": "afrobim/target/release/afrobim-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Start `claude` in the repository root, approve the server when asked and check it with `/mcp`.
Claude Code starts the server in the folder it was started in, which is then the project root.
To choose another, add `"--root", "D:/projects/kasese-road"` to `args` (or set `AFROBIM_ROOT`),
and `"--read-only"` to forbid writes. To use a pack or fonts kept elsewhere, pass them as arguments
(`"args": ["--pack", "D:/standards/UK-TSM-2019.json", "--fonts", "D:/fonts/transport"]`) or set
`AFROBIM_PACK` and `AFROBIM_FONTS` in `env`.

## Register it in Claude Desktop

Claude Desktop does not read `.mcp.json`. Add the server to `claude_desktop_config.json`
(Settings → Developer → Edit Config) with an **absolute** path to the binary, then restart Claude
Desktop:

```json
{
  "mcpServers": {
    "afrobim": {
      "command": "D:/path/to/signcore/afrobim/target/release/afrobim-mcp.exe",
      "args": ["--fonts", "D:/path/to/signcore/fonts"]
    }
  }
}
```

The server finds `packs/` by walking up from the binary, so it does not depend on the folder
Claude Desktop starts it in. If you copy the binary out of the checkout, add
`"--pack", "D:/path/to/UK-TSM-2019.json"` to `args`. Always give
`--root` there (`"--root", "D:/projects/kasese-road"`), and give `out_dir`, `csv_path` and
document paths inside it, as absolute paths or relative to it.

## Example session

What you might ask: *"Find the UK stack-type signs, lay out a stack sign for Lampton ahead and
Slough to the left at 40 mph, put it in a new model as ADS-001 and export the model to DXF."*

What goes over the wire (requests from the client, then the server's reply; the SVG and long
strings are shortened here):

```
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"demo","version":"1"}}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"afrobim","version":"0.1.0"},"instructions":"AfroBIM lays out road signs …"}}
→ {"jsonrpc":"2.0","method":"notifications/initialized"}

→ {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_catalog","arguments":{"query":"UK04"}}}
← {"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"{\"pack\":\"UK-TSM-2019\",\"count\":2,\"entries\":[{\"key\":\"UK04.1\",\"family\":\"stack\",\"family_label\":\"Stack type\",\"series\":\"UK\",\"scheme\":\"primary\",\"description\":\"Stack-type ADS (S12-5-1 arrows, §4)\",\"symbol\":null}, …]}"}]}}

→ {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"layout_sign","arguments":{"schedule_row":"ADS-1,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0"}}}
← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"{\"pack\":\"UK-TSM-2019\",\"code\":\"ADS-1\",\"kind\":\"stack\",\"catalog_key\":null,\"width_mm\":1650.6,\"height_mm\":1125.0,\"sw_mm\":31.25,\"x_height_mm\":125.0,\"primitive_counts\":{\"PolyFill\":5,\"TextOutline\":2},\"has_errors\":false,\"diagnostics\":[],\"svg\":\"<svg …</svg>\"}"}]}}

→ {"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"document_new","arguments":{"path":"out/junction.afrobim"}}}
← {"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"{\"path\":\"/repo/out/junction.afrobim\",\"standard\":\"UK-TSM-2019\",\"elements\":0}"}]}}

→ {"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"document_add_sign","arguments":{"path":"out/junction.afrobim","schedule_row":"ADS-1,stack,64,primary,green,Lampton:Ahead:0;Slough:Left:0","mark":"ADS-001"}}}
← {"jsonrpc":"2.0","id":5,"result":{"content":[{"type":"text","text":"{\"path\":\"/repo/out/junction.afrobim\",\"id\":1,\"mark\":\"ADS-001\",\"kind\":\"stack\",\"catalog_key\":null,\"x_mm\":0.0,\"y_mm\":0.0,\"width_mm\":1650.6,\"height_mm\":1125.0,\"sw_mm\":31.25,\"x_height_mm\":125.0,\"has_errors\":false,\"diagnostics\":[],\"elements\":1}"}]}}

→ {"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"document_export_dxf","arguments":{"path":"out/junction.afrobim","out_path":"out/junction.dxf"}}}
← {"jsonrpc":"2.0","id":6,"result":{"content":[{"type":"text","text":"{\"path\":\"/repo/out/junction.afrobim\",\"out_path\":\"/repo/out/junction.dxf\",\"pack\":\"UK-TSM-2019\",\"written\":[{\"id\":1,\"mark\":\"ADS-001\",\"x_mm\":0.0,\"y_mm\":0.0,\"width_mm\":1650.6,\"height_mm\":1125.0}],\"skipped\":[],\"error_count\":0,\"warning_count\":0}"}]}}

→ {"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"layout_sign","arguments":{"key":"NOPE"}}}
← {"jsonrpc":"2.0","id":7,"result":{"content":[{"type":"text","text":"An error occurred invoking 'layout_sign': catalog sign page 'NOPE' not found in rule pack 'UK-TSM-2019'"}],"isError":true}}
```

`out/junction.afrobim` now opens in the desktop app (`cargo run -p afrobim-app -- out/junction.afrobim`).

You can try the server by hand: it reads one JSON message per line on standard input.

```
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | afrobim/target/release/afrobim-mcp
```

## Protocol

| Request | Answer |
|---------|--------|
| `initialize` | the client's protocol version when it is one of `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`, otherwise `2025-11-25`; `capabilities.tools`; `serverInfo`; `instructions` |
| `notifications/*` | nothing (notifications are never answered) |
| `ping` | `{}` |
| `tools/list` | `{"tools":[…]}`: name, title, description, `inputSchema` (with `additionalProperties: false`) and annotations |
| `tools/call` | `{"content":[{"type":"text","text":"<compact JSON>"}]}`; on failure the same with `"isError": true` and the reason as text |
| `tools/call` for an unknown tool, or without a `name` | error `-32602` |
| any other method | error `-32601` |
| a line that is not a JSON-RPC message | error `-32700` |

Standard output carries protocol messages only. Warnings go to standard error.

## How it compares with the C# server

For the five ported tools the two servers were sent the same 126 calls (every one of the 37 UK
catalog keys, 52 rows from the example schedules, the three example schedules through
`validate_schedule`, and the error cases). Every result was identical field for field, including
sizes, stroke widths, x-heights, primitive counts, diagnostics with their clauses, error texts
and the SVG text, and the 86 DXF files written were byte-identical.

Differences that remain:

- `quantities` is not here yet. It needs the quantities engine, which is being ported in
  `afrobim-fabrication`.
- The Uganda pack (`UG-TSM-2004`, an XML folder pack) loads through the sign engine, so unlike a
  JSON pack its catalog needs the fonts folder too.
- Fonts come from `AFROBIM_FONTS` / `--fonts`, not `SIGNCORE_FONTS`, and `list_packs` returns
  `fonts_dir: null` when no fonts folder was found.
- `initialize` reports the server as `afrobim` and advertises only the `tools` capability (the
  C# SDK also advertises `logging` and `tools.listChanged`, which this server does not
  implement).
- A line that is not JSON gets a `-32700` error with a null id; the C# server logs it and stays
  silent. A `tools/call` without a tool name is `-32602` here and `-32603` there.
- A wrongly typed or missing argument is reported with its name (`'speed_kph' must be an
  integer`); the C# server only says that an error occurred.
- A negative `speed_kph` in a schedule row is refused when the row is read.

## Tests

```
cd afrobim
cargo test -p afrobim-mcp
```

Unit tests call every tool handler directly and through its JSON binding; `tests/stdio.rs`
starts the built binary and runs whole sessions over stdio.
