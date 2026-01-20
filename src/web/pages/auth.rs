//! Authentication pages for the web UI.
//!
//! This module provides maud-based templates for login and profile pages.

use maud::{html, Markup, PreEscaped, Render};

use crate::components::{Alert, BaseLayout, Button, Checkbox, Form, HiddenInput, Input, StatusBox};
use crate::db::User;

/// JavaScript for copying the link command to clipboard.
const COPY_LINK_COMMAND_SCRIPT: &str = r#"
function copyLinkCommand() {
    var text = document.getElementById('link-command').textContent;
    navigator.clipboard.writeText(text).then(function() {
        var btn = event.target;
        var orig = btn.textContent;
        btn.textContent = 'Copied!';
        setTimeout(function() { btn.textContent = orig; }, 1500);
    });
}
"#;

/// Render the login page.
///
/// # Arguments
///
/// * `error` - Optional error message to display
/// * `credentials` - Optional tuple of (username, password) for newly registered accounts
///
/// # Example
///
/// ```ignore
/// // Normal login page
/// let page = render_login_page(None, None);
///
/// // Login page with error
/// let page = render_login_page(Some("Invalid username or password"), None);
///
/// // After successful registration
/// let page = render_login_page(None, Some(("user123", "p@ssw0rd!")));
/// ```
#[must_use]
pub fn render_login_page(error: Option<&str>, credentials: Option<(&str, &str)>) -> Markup {
    let content = if let Some((username, password)) = credentials {
        // Show generated credentials after registration
        render_registration_success(username, password)
    } else {
        // Normal login form
        render_login_form(error)
    };

    BaseLayout::new("Login").render(content)
}

/// Render the registration success page with credentials.
fn render_registration_success(username: &str, password: &str) -> Markup {
    html! {
        div class="auth-container" style="max-width: 500px; margin: 2rem auto;" {
            // Success message with credentials
            div class="status-box status-box-success" {
                h2 style="margin-top: 0;" { "Account Created!" }
                p style="font-weight: 600;" {
                    "Save these credentials now. They will not be shown again:"
                }
                div class="credentials-box" style="background: var(--bg-primary, #ffffff); color: var(--text-primary, #18181b); padding: var(--spacing-md, 1rem); border-radius: var(--radius, 0.375rem); font-family: monospace; margin: var(--spacing-md, 1rem) 0; border: 1px solid var(--border-color, #e4e4e7);" {
                    div style="margin-bottom: var(--spacing-sm, 0.5rem);" {
                        strong { "Username: " }
                        (username)
                    }
                    div {
                        strong { "Password: " }
                        (password)
                    }
                }
                p style="font-size: var(--font-size-sm, 0.875rem);" {
                    "Your account is pending admin approval before you can submit links."
                }
            }

            // Login form below the success message
            (render_login_form_content(None))
        }
    }
}

/// Render the login form page content.
fn render_login_form(error: Option<&str>) -> Markup {
    html! {
        div class="auth-container" style="max-width: 500px; margin: 2rem auto;" {
            h1 { "Login" }

            @if let Some(e) = error {
                (Alert::error(e))
            }

            (render_login_form_content(error))

            // Register section
            div style="margin-top: var(--spacing-lg, 1.5rem); padding-top: var(--spacing-lg, 1.5rem); border-top: 1px solid var(--border-color, #e4e4e7); text-align: center;" {
                p style="font-size: var(--font-size-sm, 0.875rem); color: var(--text-secondary, #52525b); margin-bottom: var(--spacing-sm, 0.5rem);" {
                    "Don't have an account?"
                }
                (Form::post("/login", html! {
                    (HiddenInput::new("action", "register"))
                    (Button::outline("Register").r#type("submit"))
                }))
            }
        }
    }
}

/// Render just the login form content (used by both login and post-registration views).
fn render_login_form_content(error: Option<&str>) -> Markup {
    let _ = error; // Used only for context in parent

    html! {
        h1 { "Login" }

        (Form::post("/login", html! {
            (HiddenInput::new("action", "login"))

            div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                label for="username" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                    "Username"
                }
                (Input::text("username").id("username").required())
            }

            div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                label for="password" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                    "Password"
                }
                (Input::password("password").id("password").required())
            }

            div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                (Checkbox::new("remember").value("true").id("remember").label("Remember me for 30 days"))
            }

            (Button::primary("Login").r#type("submit"))
        }))
    }
}

/// Parameters for rendering the profile page.
#[derive(Debug)]
pub struct ProfilePageParams<'a> {
    /// The current user
    pub user: &'a User,
    /// Optional message to display (success/error)
    pub message: Option<&'a str>,
    /// Whether the message is an error (vs success)
    pub is_error: bool,
    /// Whether the user has a linked forum account
    pub has_forum_link: bool,
}

