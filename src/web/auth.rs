use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::net::SocketAddr;

use crate::auth::{
    generate_csrf_token, generate_password, generate_session_token, generate_unique_username,
    hash_password, validate_display_name, verify_password, MaybeUser, RequireAdmin, RequireUser,
    SessionDuration,
};
use crate::db as queries;
use crate::web::{pages, AppState};

/// Login form data.
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    action: String,
    username: Option<String>,
    password: Option<String>,
    #[serde(default)]
    remember: bool,
}

/// GET /login - Show login form.
pub async fn login_page(MaybeUser(user): MaybeUser) -> Response {
    // If already logged in, redirect to home
    if user.is_some() {
        return Redirect::to("/").into_response();
    }

    Html(pages::login_page(None, None).into_string()).into_response()
}

/// POST /login - Handle login or registration.
pub async fn login_post(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let direct_ip = addr.ip().to_string();
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(String::from);

    match form.action.as_str() {
        "register" => handle_registration(state, direct_ip, forwarded_for).await,
        "login" | "" => handle_login(state, direct_ip, forwarded_for, form).await,
        _ => (StatusCode::BAD_REQUEST, "Invalid action").into_response(),
    }
}

/// Handle user registration.
async fn handle_registration(
    state: AppState,
    ip: String,
    forwarded_for: Option<String>,
) -> Response {
    // Check rate limit: 1 registration per 5 minutes per IP
    let five_minutes_ago = Utc::now() - Duration::minutes(5);
    let count_result = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM audit_events WHERE event_type = 'registration' AND ip_address = ? AND created_at > ?",
    )
    .bind(&ip)
    .bind(five_minutes_ago.to_rfc3339())
    .fetch_one(state.db.pool())
    .await;

    if let Ok(count) = count_result {
        if count > 0 {
            return Html(
                pages::login_page(Some("Rate limit exceeded. Please try again later."), None)
                    .into_string(),
            )
            .into_response();
        }
    }

    // Generate random credentials with unique username
    let username = match generate_unique_username(state.db.pool()).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to generate unique username: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Registration failed").into_response();
        }
    };
    let password = generate_password(16);

    // Hash password
    let password_hash = match hash_password(&password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("Failed to hash password: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Registration failed").into_response();
        }
    };

    // Check if this is the first user (becomes admin)
    let user_count = queries::count_users(state.db.pool()).await.unwrap_or(0);
    let is_first_user = user_count == 0;

    // Create user
    let user_id =
        match queries::create_user(state.db.pool(), &username, &password_hash, is_first_user).await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to create user: {e}");
                return Html(
                    pages::login_page(Some("Registration failed. Please try again."), None)
                        .into_string(),
                )
                .into_response();
            }
        };

    // Log registration event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(user_id),
        "registration",
        None,
        None,
        None,
        Some(&ip),
        forwarded_for.as_deref(),
        None,
    )
    .await;

    // Show credentials to user
    Html(pages::login_page(None, Some((&username, &password))).into_string()).into_response()
}

