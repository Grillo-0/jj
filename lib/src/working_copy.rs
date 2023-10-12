// Copyright 2023 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Defines the interface for the working copy. See `LocalWorkingCopy` for the
//! default local-disk implementation.

use std::any::Any;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use thiserror::Error;

use crate::backend::{BackendError, MergedTreeId};
use crate::fsmonitor::FsmonitorKind;
use crate::gitignore::GitIgnoreFile;
use crate::op_store::{OperationId, WorkspaceId};
use crate::repo_path::RepoPath;
use crate::settings::HumanByteSize;

/// The trait all working-copy implementations must implement.
pub trait WorkingCopy {
    /// Should return `self`. For down-casting purposes.
    fn as_any(&self) -> &dyn Any;

    /// The name/id of the implementation. Used for choosing the right
    /// implementation when loading a working copy.
    fn name(&self) -> &str;

    /// The working copy's root directory.
    fn path(&self) -> &Path;

    /// The working copy's workspace ID.
    fn workspace_id(&self) -> &WorkspaceId;

    /// The operation this working copy was most recently updated to.
    fn operation_id(&self) -> &OperationId;
}

/// A working copy that's being modified.
pub trait LockedWorkingCopy {
    /// Should return `self`. For down-casting purposes.
    fn as_any(&self) -> &dyn Any;

    /// The operation at the time the lock was taken
    fn old_operation_id(&self) -> &OperationId;

    /// The tree at the time the lock was taken
    fn old_tree_id(&self) -> &MergedTreeId;

    /// Snapshot the working copy and return the tree id.
    fn snapshot(&mut self, options: SnapshotOptions) -> Result<MergedTreeId, SnapshotError>;
}

/// An error while snapshotting the working copy.
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// A path in the working copy was not valid UTF-8.
    #[error("Working copy path {} is not valid UTF-8", path.to_string_lossy())]
    InvalidUtf8Path {
        /// The path with invalid UTF-8.
        path: OsString,
    },
    /// A symlink target in the working copy was not valid UTF-8.
    #[error("Symlink {path} target is not valid UTF-8")]
    InvalidUtf8SymlinkTarget {
        /// The path of the symlink that has a target that's not valid UTF-8.
        /// This path itself is valid UTF-8.
        path: PathBuf,
        /// The symlink target with invalid UTF-8.
        target: PathBuf,
    },
    /// Reading or writing from the commit backend failed.
    #[error("Internal backend error: {0}")]
    InternalBackendError(#[from] BackendError),
    /// A file was larger than the specified maximum file size for new
    /// (previously untracked) files.
    #[error("New file {path} of size ~{size} exceeds snapshot.max-new-file-size ({max_size})")]
    NewFileTooLarge {
        /// The path of the large file.
        path: PathBuf,
        /// The size of the large file.
        size: HumanByteSize,
        /// The maximum allowed size.
        max_size: HumanByteSize,
    },
    /// Some other error happened while snapshotting the working copy.
    #[error("{message}: {err:?}")]
    Other {
        /// Error message.
        message: String,
        /// The underlying error.
        #[source]
        err: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// Options used when snapshotting the working copy. Some of them may be ignored
/// by some `WorkingCopy` implementations.
pub struct SnapshotOptions<'a> {
    /// The `.gitignore`s to use while snapshotting. The typically come from the
    /// user's configured patterns combined with per-repo patterns.
    // The base_ignores are passed in here rather than being set on the TreeState
    // because the TreeState may be long-lived if the library is used in a
    // long-lived process.
    pub base_ignores: Arc<GitIgnoreFile>,
    /// The fsmonitor (e.g. Watchman) to use, if any.
    // TODO: Should we make this a field on `LocalWorkingCopy` instead since it's quite specific to
    // that implementation?
    pub fsmonitor_kind: Option<FsmonitorKind>,
    /// A callback for the UI to display progress.
    pub progress: Option<&'a SnapshotProgress<'a>>,
    /// The size of the largest file that should be allowed to become tracked
    /// (already tracked files are always snapshotted). If there are larger
    /// files in the working copy, then `LockedWorkingCopy::snapshot()` may
    /// (depending on implementation)
    /// return `SnapshotError::NewFileTooLarge`.
    pub max_new_file_size: u64,
}

impl SnapshotOptions<'_> {
    /// Create an instance for use in tests.
    pub fn empty_for_test() -> Self {
        SnapshotOptions {
            base_ignores: GitIgnoreFile::empty(),
            fsmonitor_kind: None,
            progress: None,
            max_new_file_size: u64::MAX,
        }
    }
}

/// A callback for getting progress updates.
pub type SnapshotProgress<'a> = dyn Fn(&RepoPath) + 'a + Sync;
