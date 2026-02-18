// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

/// On WASM, there is no risk of unsafe overwrite via file descriptors,
/// since there are no real OS-level file handles.
pub fn is_unsafe_overwrite<I, O>(_input: &I, _output: &O) -> bool {
    false
}
