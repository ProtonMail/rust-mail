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

    pub fn commit(&self, name: &str, vrsn: &Version, data: &Utf8Path, json: &str) -> Result<()> {
        self.switch(|_| {
            let vrsn = vrsn.to_string();

            let data_path = self.data_path(name, &vrsn);
            let json_path = self.json_path(name, &vrsn);

            assert!(!data_path.exists());
            assert!(!json_path.exists());

            fs::copy(data, &data_path)?;
            fs::write(&json_path, json)?;

            cmd!(self.sh, "git add {data_path} {json_path}")
                .ignore_stdout()
                .ignore_stderr()
                .run()?;

            cmd!(self.sh, "git commit")
                .args(["-m", &format!("Publish: {name}-{vrsn}")])
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

    fn data_path(&self, name: &str, version: &str) -> PathBuf {
        self.downloads().join(format!("{name}@{version}.crate"))
    }

    fn json_path(&self, name: &str, version: &str) -> PathBuf {
        self.downloads().join(format!("{name}@{version}.json"))
    }

    pub fn downloads(&self) -> PathBuf {
        self.cwd().join("downloads")
    }

    #[allow(dead_code)]
    pub fn index(&self) -> PathBuf {
        self.cwd().join("index")
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

    fn cwd(&self) -> PathBuf {
        self.sh.current_dir()
    }
}
