//! The control channel on Unix: a socket file in the user's private state
//! directory, readable by the user alone.
use std::{
    io::{self, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn address(state: &Path) -> crate::Result<PathBuf> {
    Ok(state.join("control.sock"))
}

/// This transport can prove the connecting process's identity.
pub(super) const PEER_AUTH: bool = true;

use super::PeerCredentials;

pub(super) struct Listener(UnixListener);

impl Listener {
    /// Binds `path`, replacing a socket no process answers on. Another live
    /// server keeps its socket and this one does not start.
    pub(super) fn bind(path: &Path) -> crate::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        let listener = match UnixListener::bind(path) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
                if UnixStream::connect(path).is_ok() {
                    return Err(crate::Error::SocketInUse {
                        path: path.to_owned(),
                    });
                }
                std::fs::remove_file(path)?;
                UnixListener::bind(path)?
            }
            Err(error) => return Err(error.into()),
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Self(listener))
    }

    pub(super) fn accept(&self) -> io::Result<Connection> {
        Ok(Connection(self.0.accept()?.0))
    }
}

/// Removes the socket file when the app quits.
pub(super) fn release(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub(super) struct Connection(UnixStream);

/// The connecting process's identity, as the kernel proves it: Linux's
/// SO_PEERCRED carries uid, gid, and pid; macOS's getpeereid carries only
/// uid and gid, so the pid is unavailable there.
pub(super) fn peer_credentials(stream: &Connection) -> io::Result<PeerCredentials> {
    #[cfg(target_os = "linux")]
    {
        use nix::sys::socket::getsockopt;
        let credentials = getsockopt(&stream.0, nix::sys::socket::sockopt::PeerCredentials)
            .map_err(|error| io::Error::other(error))?;
        // A pid of 0 means the kernel could not tell; both ends then match
        // on uid and gid alone.
        let pid = credentials.pid();
        Ok(PeerCredentials {
            uid: credentials.uid(),
            gid: credentials.gid(),
            pid: (pid != 0).then_some(pid as u32),
        })
    }
    #[cfg(target_os = "macos")]
    {
        // nix 0.31's getpeereid takes `impl AsFd`; the stream handle itself,
        // since a raw fd does not implement it.
        let (uid, gid) = nix::unistd::getpeereid(&stream.0)
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;
        Ok(PeerCredentials {
            uid: uid.as_raw(),
            gid: gid.as_raw(),
            pid: None,
        })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "peer credentials are not provided on this platform",
        ))
    }
}

impl Connection {
    pub(super) fn prepare(&mut self, timeout: Duration) -> io::Result<()> {
        self.0.set_read_timeout(Some(timeout))?;
        self.0.set_write_timeout(Some(timeout))
    }

    /// Switches to nonblocking reads while the UI answers, so
    /// [`Self::peer_closed`] can look without waiting.
    pub(super) fn watch(&self, watching: bool) -> io::Result<()> {
        self.0.set_nonblocking(watching)
    }

    /// A request is one line only. EOF, another byte, or an I/O error ends
    /// it; WouldBlock means the peer is still waiting.
    pub(super) fn peer_closed(&self) -> bool {
        !matches!((&self.0).read(&mut [0]), Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted))
    }
}

impl Read for &Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.0).read(buf)
    }
}

impl Write for &Connection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&self.0).flush()
    }
}

/// Connects to the server at `path`; `io_timeout` bounds sending.
pub(super) fn connect(
    path: &Path,
    io_timeout: Duration,
    read_timeout: Duration,
) -> crate::Result<Connection> {
    let stream = UnixStream::connect(path).map_err(|source| crate::Error::Unavailable {
        path: path.to_owned(),
        source,
    })?;
    stream.set_write_timeout(Some(io_timeout))?;
    stream.set_read_timeout(Some(read_timeout))?;
    Ok(Connection(stream))
}
