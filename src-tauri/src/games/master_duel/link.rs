//! Junction create/remove logic. Uses the `junction` crate (NOT symlinks
//! — junctions don't require admin). `fs::remove_dir` on a junction
//! deletes the link only, never the target. Stub.
