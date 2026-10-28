//! End-to-end tests. They run against a real Postgres (`DATABASE_URL`), and some also against
//! Mailpit, S3Mock and Azurite; see docs/DEVELOPMENT.md.

mod accounts;
mod admin;
mod ai;
mod collab;
mod domain;
mod harness;
mod hooks;
mod links;
mod migrate;
mod passkeys;
mod security;
mod sso;
mod storage;
