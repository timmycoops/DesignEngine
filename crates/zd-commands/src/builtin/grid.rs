use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zd_core::{AttrValue, Attributes, Body, Change, ObjectKind};
use zd_io::read_vulcan_seam;

use crate::{Command, CommandError, CommandSpec, Field, FieldType, Inputs, Outcome};

/// `grid.import_vulcan_seam`: every grid of one seam from a folder of Vulcan grids
/// (`<prefix><seam>.<variable>g`), each as a grid object named "<SEAM> <variable>" (e.g. "UPPER sr")
/// with its seam, variable, header text and file name as attributes. Roof and floor must read;
/// grids that cannot be read (compressed ones, for now) are skipped and listed.
pub struct ImportVulcanSeam;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportVulcanSeamParams {
    /// The folder holding the grids.
    pub dir: String,
    /// The project prefix on every file name, e.g. "pit".
    pub prefix: String,
    /// The seam code, e.g. "upper".
    pub seam: String,
    pub layer: String,
}

impl Command for ImportVulcanSeam {
    type Params = ImportVulcanSeamParams;
    const NAME: &'static str = "grid.import_vulcan_seam";

    fn spec(&self) -> CommandSpec {
        CommandSpec {
            name: Self::NAME,
            summary: "Import a seam's Vulcan grids: roof, floor, thickness and qualities",
            inputs: vec![],
            params: vec![
                Field::new("dir", FieldType::Path, "Folder of Vulcan grids"),
                Field::new("prefix", FieldType::Text, "Project prefix on the file names, e.g. pit"),
                Field::new("seam", FieldType::Text, "Seam code, e.g. upper"),
                Field::new("layer", FieldType::Layer, "Layer for the grids"),
            ],
            outputs: vec![Field::object("grids", ObjectKind::Grid, "One grid per variable (sr, sf, tk, ra, ...)")],
        }
    }

    fn run(&self, _inputs: &Inputs<'_>, p: &Self::Params) -> Result<Outcome, CommandError> {
        let seam = read_vulcan_seam(Path::new(&p.dir), &p.prefix, &p.seam).map_err(|e| CommandError::Io(e.to_string()))?;
        let code = seam.seam.to_ascii_uppercase();
        let mut outcome = Outcome::default()
            .message("Vulcan grid layout is unverified until a grid matches an independent export node for node (decision 0006)");
        for (variable, vulcan) in seam.grids {
            let log = &vulcan.log;
            let mut line = format!("{variable}: {} of {} nodes live", log.live, log.nodes);
            if log.stale_nulls + log.odd_flags + log.non_finite > 0 {
                line += &format!(
                    "; null nodes holding old values {}, unknown flags {}, non-finite values {}",
                    log.stale_nulls, log.odd_flags, log.non_finite
                );
            }
            outcome = outcome.message(line);
            let mut attributes = Attributes::new();
            attributes.insert("seam".into(), AttrValue::Text(code.clone()));
            attributes.insert("variable".into(), AttrValue::Text(variable.clone()));
            attributes.insert("vulcan_header".into(), AttrValue::Text(vulcan.description));
            attributes.insert("source".into(), AttrValue::Text(format!("{}{}.{variable}g", p.prefix, p.seam).to_ascii_lowercase()));
            outcome.changes.push(Change::Create {
                name: format!("{code} {variable}"),
                layer: p.layer.clone(),
                attributes,
                body: Body::Grid(Arc::new(vulcan.grid)),
            });
        }
        for (variable, reason) in seam.skipped {
            outcome = outcome.message(format!("skipped {variable}: {reason}"));
        }
        Ok(outcome)
    }
}