/// Handle user login.
async fn handle_login(
    state: AppState,
    ip: String,
    forwarded_for: Option<String>,
    form: LoginForm,
) -> Response {
    let username = match form.username {
        Some(u) if !u.is_empty() => u,
        _ => {
            return Html(pages::login_page(Some("Username is required"), None).into_string())
                .into_response();
        }
    };

    let password = match form.password {
        Some(p) if !p.is_empty() => p,
        _ => {
            return Html(pages::login_page(Some("Password is required"), None).into_string())
                .into_response();
        }
    };

    // Get user by username or display_name (users can sign in with either)
    let user = match queries::get_user_by_username_or_display_name(state.db.pool(), &username).await
    {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Html(
                pages::login_page(Some("Invalid username or password"), None).into_string(),
            )
            .into_response();
        }
        Err(e) => {
            tracing::error!("Database error during login: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Login failed").into_response();
        }
    };

    // Check if user is active
    if !user.is_active {
        return Html(pages::login_page(Some("Account has been deactivated"), None).into_string())
            .into_response();
    }

    // Check if user is locked
    if let Some(locked_until) = &user.locked_until {
        let locked_time: Result<DateTime<Utc>, _> = locked_until.parse();
        if let Ok(locked_time) = locked_time {
            if locked_time > Utc::now() {
                return Html(
                    pages::login_page(
                        Some("Account is temporarily locked due to failed login attempts"),
                        None,
                    )
                    .into_string(),
                )
                .into_response();
            }
        }
    }

    // Verify password
    let password_valid = match verify_password(&password, &user.password_hash) {
        Ok(valid) => valid,
        Err(e) => {
            tracing::error!("Password verification error: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Login failed").into_response();
        }
    };

    if !password_valid {
        // Increment failed attempts
        let _ = queries::increment_failed_login_attempts(state.db.pool(), user.id).await;

        // Lock account after 5 failed attempts
        if user.failed_login_attempts >= 4 {
            let lock_duration = match user.failed_login_attempts {
                4 => 5,   // 5 minutes after 5th failure
                5 => 15,  // 15 minutes after 6th failure
                6 => 60,  // 1 hour after 7th failure
                _ => 240, // 4 hours after 8th+ failure
            };
            let locked_until = (Utc::now() + Duration::minutes(lock_duration)).to_rfc3339();
            let _ = queries::lock_user_until(state.db.pool(), user.id, &locked_until).await;
        }

        // Log failed login
        let _ = queries::create_audit_event(
            state.db.pool(),
            Some(user.id),
            "login_failed",
            None,
            None,
            None,
            Some(&ip),
            forwarded_for.as_deref(),
            None,
        )
        .await;

        return Html(pages::login_page(Some("Invalid username or password"), None).into_string())
            .into_response();
    }

    // Reset failed login attempts
    let _ = queries::reset_failed_login_attempts(state.db.pool(), user.id).await;

    // Enforce max concurrent sessions (10)
    const MAX_SESSIONS: i64 = 10;
    if let Ok(session_count) = queries::count_user_sessions(state.db.pool(), user.id).await {
        if session_count >= MAX_SESSIONS {
            // Delete oldest sessions to make room (keep MAX_SESSIONS - 1 so new one fits)
            if let Err(e) =
                queries::delete_oldest_user_sessions(state.db.pool(), user.id, MAX_SESSIONS - 1)
                    .await
            {
                tracing::warn!("Failed to delete oldest sessions: {e}");
            }
        }
    }

    // Create session
    let session_token = generate_session_token();
    let csrf_token = generate_csrf_token();
    let duration = if form.remember {
        SessionDuration::Long
    } else {
        SessionDuration::Short
    };
    let expires_at = (Utc::now() + Duration::seconds(duration.as_seconds())).to_rfc3339();

    if let Err(e) = queries::create_session(
        state.db.pool(),
        user.id,
        &session_token,
        &csrf_token,
        &ip,
        None, // user_agent would need to be extracted from headers
        &expires_at,
    )
    .await
    {
        tracing::error!("Failed to create session: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Login failed").into_response();
    }

    // Log successful login
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(user.id),
        "login_success",
        None,
        None,
        None,
        Some(&ip),
        forwarded_for.as_deref(),
        None,
    )
    .await;

    // Set session cookie
    let max_age = duration.as_seconds();
    let cookie = format!(
        "session={session_token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age}"
    );

    ([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response()
}

/// POST /logout - Log out user.
pub async fn logout(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireUser(user): RequireUser,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    // Get session token from cookie
    // (In a real implementation, we'd extract this from the request)
    // For now, delete all sessions for this user
    let _ = queries::delete_user_sessions(state.db.pool(), user.id).await;

    // Log logout event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(user.id),
        "logout",
        None,
        None,
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    // Clear session cookie
    let cookie = "session=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0";

    ([(header::SET_COOKIE, cookie)], Redirect::to("/login")).into_response()
}

/// GET /profile - User profile page.
pub async fn profile_page(
    State(state): State<AppState>,
    RequireUser(user): RequireUser,
) -> Response {
    // Check if user has a forum account link
    let has_forum_link = match queries::user_has_forum_link(state.db.pool(), user.id).await {
        Ok(linked) => linked,
        Err(e) => {
            tracing::error!("Failed to check forum link status: {e}");
            false
        }
    };

    Html(pages::profile_page_with_link_status(&user, None, has_forum_link).into_string())
        .into_response()
}

/// POST /profile - Update user profile.
#[derive(Debug, Deserialize)]
pub struct ProfileForm {
    email: Option<String>,
    display_name: Option<String>,
    current_password: Option<String>,
    new_password: Option<String>,
    confirm_password: Option<String>,
}

pub async fn profile_post(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    RequireUser(user): RequireUser,
    Form(form): Form<ProfileForm>,
) -> Response {
    let mut error: Option<String> = None;
    let mut password_changed = false;

    // Check if user has a forum account link (prevents display_name changes)
    let has_forum_link = match queries::user_has_forum_link(state.db.pool(), user.id).await {
        Ok(linked) => linked,
        Err(e) => {
            tracing::error!("Failed to check forum link status: {e}");
            false
        }
    };

    // Extract current session token from cookie for later use
    let current_token = headers
        .get("cookie")
        .and_then(|h| h.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .find_map(|cookie| cookie.trim().strip_prefix("session="))
        })
        .map(String::from);

    // Update email and display name if changed
    let email = form.email.filter(|e| !e.is_empty());

    // If user has forum link, keep existing display_name (ignore form input)
    // Otherwise, process the form's display_name value
    let display_name = if has_forum_link {
        // User is linked to forum account - keep existing display_name
        user.display_name.as_deref()
    } else {
        // Empty string means clear display_name (reset to null)
        form.display_name
            .as_ref()
            .filter(|d| !d.is_empty())
            .map(|s| s.as_str())
    };

    // Store old display_name for audit logging
    let old_display_name = user.display_name.clone();

    // Validate display_name if provided and not linked
    if !has_forum_link {
        if let Some(dn) = &display_name {
            // Validate format (1-20 chars, no spaces)
            if let Err(e) = validate_display_name(dn) {
                error = Some(e.to_string());
            } else {
                // Check uniqueness (excluding current user)
                match queries::display_name_exists(state.db.pool(), dn, Some(user.id)).await {
                    Ok(true) => {
                        error = Some("Display name is already taken".to_string());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::error!("Failed to check display_name uniqueness: {e}");
                        error = Some("Failed to update profile".to_string());
                    }
                }
            }
        }
    }

    // Only update profile if no validation errors so far
    if error.is_none() {
        if let Err(e) =
            queries::update_user_profile(state.db.pool(), user.id, email.as_deref(), display_name)
                .await
        {
            tracing::error!("Failed to update profile: {e}");
            error = Some("Failed to update profile".to_string());
        }
    }

    // Handle password change if requested
    if let (Some(current), Some(new), Some(confirm)) = (
        &form.current_password,
        &form.new_password,
        &form.confirm_password,
    ) {
        if !current.is_empty() && !new.is_empty() {
            // Verify current password
            let password_valid = match verify_password(current, &user.password_hash) {
                Ok(valid) => valid,
                Err(e) => {
                    tracing::error!("Password verification error: {e}");
                    error = Some("Failed to change password".to_string());
                    false
                }
            };

            if !password_valid {
                error = Some("Current password is incorrect".to_string());
            } else if new != confirm {
                error = Some("New passwords do not match".to_string());
            } else {
                // Hash new password
                match hash_password(new) {
                    Ok(new_hash) => {
                        if let Err(e) =
                            queries::update_user_password(state.db.pool(), user.id, &new_hash).await
                        {
                            tracing::error!("Failed to update password: {e}");
                            error = Some("Failed to change password".to_string());
                        } else {
                            password_changed = true;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to hash password: {e}");
                        error = Some("Failed to change password".to_string());
                    }
                }
            }
        }
    }

    // If password changed, invalidate all other sessions
    if password_changed {
        if let Some(token) = &current_token {
            match queries::delete_other_user_sessions(state.db.pool(), user.id, token).await {
                Ok(count) => {
                    tracing::info!(
                        user_id = user.id,
                        invalidated = count,
                        "Password changed, other sessions invalidated"
                    );
                }
                Err(e) => {
                    tracing::error!("Failed to invalidate other sessions: {e}");
                }
            }
        }
    }

    // Reload user and show profile page
    let updated_user = queries::get_user_by_id(state.db.pool(), user.id)
        .await
        .ok()
        .flatten()
        .unwrap_or(user);

    // Audit log display_name change if it changed (and no error occurred)
    if error.is_none() && !has_forum_link {
        let new_display_name = updated_user.display_name.as_deref();
        if old_display_name.as_deref() != new_display_name {
            let metadata = serde_json::json!({
                "old_value": old_display_name,
                "new_value": new_display_name,
            });
            if let Err(e) = queries::create_audit_event(
                state.db.pool(),
                Some(updated_user.id),
                "display_name_changed",
                Some("user"),
                Some(updated_user.id),
                Some(&metadata.to_string()),
                None,
                None,
                None,
            )
            .await
            {
                tracing::error!("Failed to create audit event for display_name change: {e}");
            }
        }
    }

    Html(
        pages::profile_page_with_link_status(&updated_user, error.as_deref(), has_forum_link)
            .into_string(),
    )
    .into_response()
}

/// GET /admin - Admin panel.
pub async fn admin_panel(
    State(state): State<AppState>,
    RequireAdmin(admin): RequireAdmin,
) -> Response {
    // Get all users
    let users = match queries::get_all_users(state.db.pool(), 100, 0).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("Failed to fetch users: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to load users").into_response();
        }
    };

    // Get recent audit events
    let audit_events = match queries::get_audit_events(state.db.pool(), 50, 0).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("Failed to fetch audit events: {e}");
            vec![]
        }
    };

    Html(pages::render_admin_panel(&users, &audit_events, &admin).into_string()).into_response()
}

