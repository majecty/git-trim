use std::fmt::Write;
use std::io::{BufRead, BufReader, Error, Write as _};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::spawn;

use log::*;
use tempfile::{tempdir, TempDir};

#[derive(Default)]
pub struct Fixture {
    fixture: String,
    epilogue: String,
}

impl Fixture {
    pub fn new() -> Fixture {
        Fixture::default()
    }

    fn append_fixture(&self, log_level: &str, appended: &str) -> Fixture {
        let mut fixture = String::new();
        writeln!(fixture, "{}", self.fixture).unwrap();
        writeln!(fixture, "echo ::set-level::{} >&2", log_level).unwrap();
        writeln!(fixture, "{}", textwrap::dedent(appended)).unwrap();
        Fixture {
            fixture,
            epilogue: self.epilogue.clone(),
        }
    }

    pub fn append_fixture_none(&self, appended: &str) -> Fixture {
        self.append_fixture("none", appended)
    }

    pub fn append_fixture_trace(&self, appended: &str) -> Fixture {
        self.append_fixture("trace", appended)
    }

    pub fn append_fixture_debug(&self, appended: &str) -> Fixture {
        self.append_fixture("debug", appended)
    }

    fn append_epilogue(&self, appended: &str) -> Fixture {
        let mut epilogue = String::new();
        writeln!(epilogue, "{}", self.epilogue).unwrap();
        writeln!(epilogue, "{}", textwrap::dedent(appended)).unwrap();
        Fixture {
            fixture: self.fixture.clone(),
            epilogue,
        }
    }

    pub fn prepare(
        &self,
        working_directory: &str,
        last_fixture: &str,
    ) -> std::io::Result<FixtureGuard> {
        let _ = env_logger::builder().is_test(true).try_init();

        let tempdir = tempdir()?;
        println!("{:?}", tempdir.path());
        let mut bash = Command::new("bash")
            .args(&["--noprofile", "--norc", "-xeo", "pipefail"])
            .current_dir(tempdir.path())
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = bash.stdin.take().unwrap();
        let merged_fixture = self
            .append_fixture_debug(&textwrap::dedent(last_fixture))
            .append_fixture_debug(&self.epilogue);
        writeln!(stdin, "{}", &merged_fixture.fixture).unwrap();
        drop(stdin);

        let stdout_thread = spawn({
            let stdout = bash.stdout.take().unwrap();
            move || {
                for line in BufReader::new(stdout).lines() {
                    match line {
                        Ok(line) if line.starts_with('+') => trace!("{}", line),
                        Ok(line) => info!("{}", line),
                        Err(err) => error!("{}", err),
                    }
                }
            }
        });

        let stderr_thread = spawn({
            let stderr = bash.stderr.take().unwrap();
            move || {
                let mut level = Some(Level::Debug);
                for line in BufReader::new(stderr).lines() {
                    match line {
                        Ok(line) if line.starts_with('+') && level.is_none() => {}
                        Ok(line) if line.starts_with('+') => {
                            log!(target: "stderr", level.unwrap(), "{}", line)
                        }
                        Ok(line) if line.starts_with("::set-level::") => {
                            if line.starts_with("::set-level::none") {
                                level = None
                            } else if line.starts_with("::set-level::trace") {
                                level = Some(Level::Trace)
                            } else if line.starts_with("::set-level::debug") {
                                level = Some(Level::Debug)
                            }
                            if let Some(level) = level {
                                log!(target: "stderr-set-level", level, "{}", line);
                            }
                        }
                        Ok(line) => info!(target: "stderr", "stderr: {}", line),
                        Err(err) => error!(target: "stderr", "stderr: {}", err),
                    }
                }
            }
        });
        stdout_thread.join().unwrap();
        stderr_thread.join().unwrap();

        let exit_status = bash.wait()?;
        if !exit_status.success() {
            return Err(Error::from_raw_os_error(exit_status.code().unwrap_or(-1)));
        }

        Ok(FixtureGuard {
            tempdir,
            working_directory: working_directory.to_string(),
        })
    }
}

#[must_use]
pub struct FixtureGuard {
    tempdir: TempDir,
    working_directory: String,
}

impl FixtureGuard {
    pub fn working_directory(&self) -> PathBuf {
        self.tempdir.path().join(&self.working_directory)
    }
}

pub fn rc() -> Fixture {
    Fixture::new()
        .append_fixture_none(
            r#"
            shopt -s expand_aliases
            within() {
                pushd $1 > /dev/null
                source /dev/stdin
                popd > /dev/null
            }
            alias upstream='within upstream'
            alias origin='within origin'
            alias local='within local'
            "#,
        )
        .append_epilogue(
            r#"
            local <<EOF
                pwd
                git remote update --prune
                git branch -vv --all
                git log --oneline --oneline --decorate --graph --all
            EOF
            "#,
        )
}

#[macro_export]
macro_rules! set {
    {$($x:expr),*} => ({
        use ::std::collections::HashSet;
        use ::std::iter::FromIterator;

        HashSet::from_iter(vec![$(From::from($x),)*])
    });
    {$($x:expr,)*} => ($crate::set!{$($x),*})
}

#[test]
#[ignore]
fn test() -> std::io::Result<()> {
    let _guard = Fixture::new()
        .append_epilogue("echo 'epilogue'")
        .append_fixture("debug", "echo 'fixture'")
        .prepare("", "");
    Ok(())
}
