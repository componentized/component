#![no_main]

use std::path::Path;

use crate::{
    exports::componentized::component::{
        path_loader::Guest,
        types::{Component, Error},
    },
    wasi::filesystem::{preopens, types as filesystem},
};

pub(crate) struct FilesystemLoader;

impl Guest for FilesystemLoader {
    #[allow(async_fn_in_trait)]
    async fn load(path: String) -> Result<Component, Error> {
        let (dir, path) = resolve(&path)?;
        let file = dir
            .open_at(
                filesystem::PathFlags::SYMLINK_FOLLOW,
                path,
                filesystem::OpenFlags::empty(),
                filesystem::DescriptorFlags::READ,
            )
            .await?;
        let (data, result) = file.read_via_stream(0);
        let component = data.collect().await;
        result.await?;
        Ok(component)
    }
}

/// Resolve a path to the preopened directory that contains it and the path relative to that
/// directory. When multiple preopens match, the most specific one wins.
fn resolve(path: &str) -> Result<(filesystem::Descriptor, String), Error> {
    let path = Path::new(path);
    let mut resolved: Option<(usize, filesystem::Descriptor, String)> = None;
    for (dir, name) in preopens::get_directories() {
        let prefix = Path::new(&name);
        let relative = if path.is_relative() && prefix == Path::new(".") {
            Some(path)
        } else {
            path.strip_prefix(prefix).ok()
        };
        let Some(relative) = relative.and_then(Path::to_str) else {
            continue;
        };
        let specificity = prefix.components().count();
        if resolved.as_ref().is_none_or(|(s, _, _)| specificity > *s) {
            resolved = Some((specificity, dir, relative.to_string()));
        }
    }
    resolved.map(|(_, dir, path)| (dir, path)).ok_or_else(|| {
        Error::Other(Some(format!(
            "filesystem: no preopen for path {}",
            path.display()
        )))
    })
}

impl From<filesystem::ErrorCode> for Error {
    fn from(value: filesystem::ErrorCode) -> Self {
        match value {
            filesystem::ErrorCode::Access => Self::Other(Some("filesystem: access".to_string())),
            filesystem::ErrorCode::Already => Self::Other(Some("filesystem: already".to_string())),
            filesystem::ErrorCode::BadDescriptor => {
                Self::Other(Some("filesystem: bad-descriptor".to_string()))
            }
            filesystem::ErrorCode::Busy => Self::Other(Some("filesystem: busy".to_string())),
            filesystem::ErrorCode::Deadlock => {
                Self::Other(Some("filesystem: deadlock".to_string()))
            }
            filesystem::ErrorCode::Quota => Self::Other(Some("filesystem: quota".to_string())),
            filesystem::ErrorCode::Exist => Self::Other(Some("filesystem: exist".to_string())),
            filesystem::ErrorCode::FileTooLarge => {
                Self::Other(Some("filesystem: file-too-large".to_string()))
            }
            filesystem::ErrorCode::IllegalByteSequence => {
                Self::Other(Some("filesystem: illegal-byte-sequence".to_string()))
            }
            filesystem::ErrorCode::InProgress => {
                Self::Other(Some("filesystem: in-progress".to_string()))
            }
            filesystem::ErrorCode::Interrupted => {
                Self::Other(Some("filesystem: interrupted".to_string()))
            }
            filesystem::ErrorCode::Invalid => Self::Other(Some("filesystem: invalid".to_string())),
            filesystem::ErrorCode::Io => Self::Other(Some("filesystem: io".to_string())),
            filesystem::ErrorCode::IsDirectory => {
                Self::Other(Some("filesystem: is-directory".to_string()))
            }
            filesystem::ErrorCode::Loop => Self::Other(Some("filesystem: loop".to_string())),
            filesystem::ErrorCode::TooManyLinks => {
                Self::Other(Some("filesystem: too-many-links".to_string()))
            }
            filesystem::ErrorCode::MessageSize => {
                Self::Other(Some("filesystem: message-size".to_string()))
            }
            filesystem::ErrorCode::NameTooLong => {
                Self::Other(Some("filesystem: name-too-long".to_string()))
            }
            filesystem::ErrorCode::NoDevice => {
                Self::Other(Some("filesystem: no-device".to_string()))
            }
            filesystem::ErrorCode::NoEntry => Self::Other(Some("filesystem: no-entry".to_string())),
            filesystem::ErrorCode::NoLock => Self::Other(Some("filesystem: no-lock".to_string())),
            filesystem::ErrorCode::InsufficientMemory => {
                Self::Other(Some("filesystem: insufficient-memory".to_string()))
            }
            filesystem::ErrorCode::InsufficientSpace => {
                Self::Other(Some("filesystem: insufficient-space".to_string()))
            }
            filesystem::ErrorCode::NotDirectory => {
                Self::Other(Some("filesystem: not-directory".to_string()))
            }
            filesystem::ErrorCode::NotEmpty => {
                Self::Other(Some("filesystem: not-empty".to_string()))
            }
            filesystem::ErrorCode::NotRecoverable => {
                Self::Other(Some("filesystem: not-recoverable".to_string()))
            }
            filesystem::ErrorCode::Unsupported => {
                Self::Other(Some("filesystem: unsupported".to_string()))
            }
            filesystem::ErrorCode::NoTty => Self::Other(Some("filesystem: no-tty".to_string())),
            filesystem::ErrorCode::NoSuchDevice => {
                Self::Other(Some("filesystem: no-such-device".to_string()))
            }
            filesystem::ErrorCode::Overflow => {
                Self::Other(Some("filesystem: overflow".to_string()))
            }
            filesystem::ErrorCode::NotPermitted => {
                Self::Other(Some("filesystem: not-permitted".to_string()))
            }
            filesystem::ErrorCode::Pipe => Self::Other(Some("filesystem: pipe".to_string())),
            filesystem::ErrorCode::ReadOnly => {
                Self::Other(Some("filesystem: read-only".to_string()))
            }
            filesystem::ErrorCode::InvalidSeek => {
                Self::Other(Some("filesystem: invalid-seek".to_string()))
            }
            filesystem::ErrorCode::TextFileBusy => {
                Self::Other(Some("filesystem: text-file-busy".to_string()))
            }
            filesystem::ErrorCode::CrossDevice => {
                Self::Other(Some("filesystem: cross-device".to_string()))
            }
            filesystem::ErrorCode::Other(None) => {
                Self::Other(Some("filesystem: other".to_string()))
            }
            filesystem::ErrorCode::Other(Some(message)) => {
                Self::Other(Some(format!("filesystem: other: {message}")))
            }
        }
    }
}

wit_bindgen::generate!({
    path: "../wit",
    world: "filesystem-loader",
    merge_structurally_equal_types: true,
    generate_all
});

export!(FilesystemLoader);