/// POST /admin/user/:id/approve - Approve a user.
#[derive(Debug, Deserialize)]
pub struct UserIdForm {
    user_id: i64,
}

pub async fn admin_approve_user(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    if let Err(e) = queries::update_user_approval(state.db.pool(), form.user_id, true).await {
        tracing::error!("Failed to approve user: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to approve user").into_response();
    }

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "user_approved",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    Redirect::to("/admin").into_response()
}

/// POST /admin/user/:id/revoke - Revoke user approval.
pub async fn admin_revoke_user(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    if let Err(e) = queries::update_user_approval(state.db.pool(), form.user_id, false).await {
        tracing::error!("Failed to revoke user approval: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to revoke approval",
        )
            .into_response();
    }

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "user_approval_revoked",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    Redirect::to("/admin").into_response()
}

/// POST /admin/user/:id/promote - Promote user to admin.
pub async fn admin_promote_user(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    if let Err(e) = queries::update_user_admin(state.db.pool(), form.user_id, true).await {
        tracing::error!("Failed to promote user: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to promote user").into_response();
    }

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "user_promoted_admin",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    Redirect::to("/admin").into_response()
}

/// POST /admin/user/:id/demote - Demote admin to regular user.
pub async fn admin_demote_user(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    // Prevent self-demotion
    if admin.id == form.user_id {
        return (StatusCode::BAD_REQUEST, "Cannot demote yourself").into_response();
    }

    if let Err(e) = queries::update_user_admin(state.db.pool(), form.user_id, false).await {
        tracing::error!("Failed to demote user: {e}");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to demote user").into_response();
    }

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "user_demoted_admin",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    Redirect::to("/admin").into_response()
}