impl<'a> ProfilePageParams<'a> {
    /// Create profile page params with minimal info.
    #[must_use]
    pub fn new(user: &'a User) -> Self {
        Self {
            user,
            message: None,
            is_error: false,
            has_forum_link: false,
        }
    }

    /// Set the message.
    #[must_use]
    pub fn with_message(mut self, message: &'a str, is_error: bool) -> Self {
        self.message = Some(message);
        self.is_error = is_error;
        self
    }

    /// Set the forum link status.
    #[must_use]
    pub fn with_forum_link(mut self, has_forum_link: bool) -> Self {
        self.has_forum_link = has_forum_link;
        self
    }
}

/// Render the profile page.
///
/// Shows the user's account status, profile update form, and password change form.
///
/// # Example
///
/// ```ignore
/// let params = ProfilePageParams::new(&user)
///     .with_forum_link(true)
///     .with_message("Profile updated!", false);
/// let page = render_profile_page(params);
/// ```
#[must_use]
pub fn render_profile_page(params: ProfilePageParams<'_>) -> Markup {
    let user = params.user;
    let display_name = user.display_name.as_deref().unwrap_or(&user.username);
    let email = user.email.as_deref().unwrap_or("");

    let content = html! {
        div style="max-width: 700px; margin: 2rem auto;" {
            h1 { "Profile" }

            // Account status box
            (render_account_status(user, params.has_forum_link))

            // Forum linking instructions (for non-linked users)
            @if !params.has_forum_link {
                (render_forum_linking_instructions(&user.username))
            }

            // Message display
            @if let Some(msg) = params.message {
                @if params.is_error {
                    (StatusBox::error("Error", msg))
                } @else {
                    (StatusBox::success("Success", msg))
                }
            }

            // Account information section
            h2 style="margin-top: var(--spacing-lg, 1.5rem);" { "Account Information" }
            div class="account-info" style="background: var(--bg-secondary, #fafafa); padding: var(--spacing-md, 1rem); border-radius: var(--radius, 0.375rem); margin-bottom: var(--spacing-lg, 1.5rem);" {
                p { strong { "Username: " } (user.username) }
                p { strong { "Account created: " } (user.created_at) }
            }

            // Profile update form
            h2 { "Update Profile" }
            (Form::post("/profile", html! {
                // Email field
                div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                    label for="email" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                        "Email (optional)"
                    }
                    (Input::email("email").id("email").value_opt(Some(email)))
                }

                // Display name field
                (render_display_name_field(display_name, params.has_forum_link))

                // Password change section
                h3 style="margin-top: var(--spacing-lg, 1.5rem); margin-bottom: var(--spacing-md, 1rem);" {
                    "Change Password"
                }

                div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                    label for="current_password" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                        "Current Password"
                    }
                    (Input::password("current_password").id("current_password"))
                }

                div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                    label for="new_password" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                        "New Password"
                    }
                    (Input::password("new_password").id("new_password"))
                }

                div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
                    label for="confirm_password" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                        "Confirm New Password"
                    }
                    (Input::password("confirm_password").id("confirm_password"))
                }

                (Button::primary("Update Profile").r#type("submit"))
            }))

            // Logout section
            div style="margin-top: var(--spacing-xl, 2rem); padding-top: var(--spacing-lg, 1.5rem); border-top: 1px solid var(--border-color, #e4e4e7);" {
                (Form::post("/logout", html! {
                    (Button::secondary("Logout").r#type("submit"))
                }))
            }
        }
    };

    BaseLayout::new("Profile")
        .with_user(Some(user))
        .render(content)
}

/// Render the account status box based on user state.
fn render_account_status(user: &User, has_forum_link: bool) -> Markup {
    if user.is_admin {
        StatusBox::success("Admin Account", "You have full administrative privileges.").render()
    } else if user.is_approved {
        if has_forum_link {
            StatusBox::success(
                "Linked Forum Account",
                "Your account is linked to your forum identity and approved.",
            )
            .render()
        } else {
            StatusBox::success("Approved Account", "You can submit links for archiving.").render()
        }
    } else {
        StatusBox::warning(
            "Pending Approval",
            "Your account is awaiting admin approval before you can submit links.",
        )
        .render()
    }
}

