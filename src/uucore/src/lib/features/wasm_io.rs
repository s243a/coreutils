// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! WASM I/O override module.
//!
//! On `wasm32-unknown-unknown`, `std::io::stdout()` silently discards output.
//! This module provides thread-local overrides so that utilities can write to
//! custom writers (e.g., brush-wasm's InMemoryStream) instead.
//!
//! Usage from a host (e.g., brush-wasm):
//! ```ignore
//! uucore::wasm_io::with_wasm_io(stdin, stdout, stderr, || {
//!     uu_cat::uumain(args.into_iter())
//! });
//! ```

use std::cell::RefCell;
use std::fs::File;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;

type FileOpenerFn = Box<dyn Fn(&Path) -> io::Result<Box<dyn Read>>>;
type FileExistsFn = Box<dyn Fn(&Path) -> bool>;

thread_local! {
    static STDOUT_OVERRIDE: RefCell<Option<Box<dyn Write>>> = RefCell::new(None);
    static STDERR_OVERRIDE: RefCell<Option<Box<dyn Write>>> = RefCell::new(None);
    static STDIN_OVERRIDE: RefCell<Option<Box<dyn Read>>> = RefCell::new(None);
    static FILE_OPENER: RefCell<Option<FileOpenerFn>> = RefCell::new(None);
    static FILE_EXISTS: RefCell<Option<FileExistsFn>> = RefCell::new(None);
}

/// Install custom stdin/stdout/stderr for the duration of a closure.
///
/// This is the primary entry point for hosts that want to capture output
/// from uutils commands on WASM. The overrides are automatically cleaned
/// up when the closure returns (even on panic).
pub fn with_wasm_io<F, R>(
    stdin: Box<dyn Read>,
    stdout: Box<dyn Write>,
    stderr: Box<dyn Write>,
    f: F,
) -> R
where
    F: FnOnce() -> R,
{
    STDIN_OVERRIDE.with(|s| *s.borrow_mut() = Some(stdin));
    STDOUT_OVERRIDE.with(|s| *s.borrow_mut() = Some(stdout));
    STDERR_OVERRIDE.with(|s| *s.borrow_mut() = Some(stderr));

    // Use a guard to ensure cleanup on panic.
    struct CleanupGuard;
    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            STDIN_OVERRIDE.with(|s| *s.borrow_mut() = None);
            STDOUT_OVERRIDE.with(|s| *s.borrow_mut() = None);
            STDERR_OVERRIDE.with(|s| *s.borrow_mut() = None);
            FILE_OPENER.with(|s| *s.borrow_mut() = None);
            FILE_EXISTS.with(|s| *s.borrow_mut() = None);
        }
    }
    let _guard = CleanupGuard;

    f()
}

/// A stdout wrapper that writes to the thread-local override if set,
/// or falls back to `std::io::stdout()`.
pub struct WasmStdout;

impl WasmStdout {
    /// No-op lock on WASM (single-threaded). Returns a `WasmStdoutLock`
    /// that implements `Write` with the same thread-local redirect.
    pub fn lock(&self) -> WasmStdoutLock {
        WasmStdoutLock
    }

    /// Always returns `false` on WASM — there is no terminal.
    pub fn is_terminal(&self) -> bool {
        false
    }
}

impl Write for WasmStdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_stdout(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        flush_stdout()
    }
}

/// Lock handle returned by `WasmStdout::lock()`. On WASM this is a
/// no-op wrapper — there is no real locking since WASM is single-threaded.
pub struct WasmStdoutLock;

impl Write for WasmStdoutLock {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_stdout(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        flush_stdout()
    }
}

fn write_stdout(buf: &[u8]) -> io::Result<usize> {
    STDOUT_OVERRIDE.with(|s| {
        let mut borrow = s.borrow_mut();
        if let Some(ref mut writer) = *borrow {
            writer.write(buf)
        } else {
            io::stdout().write(buf)
        }
    })
}

fn flush_stdout() -> io::Result<()> {
    STDOUT_OVERRIDE.with(|s| {
        let mut borrow = s.borrow_mut();
        if let Some(ref mut writer) = *borrow {
            writer.flush()
        } else {
            io::stdout().flush()
        }
    })
}

/// A stderr wrapper that writes to the thread-local override if set,
/// or falls back to `std::io::stderr()`.
pub struct WasmStderr;

impl WasmStderr {
    /// No-op lock on WASM (single-threaded).
    pub fn lock(&self) -> WasmStderrLock {
        WasmStderrLock
    }
}

impl Write for WasmStderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_stderr(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        flush_stderr()
    }
}

