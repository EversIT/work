//! An in-process client of this server's own HTTP API, acting as whichever user a call names.

use std::collections::HashMap;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

use crate::{security::auth::issue_access_token, state::AppState};

pub(super) struct Api {
    app: Router,
    state: AppState,
    /// Sessions created for the import; revoked at the end.
    sessions: HashMap<Uuid, Uuid>,
}

impl Api {
    pub(super) fn new(state: &AppState) -> Self {
        Self {
            app: crate::app::app(state.clone()),
            state: state.clone(),
            sessions: HashMap::new(),
        }
    }

    async fn session(&mut self, user: Uuid) -> Result<Uuid, sqlx::Error> {
        if let Some(s) = self.sessions.get(&user) {
            return Ok(*s);
        }
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO sessions (id, family_id, user_id, token_hash, expires_at) VALUES ($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(Uuid::new_v4())
            .bind(user)
            .bind(crate::security::auth::hash_token(&crate::security::auth::random_token()))
            .bind(Utc::now() + Duration::hours(12))
            .execute(&self.state.db)
            .await?;
        self.sessions.insert(user, id);
        Ok(id)
    }

    /// `Err((status, message))` carries the API's own error text.
    pub(super) async fn call(
        &mut self,
        user: Uuid,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, (StatusCode, String)> {
        let session = self
            .session(user)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        // Access tokens are short-lived; mint a fresh one per call.
        let token = issue_access_token(&self.state.config.jwt_secret, user, session)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "token".into()))?;
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
            .expect("valid request");
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:0"
                .parse::<std::net::SocketAddr>()
                .expect("socket address"),
        ));
        let resp = self.app.clone().oneshot(req).await.expect("infallible");
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap_or_default();
        let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if status.is_success() {
            Ok(json)
        } else {
            Err((
                status,
                json["error"]
                    .as_str()
                    .unwrap_or("request failed")
                    .to_owned(),
            ))
        }
    }

    pub(super) async fn finish(self) {
        let ids: Vec<Uuid> = self.sessions.values().copied().collect();
        let _ = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = ANY($1)")
            .bind(&ids)
            .execute(&self.state.db)
            .await;
    }
}

pub(super) fn id_of(v: &Value) -> Option<Uuid> {
    v["id"].as_str().and_then(|s| Uuid::parse_str(s).ok())
}
