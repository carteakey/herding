//! The `sheprd-acp` chats run on the machines that get a host build
//! (`host.rs`). Both targets' builds travel in one archive, with their own
//! signed manifest (schema 5) packed first, so a release stays under the
//! asset limit installed updaters read. The manifest's signature is
//! checked before any build in the archive is read, and the build against
//! the manifest before it leaves this computer.

use super::*;
use flate2::read::GzDecoder;
use std::{cell::Cell, io, rc::Rc};

pub(super) const SCHEMA: u32 = 5;
/// The signed manifest and its signature, the archive's first two entries.
const MANIFEST: (&str, &str) = ("host-chat-manifest.json", "host-chat-manifest.sig");
/// What tar adds around each entry at most: its header, the padding of its
/// data to a block, and a long name's extra header and block.
const ENTRY_OVERHEAD: u64 = 4 * 512;

/// The release asset holding every target's build.
pub(super) fn archive_name(version: &str) -> String {
    format!("sheprd-acp-{version}-linux-musl.tar.gz")
}

pub(super) fn accepts(version: &str, target: &str, asset: &str) -> bool {
    HOST_TARGETS.contains(&target) && asset == format!("sheprd-acp-{version}-{target}")
}

/// Reads the archive from `reader`, checks its manifest against `key_hex`
/// and `version`, and writes `target`'s build to `destination`, which must
/// not exist yet and is created private.
pub(super) fn unpack(
    reader: impl Read,
    version: &str,
    key_hex: &str,
    target: &str,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    if !HOST_TARGETS.contains(&target) {
        return Err(Error::UnsupportedTarget);
    }
    // Until the manifest is checked, only it and its signature may expand.
    let limit = Rc::new(Cell::new(MANIFEST_LIMIT as u64 + 64 + 2 * ENTRY_OVERHEAD));
    let stop = Rc::new(Cell::new(None));
    let guarded = Guarded {
        cancel,
        inner: GzDecoder::new(reader.take(ARCHIVE_LIMIT)),
        limit: limit.clone(),
        read: 0,
        stop: stop.clone(),
    };
    let result = read_archive(
        guarded,
        version,
        key_hex,
        target,
        destination,
        cancel,
        &limit,
    );
    // Whatever a reader made of the guard's refusal, say why it stopped.
    match (result, stop.get()) {
        (Err(_), Some(Stop::Cancelled)) => Err(Error::Cancelled),
        (Err(_), Some(Stop::Expanded)) => Err(Error::HostChatExpanded),
        (result, _) => result,
    }
}

fn read_archive(
    reader: impl Read,
    version: &str,
    key_hex: &str,
    target: &str,
    destination: &Path,
    cancel: &AtomicBool,
    limit: &Cell<u64>,
) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut entries = archive.entries().map_err(Error::ReadArchive)?;
    let mut next = |name: &str, limit: usize| -> Result<Vec<u8>> {
        let entry = entries
            .next()
            .ok_or(Error::HostChatLayout)?
            .map_err(Error::ReadArchive)?;
        if entry.path().map_err(Error::ReadArchive)? != Path::new(name) {
            return Err(Error::HostChatLayout);
        }
        read_bounded(entry, limit, cancel)
    };
    let manifest = next(MANIFEST.0, MANIFEST_LIMIT)?;
    let signature = next(MANIFEST.1, 64)?;
    let manifest = verify_manifest(&manifest, &signature, key_hex, version)?;
    // A manifest signed for another family must not answer for this one.
    if manifest.schema != SCHEMA {
        return Err(Error::ManifestSchema);
    }
    // Past the manifest, the archive holds the builds it signs and nothing
    // else: unsigned data there is refused rather than read through.
    let builds = manifest.assets.iter().fold(0u64, |total, asset| {
        total
            .saturating_add(asset.size)
            .saturating_add(ENTRY_OVERHEAD)
    });
    limit.set(limit.get().saturating_add(builds));
    let asset = manifest
        .assets
        .into_iter()
        .find(|asset| asset.target == target && accepts(version, target, &asset.name))
        .ok_or(Error::MissingPlatformAsset)?;
    let wanted = Path::new(target).join("sheprd-acp");
    for entry in entries {
        cancelled(cancel)?;
        let entry = entry.map_err(Error::ReadArchive)?;
        if entry.path().map_err(Error::ReadArchive)? != wanted {
            continue;
        }
        let mut file = create_archive(destination)?;
        copy_archive(entry, &mut file, &asset, cancel, |_, _| {})?;
        file.sync_all().map_err(Error::SyncArchive)?;
        return cancelled(cancel);
    }
    Err(Error::MissingPlatformAsset)
}

/// Why [`Guarded`] stopped reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    Cancelled,
    Expanded,
}

/// The expanded archive, read no further than `limit` and stopping once
/// cancelled, including through entries tar skips without handing out.
struct Guarded<'a, R> {
    cancel: &'a AtomicBool,
    inner: R,
    limit: Rc<Cell<u64>>,
    read: u64,
    stop: Rc<Cell<Option<Stop>>>,
}

impl<R: Read> Read for Guarded<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(self.refuse(Stop::Cancelled));
        }
        let left = self.limit.get().saturating_sub(self.read);
        // One byte past the limit tells a full archive from a longer one.
        let room = usize::try_from(left.saturating_add(1)).unwrap_or(usize::MAX);
        let take = buffer.len().min(room);
        let count = self.inner.read(&mut buffer[..take])?;
        self.read += count as u64;
        if self.read > self.limit.get() {
            return Err(self.refuse(Stop::Expanded));
        }
        Ok(count)
    }
}

impl<R> Guarded<'_, R> {
    fn refuse(&self, stop: Stop) -> io::Error {
        self.stop.set(Some(stop));
        io::Error::other(match stop {
            Stop::Cancelled => "cancelled",
            Stop::Expanded => "archive expands past its signed builds",
        })
    }
}

/// Downloads this release's `sheprd-acp` built for `target` into
/// `destination`, checked against the signed manifest in its archive by the
/// key this build embeds; a local build has neither and refuses. On error
/// the caller removes `destination`. Blocking: run it on a worker thread.
pub fn download_host_chat(target: &str, destination: &Path, cancel: &AtomicBool) -> Result<()> {
    let (version, key) =
        host::release_identity(crate::APP_VERSION, option_env!("HERDR_UPDATE_PUBLIC_KEY"))?;
    let response = request(
        &release_url(version, &archive_name(version)),
        RequestProfile::Archive,
        cancel,
    )?;
    unpack(
        response.into_body().into_reader(),
        version,
        key,
        target,
        destination,
        cancel,
    )
}
