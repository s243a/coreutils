// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// WASM platform stubs for tail's process-checking functionality.
// Process monitoring is not available on WASM.

pub type Pid = u32;

pub struct ProcessChecker {
    _pid: Pid,
}

impl ProcessChecker {
    pub fn new(process_id: Pid) -> Self {
        Self { _pid: process_id }
    }

    #[allow(clippy::wrong_self_convention)]
    pub fn is_dead(&mut self) -> bool {
        // No process checking on WASM; assume parent is alive
        false
    }
}

impl Drop for ProcessChecker {
    fn drop(&mut self) {}
}

pub fn supports_pid_checks(_pid: Pid) -> bool {
    false
}