/// Render the forum linking instructions box.
fn render_forum_linking_instructions(username: &str) -> Markup {
    let link_command = format!("link_archive_account:{username}");

    html! {
        div class="status-box status-box-info" style="margin-top: var(--spacing-md, 1rem);" {
            strong { "Link Your Forum Account" }
            p style="margin-top: 0.5rem;" {
                "To auto-approve your account and set your display name to your forum username, post this at the very start of any forum post:"
            }
            div style="position: relative; margin: 0.75rem 0;" {
                pre id="link-command" style="background: var(--bg-primary, #ffffff); padding: 0.75rem; padding-right: 4rem; border: 1px solid var(--border-color, #e4e4e7); border-radius: var(--radius, 0.375rem); font-family: monospace; font-size: 0.875rem; overflow-x: auto; margin: 0;" {
                    (link_command)
                }
                button onclick="copyLinkCommand()" style="position: absolute; top: 0.5rem; right: 0.5rem; padding: 0.25rem 0.5rem; background: var(--bg-secondary, #f4f4f5); border: 1px solid var(--border-color, #e4e4e7); border-radius: var(--radius, 0.375rem); cursor: pointer; font-size: 0.75rem;" {
                    "Copy"
                }
            }
            p style="font-size: 0.875rem; color: var(--text-secondary, #71717a); margin-bottom: 0;" {
                "Your display name will be set to your forum username."
            }
        }
        script { (PreEscaped(COPY_LINK_COMMAND_SCRIPT)) }
    }
}

/// Render the display name field (editable or disabled based on forum link status).
fn render_display_name_field(display_name: &str, has_forum_link: bool) -> Markup {
    html! {
        div class="form-group" style="margin-bottom: var(--spacing-md, 1rem);" {
            @if has_forum_link {
                label for="display_name" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                    "Display Name"
                }
                (Input::text("display_name")
                    .id("display_name")
                    .value(display_name)
                    .disabled())
                p style="margin-top: var(--spacing-xs, 0.25rem); font-size: 0.875rem; color: var(--text-secondary, #71717a);" {
                    "Your display name is linked to your forum account and cannot be changed."
                }
            } @else {
                label for="display_name" style="display: block; margin-bottom: var(--spacing-xs, 0.25rem); font-weight: 500;" {
                    "Display Name (optional)"
                }
                (Input::text("display_name")
                    .id("display_name")
                    .value(display_name))
            }
        }
    }
}

/// Simple wrapper functions for backward compatibility.

/// Render login page (legacy API).
#[must_use]
pub fn login_page(error: Option<&str>, credentials: Option<(&str, &str)>) -> Markup {
    render_login_page(error, credentials)
}

/// Render profile page (legacy API).
#[must_use]
pub fn profile_page(user: &User) -> Markup {
    render_profile_page(ProfilePageParams::new(user))
}

/// Render profile page with message (legacy API).
#[must_use]
pub fn profile_page_with_message(user: &User, message: Option<&str>) -> Markup {
    let mut params = ProfilePageParams::new(user);
    if let Some(msg) = message {
        params = params.with_message(msg, true);
    }
    render_profile_page(params)
}

