//! Strongly-typed ID newtypes over `ulid::Ulid`.
//!
//! Each entity in the system gets its own type to prevent
//! accidental cross-wiring (passing a `RunId` where a `SessionId`
//! is expected). All IDs are ULIDs (monotonic, sortable, URL-safe,
//! 128-bit, no central authority needed).
//!
//! Conventions:
//! - `Display` impls produce the canonical 26-char Crockford
//!   base32 string (same as `ulid::Ulid::to_string`).
//! - `FromStr` parses the same string. Both round-trip cleanly.
//! - `Serialize` / `Deserialize` use the same string form on the
//!   wire (no struct wrapping).
//! - `serde(rename_all = "camelCase")` is **not** applied at the
//!   ID level because IDs are atomic and should never need it.
//!
//! See `../../specs/glossary.md` for the canonical list of
//! entities and their ID types.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ulid::Ulid;

/// Generate a new monotonic ULID. Wrapper around `Ulid::new` for
/// future swappability (e.g. test injection).
#[must_use]
pub fn new_ulid() -> Ulid {
    Ulid::new()
}

/// Macro for declaring a typed ID. The resulting type is a tuple
/// struct holding a `Ulid`; it gets `Display`, `FromStr`, `Serialize`,
/// `Deserialize`, `Clone`, `Copy`, `Eq`, `Hash`, and `Debug` for
/// free. It also gets `Deref<Target = Ulid>` so `.0` is unnecessary
/// in most call sites (you can pass `&SessionId` where `&Ulid` is
/// expected).
macro_rules! id_type {
    (
        $(#[$attr:meta])*
        $vis:vis struct $name:ident;
    ) => {
        $(#[$attr])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
        )]
        $vis struct $name(Ulid);

        impl $name {
            /// Create a fresh ID with a new ULID.
            #[must_use]
            pub fn new() -> Self {
                Self(new_ulid())
            }

            /// Construct from a `Ulid` without generating a new one.
            /// Used in tests and in journal replay.
            #[must_use]
            pub const fn from_ulid(u: Ulid) -> Self {
                Self(u)
            }

            /// Get the inner `Ulid`.
            #[must_use]
            pub const fn ulid(&self) -> Ulid {
                self.0
            }

            /// Get the epoch milliseconds embedded in the ULID.
            #[must_use]
            pub fn timestamp_ms(&self) -> u64 {
                self.0.timestamp_ms()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = ulid::DecodeError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Ulid::from_str(s)?))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0.to_string())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                use serde::de::Error;
                let s = String::deserialize(d)?;
                Ulid::from_str(&s)
                    .map(Self)
                    .map_err(|e| D::Error::custom(format!("invalid ULID: {e}")))
            }
        }
    };
}

id_type! {
    /// A workspace is a project the user has opened. There can be
    /// many workspaces; each has its own config, state, journal.
    pub struct WorkspaceId;
}

id_type! {
    /// A conversation thread within a workspace. Contains many runs.
    pub struct SessionId;
}

id_type! {
    /// A single agent loop execution (one `session_send`).
    /// May contain a tree of `RunId`s when subagents are involved
    /// (parent_run_id points up the tree).
    pub struct RunId;
}

id_type! {
    /// A persisted chat message (user, assistant, tool, etc.).
    pub struct MessageId;
}

id_type! {
    /// A single invocation of a tool by the agent loop.
    pub struct ToolCallId;
}

id_type! {
    /// A permission prompt sent to the user (and the response).
    pub struct PermissionRequestId;
}

id_type! {
    /// An agent definition (built-in or custom).
    pub struct AgentId;
}

id_type! {
    /// A workspace's extra_path entry (see ADR-0007).
    pub struct ExtraPathId;
}

/// Re-export the underlying `ulid` crate for advanced uses.
pub use ulid;
