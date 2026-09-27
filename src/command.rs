//! Command implementations: each opens the store, runs one transaction and returns data
//! for the output module.

use crate::env::Env;
use crate::error::Error;
use crate::model::Project;
use crate::store::Store;

/// Opens the database at the location the environment resolves, relative paths from the
/// captured current directory.
fn open_store(env: &Env) -> Result<Store, Error> {
    Store::open(&env.current_dir().join(env.database_path()?))
}

/// `tk project ls`: the projects that are not archived, ordered by name.
pub fn project_list(env: &Env) -> Result<Vec<Project>, Error> {
    open_store(env)?.read(|tx| tx.projects(false))
}
