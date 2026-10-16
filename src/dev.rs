//! Explicit repository relocation, reverse relocation, and safe Work-link removal.
//!
//! Never deletes source/data, enables plugins, or calls shell IPC. Activation is a
//! separate, approved action. No overwrite/merge behavior is provided.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use crate::{Error, Result};

pub const ID: &str = "io.github.jwkicklighter.snipboard";

fn occupied(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

pub fn relocate(source: &Path, destination: &Path, link: &Path) -> Result<()> {
    if source.is_symlink() || !source.join(".git").is_dir() {
        return Err(Error::Invalid("source must be the real repository directory"));
    }
    if occupied(destination) {
        return Err(Error::Invalid("destination already exists; never overwrite"));
    }
    if link != source && occupied(link) {
        return Err(Error::Invalid("link destination already exists"));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    // rename is atomic on one filesystem; deliberately no cross-filesystem copy.
    fs::rename(source, destination)?;
    if let Err(error) = symlink(destination, link) {
        fs::rename(destination, source)?;
        return Err(error.into());
    }
    Ok(())
}

pub fn unlink_work(link: &Path, target: &Path) -> Result<()> {
    let expected = match (link.canonicalize(), target.canonicalize()) {
        (Ok(resolved), Ok(target)) => resolved == target,
        _ => false,
    };
    if !link.is_symlink() || !expected {
        return Err(Error::Invalid("refusing to remove anything except the expected Work symlink"));
    }
    fs::remove_file(link)?;
    Ok(())
}

/// Move the installed repository back to Work. Disable and rescan separately, with
/// approval, before moving active code.
pub fn uninstall(work: &Path, installed: &Path) -> Result<()> {
    if work.is_symlink() {
        if work.canonicalize()? != installed.canonicalize()? {
            return Err(Error::Invalid("unexpected Work link"));
        }
        fs::remove_file(work)?;
        if let Err(error) = fs::rename(installed, work) {
            symlink(installed, work)?;
            return Err(error.into());
        }
    } else if !occupied(work) {
        fs::rename(installed, work)?;
    } else {
        return Err(Error::Invalid("Work path is occupied; refusing overwrite"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_tree_and_no_overwrite_or_delete() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (source, destination) = (root.join("work"), root.join("plugins/id"));
        fs::create_dir_all(source.join(".git")).unwrap();
        fs::write(source.join("user-work"), "preserved").unwrap();
        relocate(&source, &destination, &source).unwrap();
        assert!(source.is_symlink());
        assert_eq!(fs::read_to_string(source.join("user-work")).unwrap(), "preserved");
        assert!(relocate(&destination, &destination, &source).is_err());
        assert!(unlink_work(&destination, &source).is_err());
        unlink_work(&source, &destination).unwrap();
        assert!(!occupied(&source));
        assert_eq!(fs::read_to_string(destination.join("user-work")).unwrap(), "preserved");
    }

    #[test]
    fn uninstall_restores_work_tree_without_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (work, installed) = (root.join("work"), root.join("plugins/id"));
        fs::create_dir_all(work.join(".git")).unwrap();
        relocate(&work, &installed, &work).unwrap();
        uninstall(&work, &installed).unwrap();
        assert!(work.join(".git").is_dir() && !work.is_symlink() && !occupied(&installed));
        fs::create_dir_all(&installed).unwrap();
        assert!(uninstall(&work, &installed).is_err());
        assert!(work.join(".git").is_dir());
    }
}