/// Render profile page with forum link status (legacy API).
#[must_use]
pub fn profile_page_with_link_status(
    user: &User,
    message: Option<&str>,
    has_forum_link: bool,
) -> Markup {
    let mut params = ProfilePageParams::new(user).with_forum_link(has_forum_link);
    if let Some(msg) = message {
        params = params.with_message(msg, true);
    }
    render_profile_page(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test user for unit tests.
    fn test_user(is_admin: bool, is_approved: bool) -> User {
        User {
            id: 1,
            username: "testuser".to_string(),
            password_hash: "hash".to_string(),
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            is_approved,
            is_admin,
            is_active: true,
            failed_login_attempts: 0,
            locked_until: None,
            password_updated_at: "2024-01-01T00:00:00Z".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_login_page_basic() {
        let page = render_login_page(None, None);
        let html = page.into_string();

        // Check page structure
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Login - Discourse Link Archiver</title>"));

        // Check login form elements
        assert!(html.contains(r#"action="/login""#));
        assert!(html.contains(r#"name="username""#));
        assert!(html.contains(r#"name="password""#));
        assert!(html.contains(r#"name="remember""#));
        assert!(html.contains(">Login<"));

        // Check register section
        assert!(html.contains("Don't have an account?"));
        assert!(html.contains(r#"value="register""#));
    }

    #[test]
    fn test_login_page_with_error() {
        let page = render_login_page(Some("Invalid credentials"), None);
        let html = page.into_string();

        assert!(html.contains("Invalid credentials"));
        assert!(html.contains(r#"class="error""#));
    }

    #[test]
    fn test_login_page_with_credentials() {
        let page = render_login_page(None, Some(("newuser", "secretpass")));
        let html = page.into_string();

        // Check credentials display
        assert!(html.contains("Account Created!"));
        assert!(html.contains("newuser"));
        assert!(html.contains("secretpass"));
        assert!(html.contains("pending admin approval"));

        // Should still have a login form
        assert!(html.contains(r#"action="/login""#));
    }

    #[test]
    fn test_profile_page_admin() {
        let user = test_user(true, true);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Check page structure
        assert!(html.contains("<title>Profile - Discourse Link Archiver</title>"));

        // Check admin status
        assert!(html.contains("Admin Account"));
        assert!(html.contains("full administrative privileges"));

        // Check navigation shows admin link
        assert!(html.contains(r#"href="/admin""#));

        // Check profile form
        assert!(html.contains(r#"action="/profile""#));
        assert!(html.contains(r#"name="email""#));
        assert!(html.contains(r#"name="display_name""#));

        // Check password change form
        assert!(html.contains("Change Password"));
        assert!(html.contains(r#"name="current_password""#));
        assert!(html.contains(r#"name="new_password""#));
        assert!(html.contains(r#"name="confirm_password""#));

        // Check logout button
        assert!(html.contains(r#"action="/logout""#));
    }

    #[test]
    fn test_profile_page_approved_user() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Check approved status
        assert!(html.contains("Approved Account"));
        assert!(html.contains("submit links for archiving"));

        // Check no admin link
        assert!(!html.contains(r#"href="/admin""#));

        // Check forum linking instructions shown
        assert!(html.contains("Link Your Forum Account"));
        assert!(html.contains("link_archive_account:testuser"));
    }

    #[test]
    fn test_profile_page_pending_user() {
        let user = test_user(false, false);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Check pending status
        assert!(html.contains("Pending Approval"));
        assert!(html.contains("awaiting admin approval"));

        // Check forum linking instructions shown
        assert!(html.contains("Link Your Forum Account"));
    }

    #[test]
    fn test_profile_page_with_forum_link() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user).with_forum_link(true);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Check linked status
        assert!(html.contains("Linked Forum Account"));
        assert!(html.contains("linked to your forum identity"));

        // Check no forum linking instructions
        assert!(!html.contains("Link Your Forum Account"));

        // Check display name is disabled
        assert!(html.contains("disabled"));
        assert!(html.contains("cannot be changed"));
    }

    #[test]
    fn test_profile_page_with_message() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user).with_message("Profile updated!", false);
        let page = render_profile_page(params);
        let html = page.into_string();

        assert!(html.contains("Profile updated!"));
        assert!(html.contains("status-box-success"));
    }

    #[test]
    fn test_profile_page_with_error_message() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user).with_message("Password mismatch", true);
        let page = render_profile_page(params);
        let html = page.into_string();

        assert!(html.contains("Password mismatch"));
        assert!(html.contains("status-box-error"));
    }

    #[test]
    fn test_profile_page_account_info() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Check account info section
        assert!(html.contains("Account Information"));
        assert!(html.contains("testuser"));
        assert!(html.contains("2024-01-01"));
    }

    #[test]
    fn test_profile_page_email_prefilled() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Email should be prefilled
        assert!(html.contains("test@example.com"));
    }

    #[test]
    fn test_profile_page_display_name_prefilled() {
        let user = test_user(false, true);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Display name should be prefilled
        assert!(html.contains("Test User"));
    }

    #[test]
    fn test_copy_link_command_script_included() {
        let user = test_user(false, false);
        let params = ProfilePageParams::new(&user);
        let page = render_profile_page(params);
        let html = page.into_string();

        // Script should be included for non-admin, non-linked users
        assert!(html.contains("copyLinkCommand"));
        assert!(html.contains("navigator.clipboard.writeText"));
    }

    #[test]
    fn test_legacy_api_login_page() {
        let page = login_page(Some("error"), None);
        let html = page.into_string();
        assert!(html.contains("error"));
    }

    #[test]
    fn test_legacy_api_profile_page() {
        let user = test_user(false, true);
        let page = profile_page(&user);
        let html = page.into_string();
        assert!(html.contains("Profile"));
    }

    #[test]
    fn test_legacy_api_profile_page_with_message() {
        let user = test_user(false, true);
        let page = profile_page_with_message(&user, Some("Updated"));
        let html = page.into_string();
        assert!(html.contains("Updated"));
    }

    #[test]
    fn test_legacy_api_profile_page_with_link_status() {
        let user = test_user(false, true);
        let page = profile_page_with_link_status(&user, None, true);
        let html = page.into_string();
        assert!(html.contains("Linked Forum Account"));
    }

    #[test]
    fn test_xss_prevention_username() {
        let page = render_login_page(None, Some(("<script>alert('xss')</script>", "pass")));
        let html = page.into_string();

        // Should be escaped
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_xss_prevention_message() {
        let page = render_login_page(Some("<script>alert('xss')</script>"), None);
        let html = page.into_string();

        // Should be escaped
        assert!(!html.contains("<script>alert"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
