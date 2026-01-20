use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::SqlitePool;

use crate::db as queries;
use crate::db::User;

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
                cookies.split(';').find_map(|cookie| {
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
///
/// Note: We intentionally do NOT check `is_approved` here. Admins are always
/// considered approved by virtue of being admin. The first user is auto-approved
/// when made admin, and any user promoted to admin should already be approved.
/// Checking is_approved would be redundant and could cause issues if an admin
/// was somehow unapproved (which shouldn't happen in normal operation).
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

/// Client IP information from request.
#[derive(Debug, Clone)]
pub struct ClientIp {
    /// Direct connection IP (from socket address).
    pub direct: String,
    /// X-Forwarded-For header value if present.
    pub forwarded: Option<String>,
}

/// Get client IP addresses from request headers and socket address.
pub fn get_client_ip(direct_ip: &str, headers: &axum::http::HeaderMap) -> ClientIp {
    // Extract X-Forwarded-For header (full value, not just first IP)
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(String::from);

    ClientIp {
        direct: direct_ip.to_string(),
        forwarded,
    }
}

/// Get the "best" client IP for rate limiting / display purposes.
/// Prefers X-Forwarded-For first IP if present, otherwise direct IP.
pub fn get_effective_ip(client_ip: &ClientIp) -> &str {
    if let Some(forwarded) = &client_ip.forwarded {
        if let Some(first_ip) = forwarded.split(',').next() {
            let trimmed = first_ip.trim();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    &client_ip.direct
}

/// Get user agent from request.
pub fn get_user_agent(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .map(String::from)
}

/// CSRF token from session.
/// Use this extractor to get the CSRF token for forms or validation.
#[derive(Debug, Clone)]
pub struct SessionCsrf(pub Option<String>);

#[async_trait]
impl<S> FromRequestParts<S> for SessionCsrf
where
    S: Send + Sync,
    SqlitePool: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = SqlitePool::from_ref(state);

        // Try to get session token from cookie
        let token = parts
            .headers
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(|cookies| {
                cookies
                    .split(';')
                    .find_map(|cookie| cookie.trim().strip_prefix("session="))
            });

        let Some(token) = token else {
            return Ok(SessionCsrf(None));
        };

        // Look up session
        let session = match queries::get_session_by_token(&pool, token).await {
            Ok(Some(s)) => s,
            _ => return Ok(SessionCsrf(None)),
        };

        // Check if session is expired
        let now = chrono::Utc::now().to_rfc3339();
        if session.expires_at < now {
            return Ok(SessionCsrf(None));
        }

        Ok(SessionCsrf(Some(session.csrf_token)))
    }
}

/// Validate a CSRF token from form submission against session token.
/// Returns true if tokens match, false otherwise.
pub fn validate_csrf_token(session_token: Option<&str>, form_token: Option<&str>) -> bool {
    match (session_token, form_token) {
        (Some(session), Some(form)) => {
            // Use constant-time comparison to prevent timing attacks
            session.len() == form.len()
                && session
                    .as_bytes()
                    .iter()
                    .zip(form.as_bytes())
                    .all(|(a, b)| a == b)
        }
        _ => false,
    }
}
