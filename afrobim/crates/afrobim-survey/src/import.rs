// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! Import events: parse a text with one of the readers, merge the result into a network, record what
//! was added so the event can be removed or reprocessed.

use serde::{Deserialize, Serialize};

use crate::csv::{parse_csv, CsvMapping};
use crate::error::{Result, SurveyError};
use crate::fbk::{parse_fbk, FbkOptions};
use crate::fieldbook::parse_fieldbook;
use crate::figures::{process_linework, Figure};
use crate::gsi::{parse_gsi, GsiOptions};
use crate::hash::sha256_hex;
use crate::model::*;
use crate::topcon::{parse_topcon, TopconOptions};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ImportFormat {
    /// The field book format of this crate (`fieldbook.rs`).
    Fieldbook,
    /// Autodesk Civil 3D field book (.fbk) command subset (`fbk.rs`).
    Fbk(FbkOptions),
    /// Leica GSI-8 and GSI-16 (`gsi.rs`).
    Gsi(GsiOptions),
    /// Topcon GTS-7 raw records, structure only (`topcon.rs`).
    Topcon(TopconOptions),
    /// Delimited observation table with a column mapping (`csv.rs`).
    Csv(CsvMapping),
}

/// What a reader returns, before ids are assigned and the data is merged.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedData {
    pub points: Vec<SurveyPoint>,
    pub directions: Vec<KnownDirection>,
    pub setups: Vec<Setup>,
    pub level_runs: Vec<LevelRun>,
    pub baselines: Vec<Baseline>,
    pub figures: Vec<Figure>,
    pub warnings: Vec<String>,
}

pub fn parse_with(format: &ImportFormat, text: &str) -> Result<ParsedData> {
    match format {
        ImportFormat::Fieldbook => parse_fieldbook(text),
        ImportFormat::Fbk(o) => parse_fbk(text, o),
        ImportFormat::Gsi(o) => parse_gsi(text, o),
        ImportFormat::Topcon(o) => parse_topcon(text, o),
        ImportFormat::Csv(m) => parse_csv(text, m),
    }
}

/// What to do when an imported point has the name of a point already in the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicatePolicy {
    /// Refuse the whole import.
    Error,
    /// Keep the existing point, ignore the imported one (a warning is recorded).
    Skip,
    /// Replace the existing point (the change log keeps the old coordinates).
    Replace,
}