/// Lock handle returned by `WasmStderr::lock()`.
pub struct WasmStderrLock;

impl Write for WasmStderrLock {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        write_stderr(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        flush_stderr()
    }
}

fn write_stderr(buf: &[u8]) -> io::Result<usize> {
    STDERR_OVERRIDE.with(|s| {
        let mut borrow = s.borrow_mut();
        if let Some(ref mut writer) = *borrow {
            writer.write(buf)
        } else {
            io::stderr().write(buf)
        }
    })
}

fn flush_stderr() -> io::Result<()> {
    STDERR_OVERRIDE.with(|s| {
        let mut borrow = s.borrow_mut();
        if let Some(ref mut writer) = *borrow {
            writer.flush()
        } else {
            io::stderr().flush()
        }
    })
}

/// A stdin wrapper that reads from the thread-local override if set,
/// or falls back to `std::io::stdin()`.
pub struct WasmStdin;

impl WasmStdin {
    /// No-op lock on WASM (single-threaded). Returns a buffered reader.
    pub fn lock(&self) -> WasmStdinLock {
        WasmStdinLock::new()
    }

    /// Always returns `false` on WASM — there is no terminal.
    pub fn is_terminal(&self) -> bool {
        false
    }
}

impl Read for WasmStdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        read_stdin(buf)
    }
}

/// Lock handle returned by `WasmStdin::lock()`.
/// Contains an internal buffer so it can implement `BufRead`.
pub struct WasmStdinLock {
    buf: Vec<u8>,
    pos: usize,
    filled: usize,
}

impl WasmStdinLock {
    fn new() -> Self {
        Self {
            buf: vec![0u8; 8192],
            pos: 0,
            filled: 0,
        }
    }
}

impl Read for WasmStdinLock {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we have buffered data, serve from buffer first.
        let buffered = &self.buf[self.pos..self.filled];
        if !buffered.is_empty() {
            let n = std::cmp::min(buf.len(), buffered.len());
            buf[..n].copy_from_slice(&buffered[..n]);
            self.pos += n;
            return Ok(n);
        }
        // Otherwise read directly.
        read_stdin(buf)
    }
}

impl BufRead for WasmStdinLock {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.pos >= self.filled {
            self.pos = 0;
            self.filled = read_stdin(&mut self.buf)?;
        }
        Ok(&self.buf[self.pos..self.filled])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = std::cmp::min(self.pos + amt, self.filled);
    }
}

fn read_stdin(buf: &mut [u8]) -> io::Result<usize> {
    STDIN_OVERRIDE.with(|s| {
        let mut borrow = s.borrow_mut();
        if let Some(ref mut reader) = *borrow {
            reader.read(buf)
        } else {
            io::stdin().read(buf)
        }
    })
}

/// Returns a writer that uses the thread-local override if set.
pub fn stdout() -> WasmStdout {
    WasmStdout
}

/// Returns a writer that uses the thread-local override if set.
pub fn stderr() -> WasmStderr {
    WasmStderr
}

/// Returns a reader that uses the thread-local override if set.
pub fn stdin() -> WasmStdin {
    WasmStdin
}

// ── File I/O hooks ───────────────────────────────────────────────
// Allow hosts to provide a VFS-backed file opener so that builtins
// like cat, head, sort, etc. can open files by path on WASM.

/// Install file-opening overrides. Called by the host (brush-uutils)
/// before executing a builtin.
pub fn set_file_hooks(
    opener: Box<dyn Fn(&Path) -> io::Result<Box<dyn Read>>>,
    exists: Box<dyn Fn(&Path) -> bool>,
) {
    FILE_OPENER.with(|s| *s.borrow_mut() = Some(opener));
    FILE_EXISTS.with(|s| *s.borrow_mut() = Some(exists));
}

/// Open a file for reading, using the VFS override if set,
/// otherwise falling back to `std::fs::File::open`.
pub fn open_file(path: impl AsRef<Path>) -> io::Result<Box<dyn Read>> {
    let path = path.as_ref();
    FILE_OPENER.with(|cell| {
        let borrow = cell.borrow();
        if let Some(ref opener) = *borrow {
            opener(path)
        } else {
            Ok(Box::new(File::open(path)?) as Box<dyn Read>)
        }
    })
}

/// Check if a file exists, using the VFS override if set,
/// otherwise falling back to `Path::exists`.
pub fn file_exists(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    FILE_EXISTS.with(|cell| {
        let borrow = cell.borrow();
        if let Some(ref exists_fn) = *borrow {
            exists_fn(path)
        } else {
            path.exists()
        }
    })
}