/// POST /admin/user/:id/deactivate - Deactivate user account.
pub async fn admin_deactivate_user(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    // Prevent self-deactivation
    if admin.id == form.user_id {
        return (StatusCode::BAD_REQUEST, "Cannot deactivate yourself").into_response();
    }

    if let Err(e) = queries::update_user_active(state.db.pool(), form.user_id, false).await {
        tracing::error!("Failed to deactivate user: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to deactivate user",
        )
            .into_response();
    }

    // Delete all sessions for deactivated user
    let _ = queries::delete_user_sessions(state.db.pool(), form.user_id).await;

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "user_deactivated",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    Redirect::to("/admin").into_response()
}

/// POST /admin/user/:id/reactivate - Reactivate user account.
pub async fn admin_reactivate_user(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    if let Err(e) = queries::update_user_active(state.db.pool(), form.user_id, true).await {
        tracing::error!("Failed to reactivate user: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to reactivate user",
        )
            .into_response();
    }

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "user_reactivated",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    Redirect::to("/admin").into_response()
}

/// POST /admin/user/reset-password - Reset user password (admin).
pub async fn admin_reset_password(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<UserIdForm>,
) -> Response {
    let ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok());

    // Get target user info
    let target_user = match queries::get_user_by_id(state.db.pool(), form.user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "User not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to get user: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to reset password",
            )
                .into_response();
        }
    };

    // Generate new password
    let new_password = generate_password(16);
    let password_hash = match hash_password(&new_password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("Failed to hash password: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to reset password",
            )
                .into_response();
        }
    };

    // Update password
    if let Err(e) =
        queries::update_user_password(state.db.pool(), form.user_id, &password_hash).await
    {
        tracing::error!("Failed to update password: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to reset password",
        )
            .into_response();
    }

    // Invalidate all sessions for the user
    let _ = queries::delete_user_sessions(state.db.pool(), form.user_id).await;

    // Log audit event
    let _ = queries::create_audit_event(
        state.db.pool(),
        Some(admin.id),
        "admin_password_reset",
        Some("user"),
        Some(form.user_id),
        None,
        Some(&ip),
        forwarded_for,
        None,
    )
    .await;

    tracing::info!(
        admin_id = admin.id,
        target_user_id = form.user_id,
        "Admin reset user password"
    );

    // Show the new password (one-time display)
    Html(
        pages::render_admin_password_reset_result(&target_user.username, &new_password)
            .into_string(),
    )
    .into_response()
}

