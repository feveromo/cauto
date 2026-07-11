#![allow(dead_code)]

use std::path::{Path, PathBuf};

use assert_cmd::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn fake_codex(directory: &Path) -> PathBuf {
    let path = directory.join("codex");
    let catalog = include_str!("../fixtures/catalog.json");
    let script = format!(
        "#!/bin/sh\n\
         if [ -n \"$FAKE_CODEX_LOG\" ]; then\n\
           printf '%s\\n' \"$*\" >> \"$FAKE_CODEX_LOG\"\n\
         fi\n\
         if [ \"$1\" = \"--version\" ]; then\n\
           echo 'codex-cli test-1.0'\n\
           exit 0\n\
         fi\n\
         if [ \"$1\" = \"debug\" ] && [ \"$2\" = \"models\" ]; then\n\
           printf '%s\\n' '{}'\n\
           exit 0\n\
         fi\n\
         exit 0\n",
        catalog.replace('\'', "'\\''")
    );
    std::fs::write(&path, script).unwrap();
    #[cfg(unix)]
    {
        let mut permissions = path.metadata().unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
    }
    path
}

pub fn cauto_command(home: &Path) -> Command {
    let mut command = assert_cmd::cargo::cargo_bin_cmd!("cauto");
    command
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("CAUTO_DISABLE_LOG", "1")
        .env("NO_COLOR", "1");
    command
}
