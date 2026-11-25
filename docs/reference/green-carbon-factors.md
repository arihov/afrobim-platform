# Green BIM carbon factors: ICE licence finding

Checked on 25 November 2026 by reading the Circular Ecology pages below. This is a reading of the published terms, not legal advice.

## Decision and finding

The owner decided (open question 4 of the audit and feature plan) that ICE is the default source: cradle to gate A1 to A3, A4 added only where a haul distance is entered, with manufacturers' EPDs or a client's own figures overriding ICE per material.

**Finding: ICE values may not be redistributed inside AfroBIM.** So AfroBIM ships no ICE numbers. It ships the material list, ICE sheet references and an import action that reads the user's own ICE export.

| Point | Terms found | Source |
|---|---|---|
| Current version | ICE Database V5.0 (June 2026); earlier V4.1 (October 2025) and V4.0 (December 2024) | https://circularecology.com/embodied-carbon-footprint-database.html |
| Educational edition | Not for "commercial application of any kind" (16.6); not for use "in tools, software, platforms, databases, APIs, models, calculators, plugins" (16.8); the page also states that non-educational use of the Educational database is not permitted after 30 September 2026 | https://circularecology.com/terms-of-use.html (ICE terms updated 25 June 2026) |
| Individual licence (free registration) | "This licence does not allow the data to be integrated into software, tools, databases, platforms" (17.9.1) | same |
| Tools and Integration Licence | Required to integrate ICE data in software; "may only be distributed through the registered software, tool, platform or resource" (17.10.1). Its registration page (circularecology.com/register-ice-tools-and-integration.html) returned 404 on the day, so its price and conditions were not read | same |
| General | Reproduction or distribution of data without prior consent is prohibited (5.2.1); full attribution to Circular Ecology Ltd is required (5.1.1); a link to the official download page is required for non-educational use (17.16.2) | same |
| Commercial successor | IC+ is a paid dataset for use in software and tools; public distribution of IC+ factors is not permitted | https://circularecology.com/ic-plus.html |

The plan named "ICE v3.0 or v4". Neither is current: V5.0 is. The reference below therefore says "ICE V5.0" wherever the user states that edition.

## What AfroBIM ships instead

- **No ICE values** in the repository, the binary or any default factor file.
- **Save ICE template** writes a factor file with one row per material key the model needs. Each row's `material` is the ICE material name to look up (a search hint, never a number) and its `source` names the ICE sheet. Confirm or edit these names before importing.
- **Import ICE factors** reads the CSV the user exports from their own licensed ICE spreadsheet and fills the blank rows by name. Rules: the ICE column must be in kgCO2e per kg (converted to per tonne); the A1-A3 column is preferred when several exist; exactly one ICE row must match a name, otherwise the row stays blank (NaN, "not entered") and the reason is shown; a row that already has a figure (the user's own, an EPD, a client figure) is never overwritten, which is how overrides win per material; densities are not imported.
- The window and the CSV export show `Source: ICE v5.0 (Circular Ecology), A1-A3`, built from the file's own source, edition and boundary, plus a count of materials that use their own source.
- A4 transport stays as the user's `transport_kgco2e_per_tkm` setting and per-rule haul distance.

## Whose licence applies

The user must hold a licence that lets them use ICE data in their own project calculations (the free Individual Licence covers one named person; a firm should check the terms). AfroBIM neither grants nor checks it. Attribution to Circular Ecology Ltd and the ICE edition travel with the imported rows (`source`, `edition`).

## Transport (A4) default table

None shipped. A citable candidate is the UK Government GHG conversion factors for company reporting (DESNZ, Open Government Licence) for heavy goods vehicles in kgCO2e per tonne-km, but its figures could not be fetched and checked in this session, so none are entered. Enter the figure and year from the edition you cite.

## Open item for the owner

To ship real default numbers, buy or register for the Tools and Integration Licence or IC+ and record its terms here. The loader needs no change: the same CSV format would be shipped as `afrobim-green/data/`.
