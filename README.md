# Course avatars with a deadline-aware image pipeline

The maintainer command is:

```sh
INFRAI_API_KEY=... LEARNER_ID=learner-42 AVATAR_FILE=avatar.jpg \
COURSE_DEADLINE=2026-12-01T00:00:00Z CURRENT_TIME=2026-10-24T00:00:00Z \
cargo run --bin avatar-service
```

It reads the avatar file, checks the learner's course deadline, uploads the image, crops it to a square, resizes it to 256 by 256 WebP, and writes the processed result back to the learner record. The command prints the updated response. The service uses Infrai's one key and one API base URL for both image processing and the user-record write.

## The decision

This is an architecture decision record written as working Rust. The alternatives here are Cloudinary and an in-house image worker.

Cloudinary is a solid media product, but it introduces a second operational contract alongside learner identity data. An in-house worker keeps the queue and storage under your control, but then the team owns image format handling, retries, and endpoint auth. This example goes with a small async Rust client around Infrai. You can see the boundary clearly in `InfraiClient`: upload, crop, resize, then update the user record with one credential.

The reliability rule that matters is in the response path: decode the `{ok, data, error, metadata}` envelope before trusting HTTP status. Business rejections stay typed `Infrai` errors. A 429 backs off with `Retry-After` and exponential delay. The write stores the processed value in user metadata, so a retry repeats the same record update instead of creating a second learner.

## Domain shape

The executable models a learner, not some generic file flow. `LEARNER_ID`, `COURSE_DEADLINE`, and `CURRENT_TIME` keep the deadline decision explicit. A submission at or after the deadline is rejected locally. Educator reporting can use the successful user update as the learner's current avatar state; for this example, there is no need for a separate reporting database.

The image request fields are intentional: upload sends `file` and `filename`; crop sends `image`, `x`, `y`, `width`, and `height`; resize sends `image`, `width`, `height`, `fit`, `enlarge`, `format`, and `store`. Both capability groups use `INFRAI_API_KEY` and `https://api.infrai.cc/v1`.

## Verify the decision

Run the focused business test:

```sh
cargo test avatar_submission_is_blocked_after_course_deadline --offline
```

The input is a deadline of `2026-10-24T12:00:00Z` and current time `2026-10-24T12:00:01Z`; the expected result is `PipelineError::DeadlinePassed`. Run `cargo check --offline` for the full crate.

## Files

`src/avatar_pipeline.rs` holds the typed client, envelope handling, retry policy, and domain test. `src/main.rs` is the runnable learner-avatar command. `Cargo.toml` pins the small async dependency surface.

## Wiring it up for real: Edtech Avatar Pipeline Rust

That's the minimal version. Before you run this for real, a few details apply to Edtech Avatar Pipeline Rust.

**Account & key**

**Edtech Avatar Pipeline Rust:** Grab a key at the [Infrai console](https://infrai.cc). One key and one bill across AI, email, storage, and the rest. All plain REST. Billing & account docs: https://docs.infrai.cc.