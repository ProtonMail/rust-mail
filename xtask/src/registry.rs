use crate::util::PathExt;
use anyhow::Result;
use camino::Utf8Path;
use cargo_metadata::semver::Version;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;
use xshell::{cmd, Shell};

#[derive(Debug)]
pub struct Registry {
    sh: Shell,
    branch: String,
}

impl Registry {
    pub fn new(url: &str, name: &str, email: &str, tgt: &Utf8Path) -> Result<Self> {
        let sh = Shell::new()?;
        let path = tgt.join("registry").remove_if_exists()?;

        cmd!(sh, "git clone {url} {path}")
            .ignore_stdout()
            .ignore_stderr()
            .run()?;

        sh.change_dir(path);

        cmd!(sh, "git config user.name {name}")
            .ignore_stdout()
            .ignore_stderr()
            .run()?;

        cmd!(sh, "git config user.email {email}")
            .ignore_stdout()
            .ignore_stderr()
            .run()?;

        let branch = format!("publish/{}", Uuid::new_v4());

        cmd!(sh, "git branch {branch}")
            .ignore_stdout()
            .ignore_stderr()
            .run()?;

        Ok(Self { sh, branch })
    }

    pub fn has(&self, name: &str, version: &Version) -> Result<bool> {
        self.switch(|_| {
            let version = version.to_string();

            let data_path = self.data_path(name, &version);
            let json_path = self.json_path(name, &version);

            Ok(data_path.exists() && json_path.exists())
        })
    }

    pub fn commit(&self, name: &str, version: &Version, data: &Utf8Path, json: &str) -> Result<()> {
        self.switch(|_| {
            let version = version.to_string();

            let data_path = self.data_path(name, &version);
            let json_path = self.json_path(name, &version);

            if !data_path.exists() {
                fs::copy(data, &data_path)?;
            } else {
                panic!("data file already exists: {}", data_path.display());
            }

            if !json_path.exists() {
                fs::write(&json_path, json)?;
            } else {
                panic!("json file already exists: {}", json_path.display());
            }

            cmd!(self.sh, "git add {data_path} {json_path}")
                .ignore_stdout()
                .ignore_stderr()
                .run()?;

            cmd!(self.sh, "git commit")
                .args(["-m", &format!("Publish: {name}-{version}")])
                .ignore_stdout()
                .ignore_stderr()
                .run()?;

            Ok(())
        })
    }

    pub fn push(&self) -> Result<()> {
        self.switch(|b| {
            cmd!(self.sh, "git push")
                .args(["-o", "merge_request.create"])
                .args(["-u", "origin", b])
                .ignore_stdout()
                .ignore_stderr()
                .run()?;

            Ok(())
        })
    }

    fn switch<T>(&self, f: impl FnOnce(&str) -> Result<T>) -> Result<T> {
        let branch = &self.branch;

        cmd!(self.sh, "git switch {branch}")
            .ignore_stdout()
            .ignore_stderr()
            .run()?;

        let res = f(branch)?;

        cmd!(self.sh, "git switch --discard-changes -")
            .ignore_stdout()
            .ignore_stderr()
            .run()?;

        Ok(res)
    }

    fn data_path(&self, name: &str, version: &str) -> PathBuf {
        self.downloads().join(format!("{name}@{version}.crate"))
    }

    fn json_path(&self, name: &str, version: &str) -> PathBuf {
        self.downloads().join(format!("{name}@{version}.json"))
    }

    fn downloads(&self) -> PathBuf {
        self.cwd().join("downloads")
    }

    fn cwd(&self) -> PathBuf {
        self.sh.current_dir()
    }
}
