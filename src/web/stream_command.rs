//! Reusable SSE streaming for shell commands.
//!
//! Provides a helper that spawns a [`tokio::process::Command`], pipes its
//! stdout/stderr back as Server-Sent Events, and emits a final "done" event
//! with the exit status.

use std::convert::Infallible;

use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::warn;

/// Spawn `cmd` and return an SSE stream of its output.
///
/// Events emitted:
/// - `output` – one per stdout/stderr line, `data` is the line text.
/// - `done`   – final event, `data` is `{"success":true/false,"code":<i32>}`.
///
/// If the command fails to spawn an `error` event is sent instead.
pub fn stream_command(mut cmd: Command) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        use std::process::Stdio;

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "Failed to spawn command");
                let msg = format!("Failed to start command: {e}");
                yield Ok(Event::default().event("output").data(msg));
                yield Ok(Event::default().event("done").data(
                    r#"{"success":false,"code":-1}"#,
                ));
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();

        let mut stdout_done = false;
        let mut stderr_done = false;

        while !stdout_done || !stderr_done {
            tokio::select! {
                line = stdout_lines.next_line(), if !stdout_done => {
                    match line {
                        Ok(Some(l)) => {
                            yield Ok(Event::default().event("output").data(l));
                        }
                        Ok(None) => stdout_done = true,
                        Err(e) => {
                            yield Ok(Event::default().event("output").data(
                                format!("[read error: {e}]"),
                            ));
                            stdout_done = true;
                        }
                    }
                }
                line = stderr_lines.next_line(), if !stderr_done => {
                    match line {
                        Ok(Some(l)) => {
                            yield Ok(Event::default().event("output").data(l));
                        }
                        Ok(None) => stderr_done = true,
                        Err(e) => {
                            yield Ok(Event::default().event("output").data(
                                format!("[read error: {e}]"),
                            ));
                            stderr_done = true;
                        }
                    }
                }
            }
        }

        let status = match child.wait().await {
            Ok(s) => s,
            Err(e) => {
                yield Ok(Event::default().event("output").data(
                    format!("[wait error: {e}]"),
                ));
                yield Ok(Event::default().event("done").data(
                    r#"{"success":false,"code":-1}"#,
                ));
                return;
            }
        };

        let code = status.code().unwrap_or(-1);
        let success = status.success();
        let done_data = format!(r#"{{"success":{success},"code":{code}}}"#);
        yield Ok(Event::default().event("done").data(done_data));
    };

    Sse::new(stream)
}
