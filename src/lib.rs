// TODO fix unwraps
#[macro_use]
extern crate serde_derive;
extern crate toml;

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{create_dir, File};
use std::io::{Read, Write};
use std::path::Path;

#[derive(Debug)]
pub enum PrepopError {
    PathError(String),
    TomlBuildError(String),
    InvalidProject,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoTomlPackage {
    name: String,
    version: String,
    authors: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CargoTomlData {
    package: CargoTomlPackage,
    dependencies: HashMap<String, String>,
}

#[derive(Clone)]
struct Package {
    name: String,
    dependencies: Option<Vec<String>>,
}

impl From<LockfilePackage> for Package {
    fn from(package: LockfilePackage) -> Self {
        let LockfilePackage {
            name, dependencies, ..
        } = package;
        Self { name, dependencies }
    }
}

enum CargoProject {
    Workspace(Vec<Package>),
    Project(Package),
}

#[derive(Deserialize, Debug, Clone)]
struct LockfilePackage {
    name: String,
    version: String,
    source: Option<String>,
    dependencies: Option<Vec<String>>,
}

pub fn prepopulate(lock_file_path: &Path) -> Result<(), PrepopError> {
    if lock_file_path.file_name() != Some(OsStr::new("Cargo.lock")) {
        return Err(PrepopError::PathError(
            "Path does not point to a Cargo.lock file".into(),
        ));
    }
    let base_path = {
        let mut p = lock_file_path.to_path_buf();
        p.pop();
        p
    };

    let project = parse_lockfile(&lock_file_path)?;

    match project {
        CargoProject::Workspace(members) => {
            let names = members
                .into_iter()
                .map(|member| {
                    let name = member.name.clone();
                    let member_path = base_path.join(&name);
                    populate_member(&member_path, member)?;
                    Ok(name.to_owned())
                })
                .collect::<Result<Vec<String>, PrepopError>>()?;
            build_workspace_toml(&base_path.join("Cargo.toml"), names)?;
            Ok(())
        }
        CargoProject::Project(dependencies) => populate_member(&base_path, dependencies),
    }
}

fn populate_member(member_path: &Path, member: Package) -> Result<(), PrepopError> {
    if !member_path.is_dir() {
        // TODO This should not be needed, dest should be unique and empty
        eprintln!("Creating {:?}", &member_path);
        create_dir(&member_path).unwrap();
    } else {
        eprintln!("WARN: Member-path {:?} already exists...", &member_path);
    }
    {
        let member_src_path = member_path.join("src");
        if !member_src_path.is_dir() {
            // TODO This should not be needed, dest should be unique and empty
            eprintln!("Creating {:?}", &member_src_path);
            create_dir(&member_src_path).unwrap();
        } else {
            eprintln!("WARN: Member-path {:?} already exists...", &member_src_path);
        }
        create_empty_file(&member_src_path.join("lib.rs"))?;
    }
    build_toml(&member_path.join("Cargo.toml"), member)
}

fn build_toml(path: &Path, package: Package) -> Result<(), PrepopError> {
    let Package { name, dependencies } = package;
    let cargo_toml = match dependencies {
        Some(dependencies) => CargoTomlData {
            package: CargoTomlPackage {
                name: name,
                authors: Some(vec!["cargo-prepopulate".into()]),
                version: "0.0.0".into(),
            },
            dependencies: dependencies
                .iter()
                .filter_map(|dep| build_and_filter_toml_dependency(dep))
                .collect(),
        },
        None => CargoTomlData {
            package: CargoTomlPackage {
                name: name,
                authors: Some(vec!["cargo-prepopulate".into()]),
                version: "0.0.0".into(),
            },
            dependencies: HashMap::new(),
        },
    };
    {
        let mut file = File::create(&path).map_err(|err| {
            PrepopError::TomlBuildError(format!("Could not open {:?} for writing: {:?}", path, err))
        })?;
        file.write_all(
            toml::to_string(&cargo_toml)
                .map_err(|err| {
                    PrepopError::TomlBuildError(format!(
                        "Failed toml-construction due to {:?}",
                        err
                    ))
                })?
                .as_bytes(),
        )
        .map_err(|err| {
            PrepopError::PathError(format!("Could not write to {:?}: {:?}", path, err))
        })?;
    }
    Ok(())
}

#[derive(Serialize)]
struct WorkspaceMembers {
    members: Vec<String>,
}

#[derive(Serialize)]
struct CargoWorkspace {
    workspace: WorkspaceMembers,
}

fn build_workspace_toml(path: &Path, names: Vec<String>) -> Result<(), PrepopError> {
    let cargo_toml = CargoWorkspace {
        workspace: WorkspaceMembers { members: names },
    };
    let mut file = File::create(&path).map_err(|err| {
        PrepopError::TomlBuildError(format!("Could not open {:?} for writing: {:?}", path, err))
    })?;
    file.write_all(
        toml::to_string(&cargo_toml)
            .map_err(|err| {
                PrepopError::TomlBuildError(format!("Failed toml-construction due to {:?}", err))
            })?
            .as_bytes(),
    )
    .map_err(|err| PrepopError::PathError(format!("Could not write to {:?}: {:?}", path, err)))?;
    Ok(())
}

fn build_and_filter_toml_dependency(lockfile_dependency: &str) -> Option<(String, String)> {
    let mut parts: Vec<_> = lockfile_dependency.rsplit(' ').collect();
    match parts.len() {
        3 => {
            // 3 parts means a crates.io dependency and we should return it
            let name = parts.pop().unwrap();
            let version = parts.pop().unwrap();
            let url = parts.pop().unwrap();
            if url == "(registry+https://github.com/rust-lang/crates.io-index)" {
                // TODO Properly handle non-crates io dependencies
                // For now, ignore them
                Some((name.into(), version.into()))
            } else {
                eprintln!("Ignoring non-crates-io dep '{}'", url);
                None
            }
        }
        2 => {
            // 2 parts means a local workspace component which we don't have, so skip it
            None
        }
        _ => unreachable!(),
    }
}

fn create_empty_file(path: &Path) -> Result<(), PrepopError> {
    File::create(path).map_err(|err| {
        PrepopError::PathError(format!("Could not create {:?} due to {:?}", path, err))
    })?;
    Ok(())
}
fn parse_lockfile(lockfile: &Path) -> Result<CargoProject, PrepopError> {
    let value: toml::Value = {
        let contents = get_file_contents(lockfile);
        contents.parse().unwrap()
    };
    let packages: Vec<LockfilePackage> = value.get("package").unwrap().clone().try_into().unwrap();
    let our_packages = packages
        .into_iter()
        .filter(|package| package.source.is_none())
        .map(|package| package.into())
        .collect::<Vec<Package>>();

    if our_packages.len() == 1 {
        let single_package = our_packages.first().unwrap();
        Ok(CargoProject::Project(single_package.clone()))
    } else {
        Ok(CargoProject::Workspace(our_packages))
    }
}

fn get_file_contents(path: &Path) -> String {
    let mut file = File::open(&path).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    //.map_err(|_| PrepopError::PathError(format!("Could not read {:?}", cargotoml)))?;
    contents
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