impl SurveyDatabase {
    /// Import `text` into `network`. `time` is the time stamp the caller gives (this crate reads no
    /// clock). Returns the id of the new import event. Nothing is changed when the import fails.
    #[allow(clippy::too_many_arguments)]
    pub fn import_text(&mut self, network: &str, format: ImportFormat, text: &str, file_name: &str, time: &str, description: &str, keep_source: bool, duplicates: DuplicatePolicy) -> Result<u64> {
        let parsed = parse_with(&format, text)?;
        self.network(network)?;
        let mut trial = self.clone();
        let id = trial.fresh_id();
        let mut warnings = parsed.warnings.clone();
        let mut added = AddedItems::default();
        let prefixes = trial.figure_prefixes.clone();
        {
            let net = trial.network_mut(network)?;
            for mut p in parsed.points {
                p.event = Some(id);
                match net.points.iter().position(|q| q.name == p.name) {
                    None => {
                        added.points.push(p.name.clone());
                        net.points.push(p);
                    }
                    Some(i) => match duplicates {
                        DuplicatePolicy::Error => return Err(SurveyError::Invalid(format!("point '{}' already exists in network '{network}'", p.name))),
                        DuplicatePolicy::Skip => warnings.push(format!("point '{}' already exists and was kept", p.name)),
                        DuplicatePolicy::Replace => {
                            let old = net.points[i].clone();
                            warnings.push(format!("point '{}' replaced (was E {:?} N {:?} Z {:?})", p.name, old.e, old.n, old.z));
                            added.points.push(p.name.clone());
                            net.points[i] = p;
                        }
                    },
                }
            }
            for mut d in parsed.directions {
                d.event = Some(id);
                added.directions += 1;
                net.directions.push(d);
            }
            for mut b in parsed.baselines {
                b.event = Some(id);
                added.baselines += 1;
                net.baselines.push(b);
            }
            for mut f in parsed.figures {
                f.event = Some(id);
                if f.layer.is_empty() && f.style.is_empty() {
                    let (l, s, b) = (f.layer.clone(), f.style.clone(), f.breakline);
                    f.apply_prefix(&prefixes);
                    if f.layer.is_empty() && f.style.is_empty() {
                        f.layer = l;
                        f.style = s;
                        f.breakline = b;
                    } else {
                        f.breakline = f.breakline || b;
                    }
                }
                added.figures.push(f.name.clone());
                net.figures.push(f);
            }
            let mut obs_counter = 0usize;
            let mut setups = parsed.setups;
            let mut runs = parsed.level_runs;
            for s in setups.iter_mut() {
                s.event = Some(id);
                obs_counter += s.observations.len();
            }
            for r in runs.iter_mut() {
                r.event = Some(id);
            }
            added.observations = obs_counter;
            net.setups.extend(setups);
            net.level_runs.extend(runs);
        }
        // ids are assigned after the borrow of the network ends
        let net_idx = trial.networks.iter().position(|n| n.name == network).unwrap();
        let mut setup_ids = Vec::new();
        let mut run_ids = Vec::new();
        for si in 0..trial.networks[net_idx].setups.len() {
            if trial.networks[net_idx].setups[si].event == Some(id) {
                let sid = trial.fresh_id();
                trial.networks[net_idx].setups[si].id = sid;
                setup_ids.push(sid);
                for oi in 0..trial.networks[net_idx].setups[si].observations.len() {
                    let oid = trial.fresh_id();
                    trial.networks[net_idx].setups[si].observations[oi].id = oid;
                }
            }
        }
        for ri in 0..trial.networks[net_idx].level_runs.len() {
            if trial.networks[net_idx].level_runs[ri].event == Some(id) {
                let rid = trial.fresh_id();
                trial.networks[net_idx].level_runs[ri].id = rid;
                run_ids.push(rid);
            }
        }
        added.setups = setup_ids;
        added.level_runs = run_ids;
        trial.import_events.push(ImportEvent {
            id,
            file_name: file_name.into(),
            sha256: sha256_hex(text.as_bytes()),
            imported_at: time.into(),
            format,
            network: network.into(),
            description: description.into(),
            added: added.clone(),
            warnings,
            source_text: if keep_source { Some(text.to_string()) } else { None },
        });
        trial.log(
            time,
            "import",
            &format!("event {id}: {file_name} into {network}: {} points, {} setups, {} observations, {} level runs, {} figures", added.points.len(), added.setups.len(), added.observations, added.run_count(), added.figures.len()),
        );
        *self = trial;
        Ok(id)
    }

    /// Remove everything an import event added and the event itself. Points that other observations
    /// still reference are removed too (their observations stay and will fail to reduce; remove those
    /// events as well).
    pub fn remove_import_event(&mut self, id: u64, time: &str) -> Result<()> {
        let pos = self.import_events.iter().position(|e| e.id == id).ok_or_else(|| SurveyError::Invalid(format!("import event {id} does not exist")))?;
        let ev = self.import_events.remove(pos);
        if let Some(net) = self.networks.iter_mut().find(|n| n.name == ev.network) {
            net.points.retain(|p| p.event != Some(id));
            net.directions.retain(|d| d.event != Some(id));
            net.setups.retain(|s| s.event != Some(id));
            net.level_runs.retain(|r| r.event != Some(id));
            net.baselines.retain(|b| b.event != Some(id));
            net.figures.retain(|f| f.event != Some(id));
        }
        self.log(time, "remove import event", &format!("event {id}: {}", ev.file_name));
        Ok(())
    }

    /// Remove an event and import its stored text again, optionally with other reader options. Needs
    /// the source text to have been kept. Returns the id of the new event. The old event is restored
    /// when the new import fails.
    pub fn reprocess_import_event(&mut self, id: u64, new_format: Option<ImportFormat>, time: &str, duplicates: DuplicatePolicy) -> Result<u64> {
        let ev = self.import_events.iter().find(|e| e.id == id).cloned().ok_or_else(|| SurveyError::Invalid(format!("import event {id} does not exist")))?;
        let text = ev.source_text.clone().ok_or_else(|| SurveyError::Invalid(format!("import event {id} has no stored source text (it was imported without keep_source)")))?;
        let backup = self.clone();
        self.remove_import_event(id, time)?;
        let format = new_format.unwrap_or(ev.format.clone());
        match self.import_text(&ev.network, format, &text, &ev.file_name, time, &ev.description, true, duplicates) {
            Ok(nid) => {
                self.log(time, "reprocess import event", &format!("event {id} became {nid}"));
                Ok(nid)
            }
            Err(e) => {
                *self = backup;
                Err(e)
            }
        }
    }

