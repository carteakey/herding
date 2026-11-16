# Failures installing herdr-core (Herdr 2) on an SSH host, cloud machine, or
# WSL distribution. curl, sha256sum, and shasum are command names.

error-remote-core-install-checksum = the herdr-core download does not match its pinned checksum
error-remote-core-install-dir = herdr-core's install folder must be an absolute path
error-remote-core-install-no-build = herdr-core has no build for this machine
error-remote-core-install-no-checksum-tool = the machine has no sha256sum or shasum to check herdr-core
error-remote-core-install-no-curl = the machine has no curl to download herdr-core
# $target is herdr-core's name for a build, such as linux-x86_64.
error-remote-core-install-no-pinned-build = the pinned herdr-core has no usable { $target } build
error-remote-core-install-pinned = the pinned herdr-core release is unreadable
error-remote-core-install-unfinished = the herdr-core installer did not finish
