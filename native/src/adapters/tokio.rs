//! TokioProcessSpawner — async child-process spawning with piped stdio.

use crate::ports::{ChildHandle, ProcessSpawner, SpawnError, SpawnSpec};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

type ChildMap = Arc<Mutex<HashMap<u64, Child>>>;

pub struct TokioProcessSpawner {
    children: ChildMap,
}

impl TokioProcessSpawner {
    pub fn new() -> Self {
        Self {
            children: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for TokioProcessSpawner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProcessSpawner for TokioProcessSpawner {
    async fn spawn(&self, spec: SpawnSpec) -> Result<ChildHandle, SpawnError> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command.envs(&spec.env);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }

        let mut child = command
            .spawn()
            .map_err(|err| SpawnError::SpawnFailed(format!("{}: {err}", spec.program)))?;

        let id = child
            .id()
            .map(u64::from)
            .ok_or_else(|| SpawnError::SpawnFailed("child process missing id".to_string()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| SpawnError::SpawnFailed("child stdin was not piped".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SpawnError::SpawnFailed("child stdout was not piped".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SpawnError::SpawnFailed("child stderr was not piped".to_string()))?;

        self.children.lock().await.insert(id, child);

        Ok(ChildHandle {
            id,
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
        })
    }

    async fn kill(&self, id: u64) -> Result<(), SpawnError> {
        let mut child = {
            let mut children = self.children.lock().await;
            children.remove(&id)
        }
        .ok_or(SpawnError::NotFound(id))?;

        child
            .kill()
            .await
            .map_err(|err| SpawnError::KillFailed(format!("id={id}: {err}")))?;

        let _ = child.wait().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TokioProcessSpawner;
    use crate::ports::{ProcessSpawner, SpawnError, SpawnSpec};
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{sleep, Duration};

    #[cfg(unix)]
    fn sleep_spec() -> SpawnSpec {
        SpawnSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "sleep 1".to_string()],
            env: HashMap::new(),
            cwd: None,
        }
    }

    #[cfg(windows)]
    fn sleep_spec() -> SpawnSpec {
        SpawnSpec {
            program: "cmd".to_string(),
            args: vec!["/C".to_string(), "ping -n 2 127.0.0.1 >NUL".to_string()],
            env: HashMap::new(),
            cwd: None,
        }
    }

    #[cfg(unix)]
    fn echo_spec() -> SpawnSpec {
        SpawnSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "cat".to_string()],
            env: HashMap::new(),
            cwd: None,
        }
    }

    #[cfg(windows)]
    fn echo_spec() -> SpawnSpec {
        SpawnSpec {
            program: "cmd".to_string(),
            args: vec!["/Q".to_string(), "/K".to_string(), "more".to_string()],
            env: HashMap::new(),
            cwd: None,
        }
    }

    #[tokio::test]
    async fn kill_terminates_spawned_process() {
        let spawner = TokioProcessSpawner::new();
        let child = spawner
            .spawn(sleep_spec())
            .await
            .expect("spawn should succeed");

        sleep(Duration::from_millis(100)).await;

        spawner.kill(child.id).await.expect("kill should succeed");

        let err = spawner
            .kill(child.id)
            .await
            .expect_err("child should already be removed");
        assert!(matches!(err, SpawnError::NotFound(id) if id == child.id));
    }

    #[tokio::test]
    async fn spawn_returns_piped_stdio_handles() {
        let spawner = TokioProcessSpawner::new();
        let mut child = spawner
            .spawn(echo_spec())
            .await
            .expect("spawn should succeed");

        child
            .stdin
            .write_all(b"hello\n")
            .await
            .expect("stdin write should succeed");
        child
            .stdin
            .flush()
            .await
            .expect("stdin flush should succeed");

        let expected: &[u8] = if cfg!(windows) {
            b"hello\r\n"
        } else {
            b"hello\n"
        };
        let mut buf = vec![0_u8; expected.len()];
        child
            .stdout
            .read_exact(&mut buf)
            .await
            .expect("stdout should echo input");
        assert_eq!(buf, expected);

        spawner.kill(child.id).await.expect("kill should succeed");
    }
}