    /// Generate figures from the descriptions of the points of a network with the linework code set
    /// and figure prefix table of the database. Figures made by an earlier run are replaced. Points are
    /// processed in the order of the network's point list. Returns the warnings.
    pub fn generate_figures(&mut self, network: &str, time: &str) -> Result<Vec<String>> {
        let codes = self.linework.clone();
        let prefixes = self.figure_prefixes.clone();
        let net = self.network_mut(network)?;
        let list: Vec<(String, String)> = net.points.iter().map(|p| (p.name.clone(), p.description.clone())).collect();
        let (mut figs, warnings) = process_linework(&list, &codes, &prefixes);
        for f in figs.iter_mut() {
            f.from_linework = true;
        }
        net.figures.retain(|f| !f.from_linework);
        let n = figs.len();
        net.figures.extend(figs);
        self.log(time, "generate figures", &format!("{network}: {n} figures"));
        Ok(warnings)
    }
}

impl AddedItems {
    fn run_count(&self) -> usize {
        self.level_runs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figures::FigurePrefix;

    const FB: &str = "AFROFB 1\nUNITS DIST=m ANGLE=deg\nCONTROL A 500000 9960000 100\nCONTROL B 500100 9960000 100\nSETUP A 1.5 BS=B\nOBS P1 HZ=90 HZKIND=angle HD=50 DESC=EP1 B\nOBS P2 HZ=100 HZKIND=angle HD=60 DESC=EP1\n";

    #[test]
    fn import_remove_reprocess_and_hash() {
        let mut db = SurveyDatabase::new("j");
        db.add_network("N", "t0").unwrap();
        let id = db.import_text("N", ImportFormat::Fieldbook, FB, "a.afrofb", "t1", "first", true, DuplicatePolicy::Error).unwrap();
        let ev = &db.import_events[0];
        assert_eq!(ev.sha256, sha256_hex(FB.as_bytes()));
        assert_eq!(ev.added.points, ["A", "B"]);
        assert_eq!(ev.added.observations, 2);
        assert_eq!(db.network("N").unwrap().setups.len(), 1);
        assert!(db.network("N").unwrap().setups[0].id > 0);
        let ids: Vec<u64> = db.network("N").unwrap().setups[0].observations.iter().map(|o| o.id).collect();
        assert!(ids[0] > 0 && ids[1] > ids[0]);
        // duplicate points refuse the import and leave the database unchanged
        let before = db.clone();
        assert!(db.import_text("N", ImportFormat::Fieldbook, FB, "a.afrofb", "t2", "", false, DuplicatePolicy::Error).is_err());
        assert_eq!(db, before);
        // reprocess restores equal content under a new event id
        let nid = db.reprocess_import_event(id, None, "t3", DuplicatePolicy::Error).unwrap();
        assert_ne!(nid, id);
        assert_eq!(db.network("N").unwrap().points.len(), 2);
        db.remove_import_event(nid, "t4").unwrap();
        let net = db.network("N").unwrap();
        assert!(net.points.is_empty() && net.setups.is_empty());
        assert!(db.import_events.is_empty());
        assert!(db.change_log.len() >= 5);
        assert!(db.reprocess_import_event(99, None, "t", DuplicatePolicy::Error).is_err());
    }

    #[test]
    fn linework_from_descriptions() {
        let mut db = SurveyDatabase::new("j");
        db.add_network("N", "t0").unwrap();
        db.figure_prefixes.push(FigurePrefix { prefix: "EP".into(), style: "Edge".into(), layer: "V-EDGE".into(), breakline: true });
        let net = db.network_mut("N").unwrap();
        for (i, d) in ["EP1 B", "EP1", "EP1 E"].iter().enumerate() {
            let mut p = SurveyPoint::given(&format!("{}", i + 1), 500000.0 + i as f64, 9_960_000.0, Some(1.0));
            p.description = d.to_string();
            net.points.push(p);
        }
        let w = db.generate_figures("N", "t").unwrap();
        assert!(w.is_empty());
        db.generate_figures("N", "t").unwrap();
        let f = &db.network("N").unwrap().figures;
        assert_eq!(f.len(), 1, "regeneration replaces earlier generated figures");
        assert!(f[0].breakline);
        assert_eq!(f[0].layer, "V-EDGE");
    }
}

