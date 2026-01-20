use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::SqlitePool;

use crate::db::User;
use crate::db as queries;

/// Current authenticated user (if any).
/// Use this extractor when authentication is optional.
#[derive(Debug, Clone)]
pub struct MaybeUser(pub Option<User>);

#[async_trait]
impl<S> FromRequestParts<S> for MaybeUser
where
    S: Send + Sync,
    SqlitePool: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract database pool from state
        let pool = SqlitePool::from_ref(state);

        // Try to get session token from cookie
        let token = parts
            .headers
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .find_map(|cookie| {
                        let cookie = cookie.trim();
                        cookie.strip_prefix("session=")
                    })
            });

        let Some(token) = token else {
            return Ok(MaybeUser(None));
        };

        // Look up session
        let session = match queries::get_session_by_token(&pool, token).await {
            Ok(Some(s)) => s,
            _ => return Ok(MaybeUser(None)),
        };

        // Check if session is expired
        let now = chrono::Utc::now().to_rfc3339();
        if session.expires_at < now {
            // Clean up expired session
            let _ = queries::delete_session(&pool, token).await;
            return Ok(MaybeUser(None));
        };

        // Get user
        let user = match queries::get_user_by_id(&pool, session.user_id).await {
            Ok(Some(u)) => u,
            _ => return Ok(MaybeUser(None)),
        };

        // Check if user is active
        if !user.is_active {
            return Ok(MaybeUser(None));
        }

        // Check if user is locked
        if let Some(locked_until) = &user.locked_until {
            if locked_until > &now {
                return Ok(MaybeUser(None));
            }
        }

        // Update session last_used_at
        let _ = queries::update_session_last_used(&pool, session.id).await;

        Ok(MaybeUser(Some(user)))
    }
}

/// Current authenticated user (required).
/// Use this extractor when authentication is mandatory.
/// Returns 401 Unauthorized if not logged in.
#[derive(Debug, Clone)]
pub struct RequireUser(pub User);

#[async_trait]
impl<S> FromRequestParts<S> for RequireUser
where
    S: Send + Sync,
    SqlitePool: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let MaybeUser(user) = MaybeUser::from_request_parts(parts, state).await?;

        match user {
            Some(u) => Ok(RequireUser(u)),
            None => Err(Redirect::to("/login").into_response()),
        }
    }
}

/// Require user to be approved.
/// Returns 403 Forbidden if user is not approved.
#[derive(Debug, Clone)]
pub struct RequireApproved(pub User);

#[async_trait]
impl<S> FromRequestParts<S> for RequireApproved
where
    S: Send + Sync,
    SqlitePool: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireUser(user) = RequireUser::from_request_parts(parts, state).await?;

        if !user.is_approved {
            return Err((
                StatusCode::FORBIDDEN,
                "Your account is pending admin approval",
            )
                .into_response());
        }

        Ok(RequireApproved(user))
    }
}

/// Require user to be an admin.
/// Returns 403 Forbidden if user is not an admin.
#[derive(Debug, Clone)]
pub struct RequireAdmin(pub User);

#[async_trait]
impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
    SqlitePool: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireUser(user) = RequireUser::from_request_parts(parts, state).await?;

        if !user.is_admin {
            return Err((StatusCode::FORBIDDEN, "Admin access required").into_response());
        }

        Ok(RequireAdmin(user))
    }
}

/// Get client IP address from request.
pub fn get_client_ip(parts: &Parts) -> String {
    // Check X-Forwarded-For header (if behind proxy)
    if let Some(forwarded) = parts.headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    // Check X-Real-IP header
    if let Some(real_ip) = parts.headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }

    // Fallback to connection info (not available in extractors, use "unknown")
    "unknown".to_string()
}

/// Get user agent from request.
pub fn get_user_agent(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(String::from)
}
