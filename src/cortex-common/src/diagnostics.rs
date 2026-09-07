//! Opt-in local diagnostics. The event API accepts no user-supplied text.

use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use uuid::Uuid;

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_FILES: usize = 64;
const RETENTION: Duration = Duration::from_secs(7 * 24 * 3600);
static JOURNAL: OnceLock<Journal> = OnceLock::new();

tokio::task_local! {
    static TRACE: TraceContext;
}

/// W3C trace identifiers, never credentials or user identifiers.
#[derive(Debug, Clone)]
pub struct TraceContext {
    trace_id: String,
    span_id: String,
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::from_parent(None)
    }
}

impl TraceContext {
    pub fn from_parent(parent: Option<&str>) -> Self {
        let trace_id = parent
            .filter(|value| valid_parent(value))
            .map(|value| value[3..35].to_string())
            .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        Self {
            trace_id,
            span_id: Uuid::new_v4().simple().to_string()[..16].to_string(),
        }
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn traceparent(&self) -> String {
        format!("00-{}-{}-01", self.trace_id, self.span_id)
    }

    pub fn current() -> Self {
        TRACE.try_with(Clone::clone).unwrap_or_default()
    }

    pub async fn scope<F: Future>(self, future: F) -> F::Output {
        TRACE.scope(self, future).await
    }
}

fn valid_parent(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    parts.len() == 4
        && parts[0] == "00"
        && parts[1].len() == 32
        && parts[2].len() == 16
        && parts[3].len() == 2
        && parts[1..].iter().all(|s| {
            s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        && parts[1].bytes().any(|c| c != b'0')
        && parts[2].bytes().any(|c| c != b'0')
}

/// Closed vocabulary prevents prompts, paths, tokens, or URLs entering events.
#[derive(Debug, Clone, Copy)]
pub enum Operation {
    CliCommand,
    CliInteractive,
    CliDebug,
    ServerRequest,
    SessionCreated,
    SessionDeleted,
    ServerStarted,
    HealthCheck,
}

impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::CliCommand => "cli.command",
            Self::CliInteractive => "cli.interactive",
            Self::CliDebug => "cli.debug",
            Self::ServerRequest => "server.request",
            Self::SessionCreated => "session.created",
            Self::SessionDeleted => "session.deleted",
            Self::ServerStarted => "server.started",
            Self::HealthCheck => "health.check",
        }
    }
}

pub struct Journal {
    file: Mutex<File>,
}

impl Journal {
    pub fn open(directory: &Path) -> io::Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory)?;
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::other(
                "Diagnostics directory must not be a symlink",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Diagnostics directory must be private (0700)",
                ));
            }
        }
        let mut count = 0;
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let generated = name
                .strip_prefix("run-")
                .and_then(|s| s.strip_suffix(".jsonl"))
                .is_some_and(|s| Uuid::parse_str(s).is_ok());
            if generated && entry.file_type()?.is_file() {
                let age = entry.metadata()?.modified()?.elapsed().unwrap_or_default();
                if age > RETENTION {
                    fs::remove_file(entry.path())?;
                } else {
                    count += 1;
                }
            }
        }
        if count >= MAX_FILES {
            return Err(io::Error::other(
                "Local diagnostics retention limit reached",
            ));
        }
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(directory.join(format!("run-{}.jsonl", Uuid::new_v4())))?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    pub fn record(
        &self,
        operation: Operation,
        trace: &TraceContext,
        status: u16,
        elapsed: Duration,
    ) -> io::Result<()> {
        let event = event(operation, trace, status, elapsed);
        let mut bytes = serde_json::to_vec(&event)?;
        bytes.push(b'\n');
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("Diagnostics lock failed"))?;
        if file.metadata()?.len() + bytes.len() as u64 > MAX_BYTES {
            return Err(io::Error::other("Local diagnostics file limit reached"));
        }
        file.write_all(&bytes)
    }
}

