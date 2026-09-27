//! The `tk feature` commands.

use serde::Serialize;

use super::{FeatureView, Tally, listed_projects, open_store};
use crate::env::Env;
use crate::error::Error;
use crate::model::{Project, validate_name};
use crate::scope::{self, Request, Scope};
use crate::store::Tx;

/// `feature ls` data: `{scope, features}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureList {
    /// The resolved scope.
    pub scope: Scope,
    /// Features ordered by project, then name.
    pub features: Vec<FeatureView>,
}

/// `feature mv` data: `{feature, merged, moved}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureMove {
    /// The old feature name, for the human text.
    #[serde(skip)]
    pub from: String,
    /// The feature under its new name, with its counts after the move.
    pub feature: FeatureView,
    /// Whether the old feature was merged into an existing one.
    pub merged: bool,
    /// Number of tasks that were in the old feature and are now in the new one.
    pub moved: i64,
}

fn features_of(tx: &Tx<'_>, project: &Project) -> Result<Vec<FeatureView>, Error> {
    let tally = Tally::of(tx, project.id)?;
    Ok(tx
        .features(project.id)?
        .into_iter()
        .map(|feature| {
            let (open, total) = tally.feature(feature.id);
            FeatureView {
                project: project.name.clone(),
                name: feature.name,
                open,
                total,
            }
        })
        .collect())
}

/// `tk feature ls`: the features of the resolved scope with open and total counts.
pub fn list(env: &Env, request: Request<'_>) -> Result<FeatureList, Error> {
    open_store(env)?.read(|tx| {
        let scope = scope::resolve(tx, env, request)?;
        let mut features = Vec::new();
        for project in &listed_projects(tx, &scope)? {
            features.extend(features_of(tx, project)?);
        }
        Ok(FeatureList { scope, features })
    })
}

/// `tk feature mv`: renames `old` to `new`, or, when `new` exists, moves every task of
/// `old` to it and deletes `old`.
pub fn rename(env: &Env, old: &str, new: &str, request: Request<'_>) -> Result<FeatureMove, Error> {
    validate_name("feature name", new)?;
    open_store(env)?.write(|tx| {
        let project = scope::resolve(tx, env, request)?.require()?;
        let source = tx.feature_by_name(project.id, old)?.ok_or_else(|| {
            Error::NotFound(format!(
                "no feature named {old:?} in project {}",
                project.name
            ))
        })?;
        if old == new {
            return Err(Error::Usage(format!(
                "feature {old} is already named {new}"
            )));
        }
        let (merged, moved) = if let Some(target) = tx.feature_by_name(project.id, new)? {
            let moved = tx.move_feature_tasks(source.id, target.id)?;
            tx.delete_feature(source.id)?;
            let moved = i64::try_from(moved)
                .map_err(|err| Error::Internal(format!("task count out of range: {err}")))?;
            (true, moved)
        } else {
            tx.rename_feature(source.id, new)?;
            (false, Tally::of(tx, project.id)?.feature(source.id).1)
        };
        let feature = features_of(tx, &project)?
            .into_iter()
            .find(|feature| feature.name == new)
            .ok_or_else(|| Error::Internal(format!("feature {new} vanished during the move")))?;
        Ok(FeatureMove {
            from: source.name,
            feature,
            merged,
            moved,
        })
    })
}
