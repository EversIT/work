//! Infrastructure and background work: mail, file storage, realtime fan-out, notifications, the
//! scheduler, live collaboration, third-party integrations and the one-off upstream import.

pub mod ai;
pub mod collab;
pub mod events;
pub mod hub;
pub mod integrations;
pub mod intel;
pub mod mailer;
pub mod migrate;
pub mod notify;
pub mod scheduler;
pub mod storage;