// ============================================================================
// Excluded Domains Admin Functions
// ============================================================================

/// GET /admin/excluded-domains - Show excluded domains management page.
pub async fn admin_excluded_domains_page(
    State(state): State<AppState>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    match queries::get_all_excluded_domains(state.db.pool()).await {
        Ok(domains) => {
            Html(pages::render_admin_excluded_domains_page(&domains, None).into_string())
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to fetch excluded domains: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load excluded domains",
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ExcludedDomainForm {
    domain: String,
    reason: Option<String>,
}

/// POST /admin/excluded-domains/add - Add a new excluded domain.
pub async fn admin_add_excluded_domain(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<ExcludedDomainForm>,
) -> Response {
    let direct_ip = addr.ip().to_string();
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    // Normalize the domain
    let domain = form.domain.trim().to_lowercase();
    if domain.is_empty() {
        return (StatusCode::BAD_REQUEST, "Domain cannot be empty").into_response();
    }

    let reason = form
        .reason
        .unwrap_or_else(|| "User-added exclusion".to_string());

    match queries::add_excluded_domain(state.db.pool(), &domain, &reason, Some(admin.id)).await {
        Ok(_) => {
            tracing::info!(admin_id = admin.id, domain = %domain, "Admin added excluded domain");

            // Log audit event
            let _ = queries::create_audit_event(
                state.db.pool(),
                Some(admin.id),
                "admin_add_excluded_domain",
                Some("excluded_domain"),
                None,
                Some(&domain),
                Some(&direct_ip),
                forwarded_for.as_deref(),
                None,
            )
            .await;

            // Redirect back to the excluded domains page
            Redirect::to("/admin/excluded-domains").into_response()
        }
        Err(e) => {
            tracing::error!("Failed to add excluded domain: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to add excluded domain",
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ExcludedDomainActionForm {
    domain: String,
}

/// POST /admin/excluded-domains/toggle - Toggle excluded domain active status.
pub async fn admin_toggle_excluded_domain(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<ExcludedDomainActionForm>,
) -> Response {
    let direct_ip = addr.ip().to_string();
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let domain = form.domain.trim().to_lowercase();

    // First get current status
    match queries::get_all_excluded_domains(state.db.pool()).await {
        Ok(domains) => {
            if let Some(d) = domains
                .iter()
                .find(|x: &&queries::ExcludedDomain| x.domain == domain)
            {
                match queries::update_excluded_domain_status(state.db.pool(), &domain, !d.is_active)
                    .await
                {
                    Ok(_) => {
                        let action = if d.is_active { "disabled" } else { "enabled" };
                        tracing::info!(
                            admin_id = admin.id,
                            domain = %domain,
                            action = %action,
                            "Admin toggled excluded domain"
                        );

                        // Log audit event
                        let _ = queries::create_audit_event(
                            state.db.pool(),
                            Some(admin.id),
                            "admin_toggle_excluded_domain",
                            Some("excluded_domain"),
                            None,
                            Some(&format!("{domain} -> {action}")),
                            Some(&direct_ip),
                            forwarded_for.as_deref(),
                            None,
                        )
                        .await;

                        Redirect::to("/admin/excluded-domains").into_response()
                    }
                    Err(e) => {
                        tracing::error!("Failed to update excluded domain: {e}");
                        (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed to update excluded domain",
                        )
                            .into_response()
                    }
                }
            } else {
                (StatusCode::NOT_FOUND, "Domain not found").into_response()
            }
        }
        Err(e) => {
            tracing::error!("Failed to fetch excluded domains: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch excluded domains",
            )
                .into_response()
        }
    }
}

/// POST /admin/excluded-domains/delete - Delete an excluded domain.
pub async fn admin_delete_excluded_domain(
    State(state): State<AppState>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    RequireAdmin(admin): RequireAdmin,
    Form(form): Form<ExcludedDomainActionForm>,
) -> Response {
    let direct_ip = addr.ip().to_string();
    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    let domain = form.domain.trim().to_lowercase();

    match queries::delete_excluded_domain(state.db.pool(), &domain).await {
        Ok(_) => {
            tracing::info!(admin_id = admin.id, domain = %domain, "Admin deleted excluded domain");

            // Log audit event
            let _ = queries::create_audit_event(
                state.db.pool(),
                Some(admin.id),
                "admin_delete_excluded_domain",
                Some("excluded_domain"),
                None,
                Some(&domain),
                Some(&direct_ip),
                forwarded_for.as_deref(),
                None,
            )
            .await;

            Redirect::to("/admin/excluded-domains").into_response()
        }
        Err(e) => {
            tracing::error!("Failed to delete excluded domain: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete excluded domain",
            )
                .into_response()
        }
    }
}

/// GET /admin/user/:id - Show user profile (admin-only).
pub async fn admin_user_profile(
    State(state): State<AppState>,
    axum::extract::Path(user_id): axum::extract::Path<i64>,
    RequireAdmin(admin): RequireAdmin,
) -> Response {
    // Fetch the user
    let user = match queries::get_user_by_id(state.db.pool(), user_id).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "User not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch user: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Fetch forum account link if it exists
    let forum_link = match queries::get_forum_link_by_user_id(state.db.pool(), user_id).await {
        Ok(link) => link,
        Err(e) => {
            tracing::warn!("Failed to fetch forum link for user {}: {e}", user_id);
            None
        }
    };

    // Fetch audit events for this user
    let audit_events =
        match queries::get_audit_events_for_user(state.db.pool(), user_id, 50, 0).await {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!("Failed to fetch audit events for user {}: {e}", user_id);
                Vec::new()
            }
        };

    Html(
        pages::render_admin_user_profile(&user, forum_link.as_ref(), &audit_events, &admin)
            .into_string(),
    )
    .into_response()
}

/// GET /admin/forum-user/:username - Show forum user profile (admin-only).
pub async fn admin_forum_user_profile(
    State(state): State<AppState>,
    axum::extract::Path(forum_username): axum::extract::Path<String>,
    RequireAdmin(admin): RequireAdmin,
) -> Response {
    // Fetch forum account link
    let forum_link =
        match queries::get_forum_link_by_forum_username(state.db.pool(), &forum_username).await {
            Ok(Some(link)) => link,
            Ok(None) => {
                return (StatusCode::NOT_FOUND, "Forum user not found").into_response();
            }
            Err(e) => {
                tracing::error!("Failed to fetch forum link: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        };

    // Fetch the linked user
    let user = match queries::get_user_by_id(state.db.pool(), forum_link.user_id).await {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!("Failed to fetch user for forum link: {e}");
            None
        }
    };

    Html(pages::render_admin_forum_user_profile(&forum_link, user.as_ref(), &admin).into_string())
        .into_response()
}