fn event(operation: Operation, trace: &TraceContext, status: u16, elapsed: Duration) -> Value {
    json!({
        "schema": 1,
        "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "version": env!("CARGO_PKG_VERSION"),
        "operation": operation.name(),
        "trace_id": trace.trace_id,
        "span_id": trace.span_id,
        "status": status,
        "duration_ms": elapsed.as_millis().min(u64::MAX as u128) as u64,
    })
}

/// No directory configured means no diagnostic storage and no new network client.
pub fn init_from_env() -> io::Result<()> {
    if let Some(directory) = std::env::var_os("CORTEX_DIAGNOSTICS_DIR") {
        let journal = Journal::open(Path::new(&directory))?;
        JOURNAL
            .set(journal)
            .map_err(|_| io::Error::other("Diagnostics already initialized"))?;
    }
    Ok(())
}

pub fn record(
    operation: Operation,
    trace: &TraceContext,
    status: u16,
    elapsed: Duration,
) -> io::Result<()> {
    if let Some(journal) = JOURNAL.get() {
        journal.record(operation, trace, status, elapsed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_parent_validation_and_child_span() {
        let parent = TraceContext::default();
        let child = TraceContext::from_parent(Some(&parent.traceparent()));
        assert_eq!(child.trace_id(), parent.trace_id());
        assert_ne!(child.span_id, parent.span_id);
        for invalid in [
            "",
            "secret",
            "00-00000000000000000000000000000000-0123456789012345-01",
            "00-éééééééééééééééé-0123456789012345-01",
        ] {
            assert!(!valid_parent(invalid));
            assert!(valid_parent(
                &TraceContext::from_parent(Some(invalid)).traceparent()
            ));
        }
    }

    #[tokio::test]
    async fn test_trace_context_is_scoped_across_awaits() {
        let trace = TraceContext::default();
        let expected = trace.trace_id().to_string();
        trace
            .scope(async {
                tokio::task::yield_now().await;
                assert_eq!(TraceContext::current().trace_id(), expected);
            })
            .await;
        assert_ne!(TraceContext::current().trace_id(), expected);
    }

    #[test]
    fn test_journal_has_only_allowlisted_data_and_private_permissions() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("diagnostics");
        let journal = Journal::open(&directory).unwrap();
        journal
            .record(
                Operation::CliCommand,
                &TraceContext::default(),
                500,
                Duration::from_millis(12),
            )
            .unwrap();
        let path = fs::read_dir(directory)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["status"], 500);
        assert_eq!(value["duration_ms"], 12);
        assert_eq!(value.as_object().unwrap().len(), 8);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn test_journal_refuses_unbounded_growth() {
        let root = tempfile::tempdir().unwrap();
        let journal = Journal::open(&root.path().join("diagnostics")).unwrap();
        journal.file.lock().unwrap().set_len(MAX_BYTES).unwrap();
        assert!(
            journal
                .record(
                    Operation::ServerRequest,
                    &TraceContext::default(),
                    200,
                    Duration::ZERO
                )
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_journal_rejects_symlinks_and_public_directories() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};
        let root = tempfile::tempdir().unwrap();
        symlink(root.path(), root.path().join("link")).unwrap();
        assert!(Journal::open(&root.path().join("link")).is_err());
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Journal::open(root.path()).is_err());
    }

    #[test]
    fn test_retention_removes_only_expired_generated_files() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("diagnostics");
        drop(Journal::open(&directory).unwrap());
        let generated = fs::read_dir(&directory)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        File::options()
            .write(true)
            .open(&generated)
            .unwrap()
            .set_modified(SystemTime::now() - RETENTION - Duration::from_secs(1))
            .unwrap();
        fs::write(directory.join("notes.txt"), "keep").unwrap();
        drop(Journal::open(&directory).unwrap());
        assert!(!generated.exists());
        assert_eq!(
            fs::read_to_string(directory.join("notes.txt")).unwrap(),
            "keep"
        );
        for _ in 1..MAX_FILES {
            drop(Journal::open(&directory).unwrap());
        }
        assert!(Journal::open(&directory).is_err());
    }
}
